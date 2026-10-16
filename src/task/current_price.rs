use async_std::sync::Arc;
use futures::future::BoxFuture;

use super::*;
use crate::api::model::{CompanyPrice, OptionalCompanyMarketPrice};

/// Returns the current quote, only if it has changed
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
    type Response = Quote;

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

            let detail = response.summary_detail.as_ref();

            Ok(Quote {
                regular_price: response.price.regular_market_price.price,
                extended_price: extended_hours_price(&response.price),
                volume: response.price.regular_market_volume.fmt.unwrap_or_default(),
                bid: detail.and_then(|d| Offer::new(d.bid.as_ref(), d.bid_size.as_ref())),
                ask: detail.and_then(|d| Offer::new(d.ask.as_ref(), d.ask_size.as_ref())),
            })
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Quote {
    pub regular_price: f64,
    /// Pre or post market price, if it's more recent than the regular price
    pub extended_price: Option<f64>,
    pub volume: String,
    pub bid: Option<Offer>,
    pub ask: Option<Offer>,
}

/// Best bid or ask
#[derive(Debug, Clone, PartialEq)]
pub struct Offer {
    pub price: f64,
    /// Formatted number of shares, if known
    pub size: Option<String>,
}

impl Offer {
    /// Yahoo reports a price of 0 when there's no bid / ask, and a size of 0
    /// when it's unknown (e.g. indices, non US exchanges)
    fn new(
        price: Option<&OptionalCompanyMarketPrice>,
        size: Option<&OptionalCompanyMarketPrice>,
    ) -> Option<Offer> {
        let price = price?.price.filter(|price| *price > 0.0)?;

        let size = size
            .filter(|size| size.price.is_some_and(|size| size > 0.0))
            .and_then(|size| size.fmt.clone());

        Some(Offer { price, size })
    }
}

impl std::hash::Hash for Offer {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.price.to_bits().hash(state);
        self.size.hash(state);
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

    fn offer(price: serde_json::Value, size: serde_json::Value) -> Option<Offer> {
        let price = serde_json::from_value(price).unwrap();
        let size = serde_json::from_value(size).unwrap();

        Offer::new(Some(&price), Some(&size))
    }

    #[test]
    fn offer_with_size() {
        assert_eq!(
            offer(
                serde_json::json!({"raw": 4204.7, "fmt": "4,204.70"}),
                serde_json::json!({"raw": 1400, "fmt": "1.4k", "longFmt": "1,400"}),
            ),
            Some(Offer {
                price: 4204.7,
                size: Some("1.4k".to_string()),
            })
        );
    }

    #[test]
    fn offer_without_size() {
        // ^GSPC, NOKIA.HE, ...
        assert_eq!(
            offer(
                serde_json::json!({"raw": 8.716, "fmt": "8.72"}),
                serde_json::json!({"raw": 0, "fmt": null, "longFmt": "0"}),
            ),
            Some(Offer {
                price: 8.716,
                size: None,
            })
        );
    }

    #[test]
    fn no_offer() {
        // SPY in pre market
        assert_eq!(
            offer(
                serde_json::json!({"raw": 0.0, "fmt": "0.00"}),
                serde_json::json!({"raw": 8000, "fmt": "8k", "longFmt": "8,000"}),
            ),
            None
        );

        // BTC-USD
        assert_eq!(offer(serde_json::json!({}), serde_json::json!({})), None);
        assert_eq!(Offer::new(None, None), None);
    }
}
