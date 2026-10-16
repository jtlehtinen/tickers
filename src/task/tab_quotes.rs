use std::collections::HashMap;

use async_std::sync::Arc;
use futures::future::BoxFuture;

use super::*;

/// Returns the quotes of every tab's stock with a single request, by symbol,
/// for the change the tabs show of stocks that aren't shown
pub struct TabQuotes {
    symbols: Vec<String>,
}

impl TabQuotes {
    pub fn new(symbols: Vec<String>) -> TabQuotes {
        TabQuotes { symbols }
    }
}

impl AsyncTask for TabQuotes {
    type Input = Vec<String>;
    type Response = HashMap<String, DayQuote>;

    fn update_interval(&self) -> Option<Duration> {
        Some(TAB_QUOTES_INTERVAL)
    }

    fn input(&self) -> Self::Input {
        self.symbols.clone()
    }

    fn task<'a>(input: Arc<Self::Input>) -> BoxFuture<'a, Result<Self::Response>> {
        Box::pin(async move {
            let symbols = input.as_ref();

            if symbols.is_empty() {
                return Ok(HashMap::new());
            }

            let crumb = crumb().await?;

            let quotes = check_crumb(crate::CLIENT.get_quotes(symbols, crumb).await).await?;

            Ok(quotes
                .iter()
                .filter_map(|quote| Some((quote.symbol.clone(), DayQuote::from_quote_data(quote)?)))
                .collect())
        })
    }
}
