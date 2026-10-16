use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, StatefulWidget, Tabs, Widget, Wrap};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::chart::{
    ChartState, PricesCandlestickChart, PricesKagiChart, PricesLineChart, VolumeBarChart, XAxis,
};
use super::chart_configuration::ChartConfigurationState;
use super::{block, CachableWidget, CacheState, OptionsState};
use crate::api::model::{ChartMeta, CompanyData};
use crate::api::AssetClass;
use crate::common::*;
use crate::draw::{add_padding, PaddingDirection};
use crate::service::{self, Service};
use crate::task::{MarketState, Offer};
use crate::theme::style;
use crate::{
    DEFAULT_TIMESTAMPS, ENABLE_PRE_POST, HIDE_PREV_CLOSE, HIDE_TOGGLE, OPTS, SHOW_VOLUMES,
    SHOW_X_LABELS, THEME, TIME_FRAME, TRUNC_PRE, UPDATE_INTERVAL,
};

const NUM_LOADING_TICKS: usize = 8;

/// How far wicks can extend a candlestick chart beyond the candle bodies, as a
/// fraction of the bodies' range. Longer wicks are clipped.
const MAX_WICK_EXTENSION: f64 = 0.25;
const ICON_LOADING_TICKS: [char; 8] = ['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];

pub struct StockState {
    pub symbol: String,
    pub chart_type: ChartType,
    pub stock_service: service::stock::StockService,
    pub profile: Option<CompanyData>,
    pub current_regular_price: f64,
    pub current_extended_price: Option<f64>,
    /// Price today's change is measured against, from the quote
    pub day_change_baseline: Option<f64>,
    pub prev_close_price: Option<f64>,
    pub reg_mkt_volume: Option<String>,
    /// From Nasdaq, only for US listings during pre-market
    pub pre_market_volume: Option<u64>,
    pub bid: Option<Offer>,
    pub ask: Option<Offer>,
    pub market_state: Option<MarketState>,
    pub prices: [Vec<Price>; 7],
    pub time_frame: TimeFrame,
    pub show_options: bool,
    pub show_configure: bool,
    pub options: Option<OptionsState>,
    pub chart_configuration: ChartConfigurationState,
    pub loading_tick: usize,
    pub prev_state_loaded: bool,
    pub chart_meta: Option<ChartMeta>,
    pub chart_state: Option<ChartState>,
    pub cache_state: CacheState,
    pub current_price_error: Option<String>,
    pub prices_error: Option<String>,
    /// When the last quote was received
    pub quote_received: Option<(Instant, DateTime<Local>)>,
}

impl Hash for StockState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.symbol.hash(state);
        self.chart_type.hash(state);
        self.current_regular_price.to_bits().hash(state);
        // Only fetched once, so just need to check if Some
        self.profile.is_some().hash(state);
        self.current_extended_price.map(|f| f.to_bits()).hash(state);
        self.day_change_baseline.map(|f| f.to_bits()).hash(state);
        self.prev_close_price.map(|f| f.to_bits()).hash(state);
        self.reg_mkt_volume.hash(state);
        self.pre_market_volume.hash(state);
        self.bid.hash(state);
        self.ask.hash(state);
        self.market_state.hash(state);
        self.prices.hash(state);
        self.time_frame.hash(state);
        self.show_options.hash(state);
        self.show_configure.hash(state);
        self.chart_configuration.hash(state);
        self.loading_tick.hash(state);
        self.prev_state_loaded.hash(state);
        self.chart_meta.hash(state);
        self.current_price_error.hash(state);
        self.prices_error.hash(state);
        self.stale_since().hash(state);

        if let Some(chart_state) = self.chart_state.as_ref() {
            chart_state.hash(state);
        }

        // Hash globals since they affect "state" of how widget is rendered
        DEFAULT_TIMESTAMPS.read().get(&self.time_frame).hash(state);
        ENABLE_PRE_POST.read().hash(state);
        HIDE_PREV_CLOSE.hash(state);
        HIDE_TOGGLE.hash(state);
        SHOW_VOLUMES.read().hash(state);
        SHOW_X_LABELS.read().hash(state);
        TRUNC_PRE.hash(state);
    }
}

impl StockState {
    pub fn new(symbol: String, chart_type: ChartType) -> StockState {
        let time_frame = *TIME_FRAME;

        let stock_service = service::stock::StockService::new(symbol.clone(), time_frame);
        let kagi_options = OPTS.kagi_options.get(&symbol).cloned().unwrap_or_default();

        StockState {
            symbol,
            chart_type,
            stock_service,
            profile: None,
            current_regular_price: 0.0,
            current_extended_price: None,
            day_change_baseline: None,
            prev_close_price: None,
            reg_mkt_volume: None,
            pre_market_volume: None,
            bid: None,
            ask: None,
            market_state: None,
            prices: [vec![], vec![], vec![], vec![], vec![], vec![], vec![]],
            time_frame,
            show_options: false,
            show_configure: false,
            options: None,
            chart_configuration: ChartConfigurationState {
                kagi_options,
                ..Default::default()
            },
            loading_tick: 0,
            prev_state_loaded: false,
            chart_meta: None,
            cache_state: Default::default(),
            current_price_error: None,
            prices_error: None,
            chart_state: None,
            quote_received: None,
        }
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub fn set_time_frame(&mut self, time_frame: TimeFrame) {
        self.time_frame = time_frame;

        self.stock_service.update_time_frame(time_frame);
        self.prices_error = None;

        // Resets chart state where applicable
        self.set_chart_type(self.chart_type);
    }

    // Get the actual start and end times used for building the chart
    fn adjusted_start_end(&self, prices: &[Price]) -> (i64, i64) {
        let (mut start, mut end) = self.current_trading_start_end();

        // We can't use current trading directly for non-1D. We
        // need to find the start / end for the timeframe which
        // is effectively min(start, normalized_spy_proxy_start)..end
        if !matches!(self.time_frame, TimeFrame::Day1) {
            let default_start = DEFAULT_TIMESTAMPS
                .read()
                .get(&self.time_frame)
                .and_then(|timestamps| timestamps.first().copied())
                .unwrap_or(start);
            let default_start_date = default_start - default_start % DAY;

            let start_time = start % DAY;

            start = default_start_date + start_time;

            if let Some(first_price) = prices.first() {
                start = start.min(first_price.date);
            }

            if let Some(last_price) = prices.last() {
                end = last_price.date;
            }
        }

        (start, end)
    }

    fn get_matching_prices(&self, prices: &[Price]) -> Vec<Price> {
        if prices.is_empty() {
            return vec![];
        }

        // Crypto doesn't need price matching
        if self.is_crypto() {
            return prices.to_vec();
        }

        let (adj_start, adj_end) = self.adjusted_start_end(prices);

        if matches!(self.time_frame, TimeFrame::Day1) {
            prices
                .iter()
                // Needed to filter out pre / post market data if pre / post is disabled
                .filter(|p| p.date >= adj_start && p.date < adj_end)
                .copied()
                .collect::<Vec<_>>()
        } else {
            let start = prices.first().unwrap().date;
            let end = prices.last().unwrap().date;

            let delta = self.time_frame.round_by();

            let utc_offset = self
                .chart_meta
                .as_ref()
                .and_then(|m| m.gmtoffset)
                .unwrap_or_default();

            let fill_before = MarketTimes::new(adj_start, start, delta, utc_offset);
            let fill_after = MarketTimes::new(end, adj_end, delta, utc_offset);

            fill_before
                .map(Price::new)
                .chain(prices.iter().copied())
                .chain(fill_after.map(Price::new))
                .collect()
        }
    }

    pub fn prices(&self) -> impl Iterator<Item = Price> {
        self.get_matching_prices(&self.prices[self.time_frame.idx()])
            .into_iter()
    }

    pub fn current_price(&self) -> f64 {
        let enable_pre_post = { *ENABLE_PRE_POST.read() };

        match self.current_extended_price {
            Some(price) if enable_pre_post => price,
            _ => self.current_regular_price,
        }
    }

    pub fn update(&mut self) {
        let updates = self.stock_service.updates();

        for update in updates {
            match update {
                service::stock::Update::NewPrice(quote) => {
                    self.quote_received = Some((Instant::now(), Local::now()));
                    self.current_price_error = None;
                    self.current_regular_price = quote.regular_price;
                    self.current_extended_price = quote.extended_price;
                    self.day_change_baseline = Some(quote.day_change_baseline);
                    self.reg_mkt_volume = Some(quote.volume);
                    self.bid = quote.bid;
                    self.ask = quote.ask;
                    self.market_state = quote.market_state;
                }
                service::stock::Update::Prices((time_frame, chart_meta, prices)) => {
                    self.prices_error = None;
                    self.prices[time_frame.idx()] = prices;

                    if time_frame == TimeFrame::Day1 {
                        self.prev_close_price = Some(session_previous_close(&chart_meta));
                    }

                    let instrument_type = chart_meta.instrument_type.as_deref();
                    if let Some(asset_class) = nasdaq_asset_class(&self.symbol, instrument_type) {
                        self.stock_service.track_pre_market_volume(asset_class);
                    }

                    self.chart_meta = Some(chart_meta);
                }
                service::stock::Update::PreMarketVolume(volume) => {
                    self.pre_market_volume = volume;
                }
                service::stock::Update::CompanyData(data) => {
                    self.profile = Some(*data);
                }
                service::stock::Update::CurrentPriceError(error) => {
                    self.current_price_error = Some(error);
                }
                service::stock::Update::PricesError(error) => {
                    self.prices_error = Some(error);
                }
            }
        }
    }

    fn options_enabled(&self) -> bool {
        !self.is_crypto() && !self.is_index()
    }

    fn configure_enabled(&self) -> bool {
        self.chart_type == ChartType::Kagi
    }

    fn is_crypto(&self) -> bool {
        self.chart_meta
            .as_ref()
            .and_then(|m| m.instrument_type.as_deref())
            == Some("CRYPTOCURRENCY")
    }

    fn is_index(&self) -> bool {
        self.chart_meta
            .as_ref()
            .and_then(|m| m.instrument_type.as_deref())
            == Some("INDEX")
    }

    pub fn toggle_options(&mut self) -> bool {
        if !self.options_enabled() {
            return false;
        }

        self.show_options = !self.show_options;

        if self.options.is_some() {
            self.options.take();
        } else {
            self.options = Some(OptionsState::new(self.symbol.clone()));
        }

        true
    }

    pub fn toggle_configure(&mut self) -> bool {
        if !self.configure_enabled() {
            return false;
        }

        self.show_configure = !self.show_configure;

        self.chart_configuration.reset_form(self.time_frame);

        true
    }

    pub fn current_trading_start_end(&self) -> (i64, i64) {
        let enable_pre_post = { *ENABLE_PRE_POST.read() };

        let pre = self
            .chart_meta
            .as_ref()
            .and_then(|c| c.current_trading_period.as_ref())
            .map(|p| &p.pre);

        let regular = self
            .chart_meta
            .as_ref()
            .and_then(|c| c.current_trading_period.as_ref())
            .map(|p| &p.regular);

        let post = self
            .chart_meta
            .as_ref()
            .and_then(|c| c.current_trading_period.as_ref())
            .map(|p| &p.post);

        let mut pre_start = pre.map(|p| p.start).unwrap_or(32400);
        let reg_start = regular.map(|p| p.start).unwrap_or(52200);
        let reg_end = regular.map(|p| p.end).unwrap_or(75600);
        let post_end = post.map(|p| p.end).unwrap_or(90000);

        // Pre market really only has activity 30 min before open
        if reg_start - pre_start >= 1800 && *TRUNC_PRE {
            pre_start = reg_start - 1800;
        }

        let start = if matches!(self.time_frame, TimeFrame::Day1) && enable_pre_post {
            pre_start
        } else {
            reg_start
        };

        let end = if matches!(self.time_frame, TimeFrame::Day1) && enable_pre_post {
            post_end
        } else {
            reg_end
        };

        (start, end)
    }

    /// Open and close of the regular session, on 1D when pre and post market
    /// are shown
    pub fn regular_session(&self) -> Option<(i64, i64)> {
        if self.time_frame != TimeFrame::Day1 || !*ENABLE_PRE_POST.read() {
            return None;
        }

        let regular = &self
            .chart_meta
            .as_ref()?
            .current_trading_period
            .as_ref()?
            .regular;

        Some((regular.start, regular.end))
    }

    pub fn regular_start_end_idx(&self, data: &[Price]) -> (Option<usize>, Option<usize>) {
        let reg_start = self
            .chart_meta
            .as_ref()
            .and_then(|m| m.current_trading_period.as_ref())
            .map(|c| c.regular.start);

        let reg_end = self
            .chart_meta
            .as_ref()
            .and_then(|m| m.current_trading_period.as_ref())
            // Last data point is always 1 minute before "end" time
            .map(|c| c.regular.end - 60);

        let start_idx = data
            .iter()
            .enumerate()
            .find(|(_, p)| Some(p.date) >= reg_start)
            .map(|(idx, _)| idx);

        let end_idx = data
            .iter()
            .enumerate()
            .find(|(_, p)| Some(p.date) >= reg_end)
            .map(|(idx, _)| idx);

        (start_idx, end_idx)
    }

    pub fn current_trading_period(&self, data: &[Price]) -> TradingPeriod {
        let (reg_start, reg_end) = self.regular_start_end_idx(data);

        if self.time_frame != TimeFrame::Day1 {
            TradingPeriod::Regular
        } else if reg_start.is_some() && reg_end.is_some() {
            TradingPeriod::Post
        } else if reg_start.is_some() {
            TradingPeriod::Regular
        } else {
            TradingPeriod::Pre
        }
    }

    /// Prices from min to max that a chart of `data` shows. The prices drawn
    /// (closes, and opens for candles), the current price and the previous
    /// close are always shown. With `wicks`, highs and lows are too, unless
    /// they'd extend the range by more than `MAX_WICK_EXTENSION`, so a single
    /// spike doesn't squash the chart.
    pub fn chart_bounds(&self, data: &[Price], wicks: bool) -> (f64, f64) {
        let prev_close = self
            .prev_close_price
            .filter(|_| self.time_frame == TimeFrame::Day1 && !*HIDE_PREV_CLOSE);

        let bodies = data
            .iter()
            .filter(|p| p.close > 0.0)
            .flat_map(|p| [p.close, if wicks { p.open } else { p.close }])
            .chain([self.current_price()])
            .chain(prev_close)
            .filter(|price| *price > 0.0);

        let (high, low) = self.high_low(data);
        let wick_bounds = (
            low.min(prev_close.unwrap_or(low)),
            high.max(prev_close.unwrap_or(high)),
        );

        match min_max(bodies) {
            Some((min, max)) if wicks => clip_wicks((min, max), wick_bounds),
            Some(bounds) => bounds,
            None => wick_bounds,
        }
    }

    pub fn high_low(&self, data: &[Price]) -> (f64, f64) {
        let mut data = data.to_vec();
        data.push(Price {
            close: self.current_price(),
            open: self.current_price(),
            high: self.current_price(),
            low: self.current_price(),
            ..Default::default()
        });
        data.retain(|p| p.close.gt(&0.0));

        let high = data
            .iter()
            .max_by(|a, b| a.high.partial_cmp(&b.high).unwrap())
            .map(|p| p.high)
            .unwrap_or(1.0);
        let low = data
            .iter()
            .min_by(|a, b| a.low.partial_cmp(&b.low).unwrap())
            .map(|p| p.low)
            .unwrap_or(0.0);

        (high, low)
    }

    pub fn y_bounds(&self, min: f64, max: f64) -> [f64; 2] {
        [(min), (max)]
    }

    /// Price the change is measured against: the previous close on 1D, so
    /// there's a change even before the session has any prices, otherwise
    /// the first price of the time frame
    fn change_baseline(&self, data: &[Price]) -> Option<f64> {
        let first_price = data.iter().find(|p| p.close > 0.0).map(|p| p.close);

        if self.time_frame == TimeFrame::Day1 {
            self.prev_close_price.or(first_price)
        } else {
            first_price
        }
    }

    pub fn pct_change(&self, data: &[Price]) -> f64 {
        self.change_baseline(data)
            .map_or(0.0, |baseline| self.current_price() / baseline - 1.0)
    }

    pub fn dollar_change(&self, data: &[Price]) -> f64 {
        self.change_baseline(data)
            .map_or(0.0, |baseline| self.current_price() - baseline)
    }

    /// Volume of the current session. Yahoo has the previous session's volume
    /// until the market opens, so the pre-market volume comes from Nasdaq, or
    /// the previous volume is marked as such when Nasdaq doesn't have it
    pub fn volume(&self) -> Option<String> {
        match (self.market_state, self.pre_market_volume) {
            (Some(MarketState::Pre), Some(volume)) => Some(format_volume(volume)),
            (Some(MarketState::Pre), None) => self
                .reg_mkt_volume
                .as_ref()
                .map(|volume| format!("{} (prev)", volume)),
            _ => self.reg_mkt_volume.clone(),
        }
    }

    /// Local time of the last quote, if no quote has been received since for
    /// too long, e.g. because Yahoo can't be reached
    pub fn stale_since(&self) -> Option<DateTime<Local>> {
        let (received, time) = self.quote_received?;

        (received.elapsed() >= stale_after(*UPDATE_INTERVAL)).then_some(time)
    }

    /// Today's change, whatever the time frame
    pub fn day_pct_change(&self) -> Option<f64> {
        let baseline = self.day_change_baseline.filter(|price| *price > 0.0)?;
        let current = Some(self.current_price()).filter(|price| *price > 0.0)?;

        Some(current / baseline - 1.0)
    }

    /// The most recent error preventing the stock from loading, if any
    pub fn error(&self) -> Option<&str> {
        self.prices_error
            .as_deref()
            .or(self.current_price_error.as_deref())
    }

    pub fn loaded(&self) -> bool {
        !self.prices[self.time_frame.idx()].is_empty() && self.current_price() > 0.0
    }

    pub fn loading_tick(&mut self) {
        let loaded = self.loaded();

        if !loaded {
            // Reset tick
            if self.prev_state_loaded {
                self.prev_state_loaded = false;
                self.loading_tick = 0;
            }

            self.loading_tick = (self.loading_tick + 1) % NUM_LOADING_TICKS;
        } else if !self.prev_state_loaded {
            self.prev_state_loaded = true;
        }
    }

    pub fn set_chart_type(&mut self, chart_type: ChartType) {
        self.chart_state.take();

        if chart_type == ChartType::Kagi {
            self.chart_state = Some(Default::default());
        }

        self.chart_type = chart_type;
    }

    pub fn chart_state_mut(&mut self) -> Option<&mut ChartState> {
        self.chart_state.as_mut()
    }

    pub fn chart_config_mut(&mut self) -> &mut ChartConfigurationState {
        &mut self.chart_configuration
    }
}

pub struct StockWidget {}

impl StatefulWidget for StockWidget {
    type State = StockState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        self.render_cached(area, buf, state);
    }
}

impl CachableWidget<StockState> for StockWidget {
    fn cache_state_mut(state: &mut StockState) -> &mut CacheState {
        &mut state.cache_state
    }

    fn render(self, mut area: Rect, buf: &mut Buffer, state: &mut <Self as StatefulWidget>::State) {
        let data = state.prices().collect::<Vec<_>>();
        let x_axis = XAxis::new(state, &data);

        let pct_change = state.pct_change(&data);
        let dollar_change = state.dollar_change(&data);

        let chart_type = state.chart_type;
        let show_x_labels = *SHOW_X_LABELS.read();
        let enable_pre_post = *ENABLE_PRE_POST.read();
        let show_volumes = *SHOW_VOLUMES.read() && chart_type != ChartType::Kagi;

        let loaded = state.loaded();

        let currency = match state.profile.as_ref() {
            Some(profile) => profile.price.currency.as_deref().unwrap_or("USD"),
            None => "",
        };

        let title = get_chart_title(&area, state, false);

        // Draw widget block
        {
            let mut block = block::new(&title);
            if let Some(stale) = stale_title(state) {
                block = block.title(stale);
            }
            block.render(area, buf);
            area = add_padding(area, 1, PaddingDirection::All);
            area = add_padding(area, 1, PaddingDirection::Left);
            area = add_padding(area, 1, PaddingDirection::Right);
        }

        // chunks[0] - Company Info
        // chunks[1] - Graph - fill remaining space
        // chunks[2] - Time Frame Tabs
        let mut chunks: Vec<Rect> = Layout::default()
            .constraints(
                [
                    Constraint::Length(9),
                    Constraint::Min(0),
                    Constraint::Length(2),
                ]
                .as_ref(),
            )
            .split(area)
            .to_vec();

        // Draw company info
        {
            // Check if portfolio is configured and has this ticker
            let show_portfolio = crate::OPTS
                .portfolio
                .as_ref()
                .and_then(|p| p.items.get(&state.symbol));

            // info_chunks[0] - Prices / volumes
            // info_chunks[1] - Portfolio info, if shown
            let mut info_chunks: Vec<Rect> = if show_portfolio.is_some() {
                Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Min(23), Constraint::Length(25)].as_ref())
                    .split(chunks[0])
                    .to_vec()
            } else {
                vec![chunks[0]]
            };

            info_chunks[0] = add_padding(info_chunks[0], 1, PaddingDirection::Top);

            let (high, low) = state.high_low(&data);
            let current_fmt = format_price(state.current_price());
            let high_fmt = format_price(high);
            let low_fmt = format_price(low);

            // Right align the prices to the widest, so they line up in a column
            let price_width = [&current_fmt, &high_fmt, &low_fmt]
                .iter()
                .map(|p| p.len())
                .max()
                .unwrap_or_default();

            let (high_pct, low_pct) = if state.time_frame == TimeFrame::Day1 {
                if let Some(prev_close) = state.prev_close_price {
                    (Some(high / prev_close - 1.0), Some(low / prev_close - 1.0))
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };

            let vol = state.volume().unwrap_or_default();

            let (market_cap, avg_volume) = match state.profile.as_ref() {
                Some(profile) => {
                    let mc = profile
                        .summary_detail
                        .as_ref()
                        .and_then(|sd| sd.market_cap.as_ref())
                        .and_then(|mc| mc.fmt.clone());
                    let av = profile
                        .summary_detail
                        .as_ref()
                        .and_then(|sd| sd.average_volume.as_ref())
                        .and_then(|av| av.fmt.clone());
                    (mc, av)
                }
                None => (None, None),
            };

            let company_info = vec![
                Line::from(vec![
                    Span::styled("Last: ", label_style()),
                    Span::styled(
                        if loaded {
                            format_money(currency, &current_fmt, price_width)
                        } else {
                            "".to_string()
                        },
                        style()
                            .add_modifier(Modifier::BOLD)
                            .fg(THEME.text_primary()),
                    ),
                    Span::styled(
                        if loaded {
                            format!(
                                "{:>8.2}% ({}{:.*})",
                                pct_change * 100.0,
                                if dollar_change >= 0.0 { "+" } else { "" },
                                price_decimals(state.current_price()),
                                dollar_change,
                            )
                        } else {
                            "".to_string()
                        },
                        style()
                            .add_modifier(Modifier::BOLD)
                            .fg(if pct_change >= 0.0 {
                                THEME.profit()
                            } else {
                                THEME.loss()
                            }),
                    ),
                    market_state_span(state.market_state.filter(|_| loaded)),
                ]),
                Line::from(vec![
                    Span::styled("High: ", label_style()),
                    Span::styled(
                        if loaded {
                            format_money(currency, &high_fmt, price_width)
                        } else {
                            "".to_string()
                        },
                        style().fg(THEME.text_normal()),
                    ),
                    Span::styled(
                        if loaded {
                            high_pct.map_or("".to_string(), |p| {
                                format!("{:>9}", format!("{:.2}%", p * 100.0))
                            })
                        } else {
                            "".to_string()
                        },
                        style().add_modifier(Modifier::BOLD).fg(
                            if high_pct.unwrap_or(0.0) >= 0.0 {
                                THEME.profit()
                            } else {
                                THEME.loss()
                            },
                        ),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Low:  ", label_style()),
                    Span::styled(
                        if loaded {
                            format_money(currency, &low_fmt, price_width)
                        } else {
                            "".to_string()
                        },
                        style().fg(THEME.text_normal()),
                    ),
                    Span::styled(
                        if loaded {
                            low_pct.map_or("".to_string(), |p| {
                                format!("{:>9}", format!("{:.2}%", p * 100.0))
                            })
                        } else {
                            "".to_string()
                        },
                        style()
                            .add_modifier(Modifier::BOLD)
                            .fg(if low_pct.unwrap_or(0.0) >= 0.0 {
                                THEME.profit()
                            } else {
                                THEME.loss()
                            }),
                    ),
                ]),
                offers_line(state.bid.as_ref(), state.ask.as_ref(), currency, loaded),
                Line::default(),
                Line::from(vec![
                    Span::styled("Volume:  ", label_style()),
                    Span::styled(
                        if loaded { vol } else { "".to_string() },
                        style().fg(THEME.text_normal()),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Avg Vol: ", label_style()),
                    Span::styled(
                        if loaded {
                            avg_volume.unwrap_or_default()
                        } else {
                            "".to_string()
                        },
                        style().fg(THEME.text_normal()),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Mkt Cap: ", label_style()),
                    Span::styled(
                        if loaded {
                            market_cap.unwrap_or_default()
                        } else {
                            "".to_string()
                        },
                        style().fg(THEME.text_normal()),
                    ),
                ]),
            ];

            Paragraph::new(company_info)
                .style(style().fg(THEME.text_normal()))
                .alignment(Alignment::Left)
                .wrap(Wrap { trim: true })
                .render(info_chunks[0], buf);

            // Render portfolio info if available
            if let Some(portfolio_item) = show_portfolio {
                let mut portfolio_area = info_chunks[1];
                portfolio_area = add_padding(portfolio_area, 1, PaddingDirection::Top);

                let (profit_loss, profit_loss_pct) =
                    portfolio_item.calculate_ticker_profit_loss(state.current_price());

                let profit_loss_color = if profit_loss >= 0.0 {
                    THEME.profit()
                } else {
                    THEME.loss()
                };

                let portfolio_info = vec![
                    Line::from(vec![
                        Span::styled("Qty: ", label_style()),
                        Span::styled(
                            format_decimals(portfolio_item.quantity),
                            style().fg(THEME.text_normal()),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("Avg: ", label_style()),
                        Span::styled(
                            format_price(portfolio_item.average_price),
                            style().fg(THEME.text_normal()),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("P/L: ", label_style()),
                        Span::styled(
                            format_price(profit_loss),
                            style().add_modifier(Modifier::BOLD).fg(profit_loss_color),
                        ),
                        Span::styled(
                            format!("  {:.2}%", profit_loss_pct),
                            style().add_modifier(Modifier::BOLD).fg(profit_loss_color),
                        ),
                    ]),
                ];

                Paragraph::new(portfolio_info)
                    .style(style().fg(THEME.text_normal()))
                    .alignment(Alignment::Left)
                    .wrap(Wrap { trim: true })
                    .render(portfolio_area, buf);
            }
        }

        // graph_chunks[0] = prices
        // graph_chunks[1] = volume, with a blank row above it and the time
        // labels below it
        let graph_chunks: Vec<Rect> = if show_volumes {
            let volume_rows = (chunks[1].height / 6).clamp(3, 6);
            let volume_height = 1 + volume_rows + if show_x_labels { 2 } else { 0 };

            let mut graph_chunks = Layout::default()
                .constraints([Constraint::Min(0), Constraint::Length(volume_height)].as_ref())
                .split(chunks[1])
                .to_vec();
            graph_chunks[1] = add_padding(graph_chunks[1], 1, PaddingDirection::Top);

            graph_chunks
        } else {
            Layout::default()
                .constraints([Constraint::Min(0)].as_ref())
                .split(chunks[1])
                .to_vec()
        };

        // Draw prices line chart
        match chart_type {
            ChartType::Line => {
                PricesLineChart {
                    data: &data,
                    x_axis: &x_axis,
                    enable_pre_post,
                    is_profit: pct_change >= 0.0,
                    is_summary: false,
                    loaded,
                    show_x_labels: show_x_labels && !show_volumes,
                    show_y_labels: true,
                }
                .render(graph_chunks[0], buf, state);
            }
            ChartType::Candlestick => {
                PricesCandlestickChart {
                    data: &data,
                    x_axis: &x_axis,
                    loaded,
                    show_x_labels: show_x_labels && !show_volumes,
                    show_y_labels: true,
                    is_summary: false,
                }
                .render(graph_chunks[0], buf, state);
            }
            ChartType::Kagi => {
                PricesKagiChart {
                    data: &data,
                    loaded,
                    show_x_labels: show_x_labels && !show_volumes,
                    show_y_labels: true,
                    is_summary: false,
                    kagi_options: state.chart_configuration.kagi_options.clone(),
                }
                .render(graph_chunks[0], buf, state);
            }
        }

        // Show why the chart isn't loading instead of only a spinner
        if !loaded {
            if let Some(error) = state.error() {
                let mut error_area = add_padding(graph_chunks[0], 1, PaddingDirection::Top);
                error_area = add_padding(error_area, 10, PaddingDirection::Left);

                render_error(error_area, buf, error);
            }
        }

        // Draw volumes bar chart
        if show_volumes {
            VolumeBarChart {
                data: &data,
                x_axis: &x_axis,
                show_x_labels,
            }
            .render(graph_chunks[1], buf, state);
        }

        // Draw time frame tabs & optional chart scroll indicators
        {
            Block::default()
                .borders(Borders::TOP)
                .border_style(style().fg(THEME.border_secondary()))
                .render(chunks[2], buf);
            chunks[2] = add_padding(chunks[2], 1, PaddingDirection::Top);

            let tab_names = TimeFrame::tab_names();

            // Each tab is padded by a space on both sides, with a divider between tabs
            let tabs_width = tab_names.iter().map(|name| name.width() + 2).sum::<usize>()
                + tab_names.len().saturating_sub(1);

            // layout[0] - timeframe
            // layout[1] - toggles
            // layout[2] - scroll indicators
            let layout: Vec<Rect> = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(tabs_width as u16),
                    Constraint::Min(0),
                    Constraint::Length(if state.chart_state.is_some() { 3 } else { 0 }),
                ])
                .split(chunks[2])
                .to_vec();

            let tab_names: Vec<Line> = tab_names.iter().map(|s| Line::from(*s)).collect();

            Tabs::new(tab_names)
                .select(state.time_frame.idx())
                .style(style().fg(THEME.text_secondary()))
                .highlight_style(style().fg(THEME.text_primary()))
                .render(layout[0], buf);

            if !*HIDE_TOGGLE && loaded {
                let toggles = [
                    Some((format!("{} 'c'", chart_type.as_str()), false)),
                    (chart_type != ChartType::Kagi).then(|| ("Volumes 'v'".into(), show_volumes)),
                    Some(("Date 'x'".into(), show_x_labels)),
                    Some(("Pre/Post 'p'".into(), enable_pre_post)),
                    state
                        .configure_enabled()
                        .then(|| ("Edit 'e'".into(), state.show_configure)),
                    state
                        .options_enabled()
                        .then(|| ("Options 'o'".into(), state.show_options)),
                    Some(("Summary 's'".into(), false)),
                ];

                // Keep a gap after the time frame tabs
                let width = layout[1].width.saturating_sub(2);

                Paragraph::new(toggles_line(toggles.into_iter().flatten(), width))
                    .alignment(Alignment::Right)
                    .render(layout[1], buf);
            }

            if let Some(chart_state) = state.chart_state.as_ref() {
                let more_left = chart_state.offset.unwrap_or_default()
                    < chart_state.max_offset.unwrap_or_default();
                let more_right = chart_state.offset.is_some();

                let left_arrow = Span::styled(
                    "ᐸ",
                    style().fg(if more_left {
                        THEME.text_normal()
                    } else {
                        THEME.gray()
                    }),
                );
                let right_arrow = Span::styled(
                    "ᐳ",
                    style().fg(if more_right {
                        THEME.text_normal()
                    } else {
                        THEME.gray()
                    }),
                );

                Paragraph::new(Line::from(vec![left_arrow, Span::raw(" "), right_arrow]))
                    .render(layout[2], buf);
            }
        }
    }
}

pub fn render_error(area: Rect, buf: &mut Buffer, error: &str) {
    Paragraph::new(error)
        .style(style().fg(THEME.loss()))
        .wrap(Wrap { trim: true })
        .render(area, buf);
}

pub fn get_chart_title(
    area: &Rect,
    state: &<StockWidget as StatefulWidget>::State,
    summary: bool,
) -> String {
    let company_name = match state.profile.as_ref() {
        Some(profile) => profile.price.short_name.as_str(),
        None => "",
    };

    // Add the company name if it is available
    let mut title = match state.profile {
        Some(_) => format!("{} - {}", state.symbol, company_name),
        None => state.symbol.clone(),
    };

    // Take the loading indicator and stale label into account for the truncation
    // calculation
    let loading_indicator_overhead = !state.loaded() as usize * 2;
    let stale_overhead = stale_label(state).map_or(0, |label| label.width() + 1);
    // Summary has no outer block so its only our title padding of 2, otherwise
    // we must add the outer block padding of 2 as well
    let padding = if summary { 2 } else { 4 };

    // Constraint the title length to the screen area less padding & dots if it is truncated
    let max_width =
        (area.width as usize).saturating_sub(padding + loading_indicator_overhead + stale_overhead);
    title = truncate_title(&title, max_width);

    // Add padding and the loading indicator
    title = match state.loaded() {
        true => format!(" {} ", title),
        false => format!(" {} {} ", title, ICON_LOADING_TICKS[state.loading_tick]),
    };

    title
}

/// Smallest and largest of `prices`
fn min_max(prices: impl Iterator<Item = f64>) -> Option<(f64, f64)> {
    prices.fold(None, |bounds, price| match bounds {
        None => Some((price, price)),
        Some((min, max)) => Some((min.min(price), max.max(price))),
    })
}

/// Bounds of the candle `bodies` (min, max), extended to the `wicks`' bounds
/// by at most `MAX_WICK_EXTENSION` of the bodies' range on each side
fn clip_wicks(bodies: (f64, f64), wicks: (f64, f64)) -> (f64, f64) {
    let extension = (bodies.1 - bodies.0) * MAX_WICK_EXTENSION;

    // Nothing to measure wicks against
    if extension <= 0.0 {
        return (wicks.0.min(bodies.0), wicks.1.max(bodies.1));
    }

    (
        wicks.0.max(bodies.0 - extension).min(bodies.0),
        wicks.1.min(bodies.1 + extension).max(bodies.1),
    )
}

/// How long without a new quote before it's stale. Longer than the background
/// update interval, so switching to a tab doesn't flash it.
fn stale_after(update_interval: u64) -> Duration {
    Duration::from_secs(update_interval.saturating_mul(3).max(20))
}

/// Label of a stale quote, e.g. " No updates since 10:32:05 "
fn stale_label(state: &StockState) -> Option<String> {
    let since = state.stale_since()?;

    Some(format!(" No updates since {} ", since.format("%H:%M:%S")))
}

/// Right aligned block title marking a stale quote
pub fn stale_title<'a>(state: &StockState) -> Option<Line<'a>> {
    let label = stale_label(state)?;

    Some(Line::from(Span::styled(label, style().fg(THEME.loss()))).alignment(Alignment::Right))
}

/// Truncates `title` to `max_width` display columns, ending it with an elipsis
/// if it doesn't fit. Works on whole characters, so multi-byte and wide
/// characters (e.g. "Nestlé", CJK names) are never split.
fn truncate_title(title: &str, max_width: usize) -> String {
    if title.width() <= max_width {
        return title.to_string();
    }

    let available = max_width.saturating_sub(3);

    let mut truncated = String::new();
    let mut truncated_width = 0;
    for c in title.chars() {
        let char_width = c.width().unwrap_or(0);

        if truncated_width + char_width > available {
            break;
        }

        truncated.push(c);
        truncated_width += char_width;
    }

    let trimmed = truncated.trim_end();

    // Move trimmed spaces, if any, after elipsis so we still fill
    // the total space (prevents exposing border after elipsis)
    let fill = " ".repeat(available - trimmed.width());

    format!("{trimmed}...{fill}")
}

/// The toggles, as their label and whether they're on, that fit in `width`.
/// Toggles that don't fit are left out, starting from the last one.
fn toggles_line<'a>(toggles: impl Iterator<Item = (String, bool)>, width: u16) -> Line<'a> {
    let mut spans = vec![];
    let mut used = 0;

    for (label, on) in toggles {
        let gap = if spans.is_empty() { 0 } else { 2 };
        let label_width = label.width() + gap;

        if used + label_width > width as usize {
            break;
        }
        used += label_width;

        if gap > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            label,
            style().fg(THEME.text_normal()).bg(if on {
                THEME.highlight_unfocused()
            } else {
                THEME.background()
            }),
        ));
    }

    Line::from(spans)
}

/// Nasdaq asset class of a Yahoo symbol, if Nasdaq has data for it (US stocks
/// and ETFs)
fn nasdaq_asset_class(symbol: &str, instrument_type: Option<&str>) -> Option<AssetClass> {
    // Non US listings have an exchange suffix, e.g. SIVE.ST
    if symbol.contains('.') {
        return None;
    }

    match instrument_type? {
        "EQUITY" => Some(AssetClass::Stocks),
        "ETF" => Some(AssetClass::Etf),
        _ => None,
    }
}

/// Which session the current price is from, e.g. "Pre-market"
fn market_state_span<'a>(market_state: Option<MarketState>) -> Span<'a> {
    match market_state {
        Some(market_state) => Span::styled(
            format!("  {}", market_state.as_str()),
            style().fg(if market_state == MarketState::Closed {
                THEME.gray()
            } else {
                THEME.text_secondary()
            }),
        ),
        None => Span::raw(""),
    }
}

/// Style of the labels in front of values, e.g. "High:"
pub fn label_style() -> Style {
    style().fg(THEME.gray())
}

/// Difference between the `ask` and `bid` prices, and as a fraction of their
/// midpoint
fn spread(bid: f64, ask: f64) -> (f64, f64) {
    let spread = ask - bid;
    let mid = (ask + bid) / 2.0;

    (spread, if mid > 0.0 { spread / mid } else { 0.0 })
}

/// Line showing the bid, ask and their spread as
/// "Bid:  $6.33 × 100  Ask: $7.51 × 200  Spread: $1.18 (17.04%)"
fn offers_line<'a>(
    bid: Option<&Offer>,
    ask: Option<&Offer>,
    currency: &str,
    loaded: bool,
) -> Line<'a> {
    let format_offer = |offer: Option<&Offer>| match offer {
        _ if !loaded => "".to_string(),
        Some(offer) => {
            let price = format_money(currency, &format_price(offer.price), 0);

            match &offer.size {
                Some(size) => format!("{} × {}", price, size),
                None => price,
            }
        }
        None => "-".to_string(),
    };

    let mut spans = vec![
        Span::styled("Bid:  ", label_style()),
        Span::styled(format_offer(bid), style().fg(THEME.profit())),
        Span::styled("  Ask: ", label_style()),
        Span::styled(format_offer(ask), style().fg(THEME.loss())),
    ];

    if let (Some(bid), Some(ask), true) = (bid, ask, loaded) {
        let (spread, pct) = spread(bid.price, ask.price);
        let spread = format!("{:.*}", price_decimals(ask.price), spread);

        spans.push(Span::styled("  Spread: ", label_style()));
        spans.push(Span::styled(
            format!(
                "{} ({:.2}%)",
                format_money(currency, &spread, 0),
                pct * 100.0
            ),
            style().fg(THEME.text_normal()),
        ));
    }

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use unicode_width::UnicodeWidthStr;

    use std::time::Duration;

    use super::{clip_wicks, nasdaq_asset_class, spread, stale_after, truncate_title};
    use crate::api::AssetClass;

    #[test]
    fn truncate_title_keeps_titles_that_fit() {
        assert_eq!(truncate_title("MU - Micron", 11), "MU - Micron");
    }

    #[test]
    fn truncate_title_adds_elipsis() {
        assert_eq!(truncate_title("MU - Micron Technology", 10), "MU - Mi...");
    }

    #[test]
    fn truncate_title_moves_trailing_spaces_after_elipsis() {
        assert_eq!(truncate_title("MU - Micron Technology", 8), "MU -... ");
    }

    #[test]
    fn truncate_title_handles_multibyte_characters() {
        // 'é' is two bytes, a byte based cut at 9 would land inside it
        let truncated = truncate_title("NESN.SW - Nestlé S.A.", 15);
        assert_eq!(truncated, "NESN.SW - Ne...");

        let truncated = truncate_title("NESN.SW - Nestlé S.A.", 17);
        assert_eq!(truncated, "NESN.SW - Nest...");

        let truncated = truncate_title("NESN.SW - Nestlé S.A.", 18);
        assert_eq!(truncated, "NESN.SW - Nestl...");

        let truncated = truncate_title("NESN.SW - Nestlé S.A.", 19);
        assert_eq!(truncated, "NESN.SW - Nestlé...");
    }

    #[test]
    fn truncate_title_handles_wide_characters() {
        let truncated = truncate_title("7203.T - トヨタ自動車", 15);
        assert_eq!(truncated.width(), 15);
        assert_eq!(truncated, "7203.T - ト... ");
    }

    #[test]
    fn truncate_title_handles_tiny_widths() {
        assert_eq!(truncate_title("MU - Micron", 0), "...");
        assert_eq!(truncate_title("MU - Micron", 2), "...");
    }

    #[test]
    fn nasdaq_has_us_stocks_and_etfs() {
        assert_eq!(
            nasdaq_asset_class("MU", Some("EQUITY")),
            Some(AssetClass::Stocks)
        );
        assert_eq!(
            nasdaq_asset_class("BRK-B", Some("EQUITY")),
            Some(AssetClass::Stocks)
        );
        assert_eq!(
            nasdaq_asset_class("QQQ", Some("ETF")),
            Some(AssetClass::Etf)
        );
    }

    #[test]
    fn nasdaq_has_no_other_listings() {
        assert_eq!(nasdaq_asset_class("SIVE.ST", Some("EQUITY")), None);
        assert_eq!(nasdaq_asset_class("^GSPC", Some("INDEX")), None);
        assert_eq!(nasdaq_asset_class("BTC-USD", Some("CRYPTOCURRENCY")), None);
        assert_eq!(nasdaq_asset_class("ES=F", Some("FUTURE")), None);
        assert_eq!(nasdaq_asset_class("MU", None), None);
    }

    #[test]
    fn spread_is_relative_to_the_midpoint() {
        let (spread, pct) = spread(900.0, 924.96);

        assert!((spread - 24.96).abs() < 1e-9);
        assert!((pct - 0.027_355).abs() < 1e-6, "{}", pct);
    }

    #[test]
    fn spread_without_prices() {
        assert_eq!(spread(0.0, 0.0), (0.0, 0.0));
    }

    #[test]
    fn quotes_are_stale_after_20_seconds() {
        assert_eq!(stale_after(1), Duration::from_secs(20));
    }

    #[test]
    fn quotes_are_stale_after_3_slow_updates() {
        assert_eq!(stale_after(10), Duration::from_secs(30));
        assert_eq!(stale_after(u64::MAX), Duration::from_secs(u64::MAX));
    }

    #[test]
    fn long_wicks_are_clipped() {
        // MU's 04:02 spike to 937.53, with bodies from 915 to 931.24
        assert_eq!(clip_wicks((915.0, 931.24), (914.5, 937.53)), (914.5, 935.3));
    }

    #[test]
    fn short_wicks_are_kept() {
        assert_eq!(clip_wicks((100.0, 200.0), (95.0, 210.0)), (95.0, 210.0));
    }

    #[test]
    fn wicks_of_flat_bodies_are_kept() {
        assert_eq!(clip_wicks((100.0, 100.0), (99.0, 102.0)), (99.0, 102.0));
    }
}
