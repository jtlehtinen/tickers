use async_std::sync::Arc;
use futures::future::BoxFuture;

use super::*;
use crate::api::model::CompanyPrice;

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

            let extended_price = extended_hours_price(&response.price);

            let volume = response.price.regular_market_volume.fmt.unwrap_or_default();

            Ok((regular_price, extended_price, volume))
        })
    }
}

/// The latest pre or post market price, if it's more recent than the regular
/// market price. Yahoo keeps reporting the previous session's post market price
/// during the next pre market (and both during regular hours), so their times
/// decide which one is current.
fn extended_hours_price(price: &CompanyPrice) -> Option<f64> {
    let regular_time = price.regular_market_time.unwrap_or(i64::MIN);

    [
        (price.pre_market_time, price.pre_market_price.price),
        (price.post_market_time, price.post_market_price.price),
    ]
    .into_iter()
    .filter_map(|(time, price)| Some((time?, price?)))
    .filter(|(time, _)| *time > regular_time)
    .max_by_key(|(time, _)| *time)
    .map(|(_, price)| price)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MU on 2026-10-16 before the market opened
    const PRE_MARKET: &str = r#"{
        "symbol": "MU",
        "shortName": "Micron Technology, Inc.",
        "marketState": "PRE",
        "preMarketTime": 1792144326,
        "preMarketPrice": {"raw": 920.0, "fmt": "920.00"},
        "postMarketTime": 1792108794,
        "postMarketPrice": {"raw": 923.018, "fmt": "923.02"},
        "regularMarketTime": 1792094401,
        "regularMarketPrice": {"raw": 931.24, "fmt": "931.24"},
        "regularMarketPreviousClose": {"raw": 925.0, "fmt": "925.00"},
        "regularMarketVolume": {"raw": 1000, "fmt": "1k"}
    }"#;

    fn price(json: &str) -> CompanyPrice {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn pre_market_uses_todays_pre_market_price() {
        assert_eq!(extended_hours_price(&price(PRE_MARKET)), Some(920.0));
    }

    #[test]
    fn post_market_uses_post_market_price() {
        let json = PRE_MARKET.replace("1792144326", "1792000000");

        assert_eq!(extended_hours_price(&price(&json)), Some(923.018));
    }

    #[test]
    fn regular_market_ignores_older_extended_prices() {
        let json = PRE_MARKET.replace("1792094401", "1792160000");

        assert_eq!(extended_hours_price(&price(&json)), None);
    }

    #[test]
    fn missing_extended_prices() {
        let json = r#"{
            "symbol": "^GSPC",
            "shortName": "S&P 500",
            "preMarketPrice": {},
            "regularMarketTime": 1792096433,
            "regularMarketPrice": {"raw": 7744.06, "fmt": "7,744.06"},
            "regularMarketPreviousClose": {"raw": 7700.0, "fmt": "7,700.00"},
            "regularMarketVolume": {}
        }"#;

        assert_eq!(extended_hours_price(&price(json)), None);
    }
}
