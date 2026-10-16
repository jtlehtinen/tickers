use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::time::Duration;

use chrono::{DateTime, Datelike, Weekday};
use itertools::izip;
use serde::Deserialize;
use tickrs_api::Interval;

use crate::api::model::{ChartData, ChartMeta};
use crate::api::Range;

pub const DAY: i64 = 60 * 60 * 24;

#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash, Deserialize)]
pub enum ChartType {
    #[serde(rename = "line")]
    Line,
    #[serde(rename = "candle")]
    Candlestick,
    #[serde(rename = "kagi")]
    Kagi,
}

impl ChartType {
    pub fn toggle(self) -> Self {
        match self {
            ChartType::Line => ChartType::Candlestick,
            ChartType::Candlestick => ChartType::Kagi,
            ChartType::Kagi => ChartType::Line,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ChartType::Line => "Line",
            ChartType::Candlestick => "Candle",
            ChartType::Kagi => "Kagi",
        }
    }
}

/// Orders the tabs can be sorted in
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum TabSort {
    ChangeDesc,
    ChangeAsc,
    Symbol,
}

impl TabSort {
    pub fn next(self) -> Self {
        match self {
            TabSort::ChangeDesc => TabSort::ChangeAsc,
            TabSort::ChangeAsc => TabSort::Symbol,
            TabSort::Symbol => TabSort::ChangeDesc,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TabSort::ChangeDesc => "↓%",
            TabSort::ChangeAsc => "↑%",
            TabSort::Symbol => "A-Z",
        }
    }

    /// Compares tabs by their symbol and today's change. Tabs without a change
    /// go last either way, and changes equal at the shown precision are equal,
    /// so a stable sort keeps their order
    pub fn cmp(
        self,
        (a_symbol, a_pct): (&str, Option<f64>),
        (b_symbol, b_pct): (&str, Option<f64>),
    ) -> Ordering {
        // Hundredths of a percent, as the tabs show
        let shown = |pct: f64| (pct * 10_000.0).round() as i64;

        match (self, a_pct.map(shown), b_pct.map(shown)) {
            (TabSort::Symbol, _, _) => a_symbol.cmp(b_symbol),
            (_, None, None) => Ordering::Equal,
            (_, None, Some(_)) => Ordering::Greater,
            (_, Some(_), None) => Ordering::Less,
            (TabSort::ChangeDesc, Some(a), Some(b)) => b.cmp(&a),
            (TabSort::ChangeAsc, Some(a), Some(b)) => a.cmp(&b),
        }
    }
}

impl FromStr for ChartType {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use ChartType::*;

        match s {
            "line" => Ok(Line),
            "candle" => Ok(Candlestick),
            "kagi" => Ok(Kagi),
            _ => Err("Valid chart types are: 'line', 'candle', 'kagi'"),
        }
    }
}
#[derive(Clone, Copy, PartialOrd, Debug, Hash, PartialEq, Eq, Deserialize, Ord)]
pub enum TimeFrame {
    #[serde(alias = "1D")]
    Day1,
    #[serde(alias = "1W")]
    Week1,
    #[serde(alias = "1M")]
    Month1,
    #[serde(alias = "3M")]
    Month3,
    #[serde(alias = "6M")]
    Month6,
    #[serde(alias = "1Y")]
    Year1,
    #[serde(alias = "5Y")]
    Year5,
}

impl FromStr for TimeFrame {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use TimeFrame::*;

        match s {
            "1D" => Ok(Day1),
            "1W" => Ok(Week1),
            "1M" => Ok(Month1),
            "3M" => Ok(Month3),
            "6M" => Ok(Month6),
            "1Y" => Ok(Year1),
            "5Y" => Ok(Year5),
            _ => Err("Valid time frames are: '1D', '1W', '1M', '3M', '6M', '1Y', '5Y'"),
        }
    }
}

impl TimeFrame {
    pub fn idx(self) -> usize {
        match self {
            TimeFrame::Day1 => 0,
            TimeFrame::Week1 => 1,
            TimeFrame::Month1 => 2,
            TimeFrame::Month3 => 3,
            TimeFrame::Month6 => 4,
            TimeFrame::Year1 => 5,
            TimeFrame::Year5 => 6,
        }
    }

    pub const fn tab_names() -> [&'static str; 7] {
        ["1D", "1W", "1M", "3M", "6M", "1Y", "5Y"]
    }

    pub const ALL: [TimeFrame; 7] = [
        TimeFrame::Day1,
        TimeFrame::Week1,
        TimeFrame::Month1,
        TimeFrame::Month3,
        TimeFrame::Month6,
        TimeFrame::Year1,
        TimeFrame::Year5,
    ];

    pub fn update_interval(self) -> Duration {
        match self {
            TimeFrame::Day1 => Duration::from_secs(60),
            TimeFrame::Week1 => Duration::from_secs(60 * 5),
            TimeFrame::Month1 => Duration::from_secs(60 * 30),
            TimeFrame::Month3 => Duration::from_secs(60 * 60),
            TimeFrame::Month6 => Duration::from_secs(60 * 60),
            TimeFrame::Year1 => Duration::from_secs(60 * 60 * 24),
            TimeFrame::Year5 => Duration::from_secs(60 * 60 * 24),
        }
    }

    pub fn up(self) -> TimeFrame {
        match self {
            TimeFrame::Day1 => TimeFrame::Week1,
            TimeFrame::Week1 => TimeFrame::Month1,
            TimeFrame::Month1 => TimeFrame::Month3,
            TimeFrame::Month3 => TimeFrame::Month6,
            TimeFrame::Month6 => TimeFrame::Year1,
            TimeFrame::Year1 => TimeFrame::Year5,
            TimeFrame::Year5 => TimeFrame::Day1,
        }
    }

    pub fn down(self) -> TimeFrame {
        match self {
            TimeFrame::Day1 => TimeFrame::Year5,
            TimeFrame::Week1 => TimeFrame::Day1,
            TimeFrame::Month1 => TimeFrame::Week1,
            TimeFrame::Month3 => TimeFrame::Month1,
            TimeFrame::Month6 => TimeFrame::Month3,
            TimeFrame::Year1 => TimeFrame::Month6,
            TimeFrame::Year5 => TimeFrame::Year1,
        }
    }

    pub fn as_range(self) -> Range {
        match self {
            TimeFrame::Day1 => Range::Day1,
            TimeFrame::Week1 => Range::Day5,
            TimeFrame::Month1 => Range::Month1,
            TimeFrame::Month3 => Range::Month3,
            TimeFrame::Month6 => Range::Month6,
            TimeFrame::Year1 => Range::Year1,
            TimeFrame::Year5 => Range::Year5,
        }
    }

    pub fn api_interval(self) -> Interval {
        match self {
            TimeFrame::Day1 => Interval::Minute1,
            TimeFrame::Week1 => Interval::Minute5,
            TimeFrame::Month1 => Interval::Minute30,
            TimeFrame::Month3 => Interval::Minute60,
            TimeFrame::Month6 => Interval::Minute60,
            _ => Interval::Day1,
        }
    }

    pub fn round_by(self) -> i64 {
        match self {
            TimeFrame::Day1 => 60,
            TimeFrame::Week1 => 60 * 5,
            TimeFrame::Month1 => 60 * 30,
            TimeFrame::Month3 => 60 * 60,
            TimeFrame::Month6 => 60 * 60,
            _ => 60 * 60 * 24,
        }
    }

    /// Format of a point in time on this time frame
    pub fn time_format(self) -> &'static str {
        match self {
            TimeFrame::Day1 => "%H:%M",
            TimeFrame::Week1 => "%m-%d %H:%M",
            _ => "%F",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradingPeriod {
    Pre,
    Regular,
    Post,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Price {
    pub close: f64,
    pub volume: u64,
    pub high: f64,
    pub low: f64,
    pub open: f64,
    pub date: i64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MarketTimes {
    pub start: i64,
    pub end: i64,
    pub delta: i64,
    pub current: i64,
    /// Exchange offset from UTC in seconds. Iteration happens in exchange
    /// local time so sessions don't wrap around midnight and weekends fall
    /// on the exchange's weekend.
    pub utc_offset: i64,
}

impl Price {
    pub fn new(date: i64) -> Self {
        Price {
            date,
            ..Default::default()
        }
    }
}

impl Hash for Price {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.close.to_bits().hash(state);
        self.volume.hash(state);
        self.high.to_bits().hash(state);
        self.low.to_bits().hash(state);
        self.open.to_bits().hash(state);
        self.date.hash(state);
    }
}

impl MarketTimes {
    /// Times are UTC epochs, `utc_offset` is the exchange's offset from UTC
    pub fn new(start: i64, end: i64, delta: i64, utc_offset: i64) -> Self {
        MarketTimes {
            start: start + utc_offset,
            end: end + utc_offset,
            delta,
            current: start + utc_offset,
            utc_offset,
        }
    }
}

impl Iterator for MarketTimes {
    type Item = i64;

    fn next(&mut self) -> Option<Self::Item> {
        // Round to the last value divisible by delta
        let start = self.start - self.start % self.delta;
        let end = self.end - self.end % self.delta;
        let current = self.current - self.current % self.delta;

        // We exceeded the end time, stop the iteration
        if current >= end {
            return None;
        }

        // Elapsed time since 00:00
        let start_time = start % DAY;
        let end_time = end % DAY;
        let current_time = current % DAY;

        if (start_time < end_time && (start_time <= current_time && current_time < end_time))
            || (start_time > end_time && (current_time < end_time || start_time <= current_time))
        {
            // Market hasn't closed yet, advance by delta
            self.current = current + self.delta;
        } else {
            // We're post the market closing time

            // Today's market opening time, which is still ahead of us if the
            // session wraps around midnight
            let mut next_open = current - current_time + start_time;

            if next_open <= current {
                next_open += DAY;
            }

            // Skip the weekend
            while matches!(get_weekday(next_open), Weekday::Sat | Weekday::Sun) {
                next_open += DAY;
            }

            self.current = next_open;
        }

        Some(current - self.utc_offset)
    }
}

pub fn chart_data_to_prices(mut chart_data: ChartData) -> Vec<Price> {
    if chart_data.indicators.quote.len() != 1 {
        return vec![];
    }

    let quote = chart_data.indicators.quote.remove(0);
    let timestamps = chart_data.timestamp;

    izip!(
        &quote.close,
        &quote.volume,
        &quote.high,
        &quote.low,
        &quote.open,
        &timestamps,
    )
    .map(|(c, v, h, l, o, t)| Price {
        close: *c,
        volume: *v,
        high: *h,
        low: *l,
        open: *o,
        date: *t,
    })
    .collect()
}

/// Close of the session before the current one, which 1D changes are
/// measured against.
///
/// Until the current session's regular market opens, Yahoo's chart previous
/// close is the close before the *last* session, while the last session's close
/// is still the regular market price.
pub fn session_previous_close(meta: &ChartMeta) -> f64 {
    let session_start = meta
        .current_trading_period
        .as_ref()
        .map(|period| period.regular.start);

    match (meta.regular_market_time, session_start) {
        (Some(time), Some(start)) if time < start => meta.regular_market_price,
        _ => meta.chart_previous_close,
    }
}

pub fn cast_historical_as_price(input: &Price) -> f64 {
    input.close
}

pub fn zeros_as_pre(prices: &mut [f64]) {
    if prices.len() <= 1 {
        return;
    }

    let zero_indexes = prices
        .iter()
        .enumerate()
        .filter_map(|(idx, price)| if *price == 0.0 { Some(idx) } else { None })
        .collect::<Vec<usize>>();

    for idx in zero_indexes {
        if idx == 0 {
            prices[0] = prices[1];
        } else {
            prices[idx] = prices[idx - 1];
        }
    }
}

pub fn format_decimals(value: f64) -> String {
    let abs = value.abs();

    if abs == 0.0 {
        "0".into()
    } else {
        let max_chars: usize = 8;

        // Max chars minus (chars to left of decial + decimal place)
        let n = max_chars.saturating_sub(abs.log10() as usize + 2);

        format!("{:.*}", n, value)
    }
}

/// Decimals to show a price with: cents for prices of at least 1, and around
/// four significant digits for smaller prices. Large prices drop decimals to
/// fit in 8 chars
pub fn price_decimals(price: f64) -> usize {
    let abs = price.abs();

    if abs == 0.0 || !abs.is_finite() {
        2
    } else if abs < 1.0 {
        ((3.0 - abs.log10().floor()) as usize).clamp(4, 8)
    } else {
        let int_digits = abs.log10().floor() as usize + 1;

        // Max chars minus (int digits + decimal place)
        2.min(7usize.saturating_sub(int_digits))
    }
}

pub fn format_price(price: f64) -> String {
    format!("{:.*}", price_decimals(price), price)
}

/// Where a currency's symbol goes, relative to the amount
enum CurrencySymbol {
    Prefix(&'static str),
    Suffix(&'static str),
}

/// Symbol of a currency (an ISO 4217 code, as reported by Yahoo), if it's
/// unambiguous, e.g. $ is only used for USD
fn currency_symbol(currency: &str) -> Option<CurrencySymbol> {
    use CurrencySymbol::*;

    let symbol = match currency {
        "USD" => Prefix("$"),
        "EUR" => Prefix("€"),
        "GBP" => Prefix("£"),
        // London listings are quoted in pence
        "GBp" | "GBX" => Suffix("p"),
        "JPY" => Prefix("¥"),
        "CNY" => Prefix("CN¥"),
        "HKD" => Prefix("HK$"),
        "CAD" => Prefix("CA$"),
        "AUD" => Prefix("A$"),
        "NZD" => Prefix("NZ$"),
        "SGD" => Prefix("S$"),
        "TWD" => Prefix("NT$"),
        "MXN" => Prefix("MX$"),
        "BRL" => Prefix("R$"),
        "INR" => Prefix("₹"),
        "KRW" => Prefix("₩"),
        "ILS" => Prefix("₪"),
        _ => return None,
    };

    Some(symbol)
}

/// Formats an `amount` in `currency` with its symbol, e.g. $9.97 or 123.40p,
/// or with its code when there's no unambiguous symbol, e.g. SEK 9.97. The
/// amount is right aligned to `width`, so amounts can line up in a column.
pub fn format_money(currency: &str, amount: &str, width: usize) -> String {
    match currency_symbol(currency) {
        Some(CurrencySymbol::Prefix(symbol)) => {
            let money = format!("{}{}", symbol, amount);
            let width = width + symbol.chars().count();

            format!("{:>width$}", money)
        }
        Some(CurrencySymbol::Suffix(symbol)) => format!("{:>width$}{}", amount, symbol),
        None => format!("{} {:>width$}", currency, amount),
    }
}

/// Formats a volume like Yahoo does, e.g. 389.23k or 38.35M
pub fn format_volume(volume: u64) -> String {
    let volume = volume as f64;

    match [(1e12, "T"), (1e9, "B"), (1e6, "M"), (1e3, "k")]
        .into_iter()
        .find(|(unit, _)| volume >= *unit)
    {
        Some((unit, suffix)) => format!("{:.2}{}", volume / unit, suffix),
        None => format!("{}", volume),
    }
}

fn get_weekday(epoch: i64) -> Weekday {
    DateTime::from_timestamp(epoch, 0)
        .unwrap_or_default()
        .weekday()
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Datelike, Weekday};

    use super::{
        format_money, format_price, format_volume, session_previous_close, MarketTimes, TabSort,
        DAY,
    };
    use crate::api::model::ChartMeta;

    const HOUR: i64 = 60 * 60;

    /// Monday 2026-01-05 00:00 UTC
    const MONDAY: i64 = 1_767_571_200;

    fn weekday(epoch: i64) -> Weekday {
        DateTime::from_timestamp(epoch, 0).unwrap().weekday()
    }

    /// The implementation prior to iterating in exchange local time
    fn legacy_market_times(start: i64, end: i64, delta: i64) -> Vec<i64> {
        let mut times = vec![];
        let mut current = start;

        loop {
            let start = start - start % delta;
            let end = end - end % delta;
            let now = current - current % delta;

            if now >= end {
                return times;
            }

            let (start_time, end_time, now_time) = (start % DAY, end % DAY, now % DAY);

            if (start_time < end_time && (start_time <= now_time && now_time < end_time))
                || (start_time > end_time && (now_time < end_time || start_time <= now_time))
            {
                current = now + delta;
            } else {
                current = now - now_time + start_time;
                current += DAY
                    * match weekday(current) {
                        Weekday::Fri => 3,
                        Weekday::Sat => 2,
                        _ => 1,
                    };
            }

            times.push(now);
        }
    }

    #[test]
    fn market_times_matches_legacy_for_us_hours() {
        // NYSE 09:30 - 16:00, in both EDT and EST
        for utc_offset in [-4 * HOUR, -5 * HOUR] {
            let start = MONDAY + 9 * HOUR + 30 * 60 - utc_offset;
            let end = start + 14 * DAY + 6 * HOUR + 30 * 60;

            for delta in [5 * 60, 30 * 60, HOUR] {
                let times = MarketTimes::new(start, end, delta, utc_offset).collect::<Vec<_>>();

                assert!(!times.is_empty());
                assert_eq!(times, legacy_market_times(start, end, delta));
            }
        }
    }

    #[test]
    fn market_times_handles_sessions_wrapping_utc_midnight() {
        // ASX during AEDT (UTC+11): 10:00 - 16:00 local, 23:00 - 05:00 UTC
        let utc_offset = 11 * HOUR;
        let start = MONDAY - HOUR; // Sunday 23:00 UTC, Monday 10:00 local
        let end = start + 7 * DAY + 6 * HOUR;

        let times = MarketTimes::new(start, end, HOUR, utc_offset).collect::<Vec<_>>();

        let mut session_days = times
            .iter()
            .map(|t| (t - start).div_euclid(DAY))
            .collect::<Vec<_>>();
        session_days.dedup();

        // Every weekday session, previously every other session was skipped
        assert_eq!(session_days, vec![0, 1, 2, 3, 4, 7]);

        // Hourly steps from the 10:00 open through the 16:00 close (local), the
        // last session is cut off by `end`
        assert_eq!(times.len(), 5 * 7 + 6);
        for time in times {
            let local_hour = (time + utc_offset).rem_euclid(DAY) / HOUR;
            assert!((10..=16).contains(&local_hour), "{}", local_hour);
            assert!(!matches!(
                weekday(time + utc_offset),
                Weekday::Sat | Weekday::Sun
            ));
        }
    }

    #[test]
    fn market_times_steps_daily_over_business_days() {
        let utc_offset = -5 * HOUR;
        let start = MONDAY + 14 * HOUR;
        let end = start + 14 * DAY;

        let times = MarketTimes::new(start, end, DAY, utc_offset).collect::<Vec<_>>();

        assert_eq!(times.len(), 10);
        for time in times {
            assert!(!matches!(
                weekday(time + utc_offset),
                Weekday::Sat | Weekday::Sun
            ));
        }
    }

    /// ONDS chart meta on 2026-10-16 before the market opened, after closing at
    /// 6.35 on 10-15 and 6.14 on 10-14
    fn onds_meta(regular_market_time: i64) -> ChartMeta {
        serde_json::from_value(serde_json::json!({
            "regularMarketPrice": 6.35,
            "regularMarketTime": regular_market_time,
            "chartPreviousClose": 6.14,
            "currentTradingPeriod": {
                "pre": {"start": 1_792_137_600, "end": 1_792_157_400},
                "regular": {"start": 1_792_157_400, "end": 1_792_180_800},
                "post": {"start": 1_792_180_800, "end": 1_792_195_200}
            }
        }))
        .unwrap()
    }

    #[test]
    fn previous_close_before_open_is_last_sessions_close() {
        // 10-15 20:00 UTC, the last session's close
        assert_eq!(session_previous_close(&onds_meta(1_792_094_400)), 6.35);
    }

    #[test]
    fn previous_close_after_open_is_chart_previous_close() {
        // 10-16 14:00 UTC, the session is open
        assert_eq!(session_previous_close(&onds_meta(1_792_159_200)), 6.14);
    }

    #[test]
    fn prices_of_at_least_one_have_cents() {
        assert_eq!(format_price(915.65), "915.65");
        assert_eq!(format_price(-15.59), "-15.59");
        assert_eq!(format_price(6.3), "6.30");
        assert_eq!(format_price(1.0), "1.00");
    }

    #[test]
    fn prices_below_one_have_four_significant_digits() {
        assert_eq!(format_price(0.5), "0.5000");
        assert_eq!(format_price(0.0123), "0.01230");
        assert_eq!(format_price(0.00001234), "0.00001234");
        assert_eq!(format_price(0.000000012), "0.00000001");
    }

    #[test]
    fn large_prices_fit_in_eight_chars() {
        assert_eq!(format_price(99_999.99), "99999.99");
        assert_eq!(format_price(105_000.5), "105000.5");
        assert_eq!(format_price(1_234_567.0), "1234567");
    }

    #[test]
    fn money_has_currency_symbols() {
        assert_eq!(format_money("USD", "9.97", 0), "$9.97");
        assert_eq!(format_money("EUR", "9.97", 0), "€9.97");
        assert_eq!(format_money("CAD", "9.97", 0), "CA$9.97");
        assert_eq!(format_money("GBp", "123.40", 0), "123.40p");
    }

    #[test]
    fn money_without_symbol_has_currency_code() {
        assert_eq!(format_money("SEK", "9.97", 0), "SEK 9.97");
        assert_eq!(format_money("CHF", "9.97", 0), "CHF 9.97");
    }

    #[test]
    fn money_lines_up_in_a_column() {
        assert_eq!(format_money("USD", "9.97", 5), " $9.97");
        assert_eq!(format_money("USD", "10.28", 5), "$10.28");
        assert_eq!(format_money("EUR", "9.97", 5), " €9.97");
        assert_eq!(format_money("GBp", "9.97", 5), " 9.97p");
        assert_eq!(format_money("SEK", "9.97", 5), "SEK  9.97");
    }

    #[test]
    fn volumes_are_shortened() {
        assert_eq!(format_volume(0), "0");
        assert_eq!(format_volume(999), "999");
        assert_eq!(format_volume(389_229), "389.23k");
        assert_eq!(format_volume(38_345_136), "38.35M");
        assert_eq!(format_volume(1_050_000_000), "1.05B");
        assert_eq!(format_volume(2_000_000_000_000), "2.00T");
    }

    #[test]
    fn zero_price_has_cents() {
        assert_eq!(format_price(0.0), "0.00");
    }

    #[test]
    fn tab_sort_orders_by_change_with_missing_changes_last() {
        let tabs = [
            ("A", Some(0.01)),
            ("B", None),
            ("C", Some(-0.02)),
            ("D", Some(0.03)),
        ];
        let sorted = |sort: TabSort| {
            let mut tabs = tabs.to_vec();
            tabs.sort_by(|a, b| sort.cmp(*a, *b));
            tabs.iter().map(|(symbol, _)| *symbol).collect::<String>()
        };

        assert_eq!(sorted(TabSort::ChangeDesc), "DACB");
        assert_eq!(sorted(TabSort::ChangeAsc), "CADB");
        assert_eq!(sorted(TabSort::Symbol), "ABCD");
    }

    #[test]
    fn tab_sort_treats_changes_shown_alike_as_equal() {
        // Both show as 1.23%
        let (a, b) = (("A", Some(0.01231)), ("B", Some(0.01234)));

        assert_eq!(TabSort::ChangeDesc.cmp(a, b), std::cmp::Ordering::Equal);
        assert_eq!(TabSort::ChangeAsc.cmp(a, b), std::cmp::Ordering::Equal);
    }
}
