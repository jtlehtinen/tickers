use async_std::sync::Arc;
use futures::future::BoxFuture;

use super::*;

/// Returns the current price, only if it has changed
pub struct CurrentPrice {
    symbol: String,
}

impl CurrentPrice {
    pub fn new(symbol: String) -> CurrentPrice {
        CurrentPrice { symbol }
    }
}

impl AsyncTask for CurrentPrice {
    type Input = String;
    type Response = (f64, Option<f64>, String);

    fn update_interval(&self) -> Option<Duration> {
        Some(Duration::from_secs(1))
    }

    fn input(&self) -> Self::Input {
        self.symbol.clone()
    }

    fn task<'a>(input: Arc<Self::Input>) -> BoxFuture<'a, Result<Self::Response>> {
        Box::pin(async move {
            let symbol = input.as_ref();

            let crumb = crumb().await?;

            let response = check_crumb(crate::CLIENT.get_company_data(symbol, crumb).await).await?;

            let regular_price = response.price.regular_market_price.price;

            let post_price = response.price.post_market_price.price;

            let volume = response.price.regular_market_volume.fmt.unwrap_or_default();

            Ok((regular_price, post_price, volume))
        })
    }
}
