use std::collections::HashMap;

use super::*;
use crate::task::*;

/// Quotes of every tab's stock, fetched together
pub struct TabQuotesService {
    symbols: Vec<String>,
    handle: AsyncTaskHandle<HashMap<String, DayQuote>>,
}

impl TabQuotesService {
    pub fn new(symbols: Vec<String>) -> TabQuotesService {
        let handle = TabQuotes::new(symbols.clone()).connect();

        TabQuotesService { symbols, handle }
    }

    /// Fetches the quotes of `symbols` from now on, if they've changed
    pub fn set_symbols(&mut self, symbols: Vec<String>) {
        if symbols != self.symbols {
            *self = TabQuotesService::new(symbols);
        }
    }
}

impl Service for TabQuotesService {
    type Update = HashMap<String, DayQuote>;

    /// Failures keep the last quotes, they're refetched soon
    fn updates(&self) -> Vec<Self::Update> {
        self.handle
            .response()
            .try_iter()
            .filter_map(Result::ok)
            .collect()
    }

    fn pause(&self) {
        self.handle.pause();
    }

    fn resume(&self) {
        self.handle.resume();
    }
}
