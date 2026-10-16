use crossterm::event::Event;

use crate::common::{ChartType, TimeFrame};
use crate::service::default_timestamps::DefaultTimestampService;
use crate::service::tab_quotes::TabQuotesService;
use crate::service::Service;
use crate::{widget, DEFAULT_TIMESTAMPS};

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Mode {
    AddStock,
    ConfigureChart,
    DisplayStock,
    DisplayOptions,
    DisplaySummary,
    Help,
}

pub struct App {
    pub mode: Mode,
    pub stocks: Vec<widget::StockState>,
    pub add_stock: widget::AddStockState,
    pub help: widget::HelpWidget,
    pub current_tab: usize,
    pub hide_help: bool,
    pub debug: DebugInfo,
    pub previous_mode: Mode,
    pub time_frame: TimeFrame,
    pub default_timestamp_service: DefaultTimestampService,
    pub tab_quotes_service: TabQuotesService,
    pub summary_scroll_state: SummaryScrollState,
    pub chart_type: ChartType,
}

impl App {
    pub fn update(&mut self) {
        let mut timestamp_updates = self.default_timestamp_service.updates();

        if let Some(new_defaults) = timestamp_updates.pop() {
            *DEFAULT_TIMESTAMPS.write() = new_defaults;
        }

        // Follow stocks being added and removed
        let symbols = self.stocks.iter().map(|s| s.symbol().to_string()).collect();
        self.tab_quotes_service.set_symbols(symbols);

        for quotes in self.tab_quotes_service.updates() {
            for stock in self.stocks.iter_mut() {
                if let Some(quote) = quotes.get(stock.symbol()) {
                    stock.update_tab_quote(quote.clone());
                }
            }
        }
    }

    pub fn time_frame_up(&mut self) {
        self.set_time_frame(self.time_frame.up());
    }

    pub fn time_frame_down(&mut self) {
        self.set_time_frame(self.time_frame.down());
    }

    pub fn set_time_frame(&mut self, time_frame: TimeFrame) {
        self.time_frame = time_frame;

        for stock in self.stocks.iter_mut() {
            stock.set_time_frame(time_frame);
        }
    }
}

pub struct EnvConfig {
    pub show_debug: bool,
    pub debug_mouse: bool,
}

impl EnvConfig {
    #[inline]
    fn env_match(key: &str, default: &str, expected: &str) -> bool {
        std::env::var(key).ok().unwrap_or_else(|| default.into()) == expected
    }

    pub fn load() -> Self {
        Self {
            show_debug: Self::env_match("SHOW_DEBUG", "0", "1"),
            debug_mouse: Self::env_match("DEBUG_MOUSE", "0", "1"),
        }
    }
}

#[derive(Debug)]
pub struct DebugInfo {
    pub enabled: bool,
    pub dimensions: (u16, u16),
    pub cursor_location: Option<(u16, u16)>,
    pub last_event: Option<Event>,
    pub mode: Mode,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SummaryScrollState {
    pub offset: usize,
    pub queued_scroll: Option<ScrollDirection>,
}

#[derive(Debug, Clone, Copy)]
pub enum ScrollDirection {
    Up,
    Down,
}
