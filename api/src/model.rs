use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use anyhow::Result;
use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone)]
pub struct CrumbData {
    pub cookie: String,
    pub crumb: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Chart {
    pub chart: ChartStatus,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChartStatus {
    pub result: Option<Vec<ChartData>>,
    pub error: Option<Error>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Error {
    pub code: String,
    pub description: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChartData {
    pub meta: ChartMeta,
    pub timestamp: Vec<i64>,
    pub indicators: ChartIndicators,
}
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChartMeta {
    pub instrument_type: Option<String>,
    pub regular_market_price: f64,
    pub regular_market_time: Option<i64>,
    pub chart_previous_close: f64,
    pub current_trading_period: Option<ChartCurrentTradingPeriod>,
    /// Offset of the exchange's timezone from UTC, in seconds
    pub gmtoffset: Option<i64>,
}

impl Hash for ChartMeta {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.instrument_type.hash(state);
        self.regular_market_price.to_bits().hash(state);
        self.regular_market_time.hash(state);
        self.chart_previous_close.to_bits().hash(state);
        self.current_trading_period.hash(state);
        self.gmtoffset.hash(state);
    }
}

#[derive(Debug, Deserialize, Clone, Hash)]
#[serde(rename_all = "camelCase")]
pub struct ChartCurrentTradingPeriod {
    pub regular: ChartTradingPeriod,
    pub pre: ChartTradingPeriod,
    pub post: ChartTradingPeriod,
}

#[derive(Debug, Deserialize, Clone, Hash)]
#[serde(rename_all = "camelCase")]
pub struct ChartTradingPeriod {
    pub start: i64,
    pub end: i64,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChartIndicators {
    pub quote: Vec<ChartQuote>,
    pub adjclose: Option<Vec<ChartAdjClose>>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChartAdjClose {
    #[serde(deserialize_with = "deserialize_vec")]
    pub adjclose: Vec<f64>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChartQuote {
    #[serde(deserialize_with = "deserialize_vec")]
    pub close: Vec<f64>,
    #[serde(deserialize_with = "deserialize_vec")]
    pub volume: Vec<u64>,
    #[serde(deserialize_with = "deserialize_vec")]
    pub high: Vec<f64>,
    #[serde(deserialize_with = "deserialize_vec")]
    pub low: Vec<f64>,
    #[serde(deserialize_with = "deserialize_vec")]
    pub open: Vec<f64>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Company {
    #[serde(rename = "quoteSummary")]
    pub company: CompanyStatus,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CompanyStatus {
    pub result: Option<Vec<CompanyData>>,
    pub error: Option<Error>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CompanyData {
    #[serde(rename = "assetProfile")]
    pub profile: Option<CompanyProfile>,
    pub price: CompanyPrice,
    #[serde(rename = "summaryDetail")]
    pub summary_detail: Option<SummaryDetail>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SummaryDetail {
    pub market_cap: Option<OptionalCompanyMarketPrice>,
    #[serde(rename = "averageVolume")]
    pub average_volume: Option<OptionalCompanyMarketPrice>,
    pub bid: Option<OptionalCompanyMarketPrice>,
    pub bid_size: Option<OptionalCompanyMarketPrice>,
    pub ask: Option<OptionalCompanyMarketPrice>,
    pub ask_size: Option<OptionalCompanyMarketPrice>,
}

#[derive(Debug, Deserialize, Clone, Hash)]
#[serde(rename_all = "camelCase")]
pub struct CompanyProfile {
    pub website: Option<String>,
    pub industry: Option<String>,
    pub sector: Option<String>,
    #[serde(rename = "longBusinessSummary")]
    pub description: Option<String>,
    #[serde(rename = "fullTimeEmployees")]
    pub employees: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CompanyPrice {
    pub symbol: String,
    pub short_name: String,
    pub long_name: Option<String>,
    pub regular_market_price: CompanyMarketPrice,
    pub regular_market_time: Option<i64>,
    pub regular_market_previous_close: CompanyMarketPrice,
    #[serde(default)]
    pub pre_market_price: OptionalCompanyMarketPrice,
    pub pre_market_time: Option<i64>,
    #[serde(default)]
    pub post_market_price: OptionalCompanyMarketPrice,
    pub post_market_time: Option<i64>,
    pub regular_market_volume: OptionalCompanyMarketPrice,
    pub currency: Option<String>,
    /// PREPRE, PRE, REGULAR, POST, POSTPOST or CLOSED
    pub market_state: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CompanyMarketPrice {
    #[serde(rename = "raw")]
    pub price: f64,
    pub fmt: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct OptionalCompanyMarketPrice {
    #[serde(rename = "raw")]
    pub price: Option<f64>,
    pub fmt: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub option_chain: OptionsStatus,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OptionsStatus {
    pub result: Option<Vec<OptionsHeader>>,
    pub error: Option<Error>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OptionsHeader {
    pub quote: OptionsQuote,
    pub expiration_dates: Vec<i64>,
    pub options: Vec<OptionsData>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OptionsQuote {
    pub regular_market_price: f64,
}

impl Hash for OptionsQuote {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.regular_market_price.to_bits().hash(state);
    }
}

#[derive(Debug, Deserialize, Clone, Hash)]
#[serde(rename_all = "camelCase")]
pub struct OptionsData {
    pub expiration_date: i64,
    pub calls: Vec<OptionsContract>,
    pub puts: Vec<OptionsContract>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OptionsContract {
    pub strike: f64,
    pub last_price: f64,
    pub change: f64,
    #[serde(default)]
    pub percent_change: f64,
    pub volume: Option<u64>,
    pub open_interest: Option<u64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub implied_volatility: Option<f64>,
    pub in_the_money: Option<bool>,
    pub currency: Option<String>,
}

impl Hash for OptionsContract {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.strike.to_bits().hash(state);
        self.last_price.to_bits().hash(state);
        self.change.to_bits().hash(state);
        self.percent_change.to_bits().hash(state);
        self.volume.hash(state);
        self.open_interest.hash(state);
        self.bid.map(|f| f.to_bits()).hash(state);
        self.ask.map(|f| f.to_bits()).hash(state);
        self.implied_volatility.map(|f| f.to_bits()).hash(state);
        self.in_the_money.hash(state);
        self.currency.hash(state);
    }
}

fn deserialize_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    struct SeqVisitor<T>(PhantomData<T>);

    impl<'de, T> Visitor<'de> for SeqVisitor<T>
    where
        T: Deserialize<'de> + Default,
    {
        type Value = Vec<T>;

        fn expecting(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
            fmt.write_str("default vec")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut vec = Vec::new();
            // `null` entries (e.g. missing data points) become the default value
            while let Some(elem) = seq.next_element::<Option<T>>()? {
                vec.push(elem.unwrap_or_default());
            }
            Ok(vec)
        }
    }
    deserializer.deserialize_seq(SeqVisitor(PhantomData))
}

/// Response of Nasdaq's quote info endpoint, `data` is null for unknown symbols
#[derive(Debug, Deserialize, Clone)]
pub struct NasdaqQuoteInfo {
    pub data: Option<NasdaqQuoteInfoData>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NasdaqQuoteInfoData {
    /// "Pre-Market", "Market Open", "After-Hours" or "Closed"
    pub market_status: Option<String>,
    /// Data of the current session
    pub primary_data: Option<NasdaqPrimaryData>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NasdaqPrimaryData {
    /// Shares traded in the session, e.g. "346,943.448348"
    pub volume: Option<String>,
}

impl NasdaqQuoteInfo {
    /// Shares traded in pre-market, if Nasdaq is in pre-market
    pub fn pre_market_volume(&self) -> Option<u64> {
        let data = self.data.as_ref()?;

        if data.market_status.as_deref() != Some("Pre-Market") {
            return None;
        }

        let volume = data.primary_data.as_ref()?.volume.as_ref()?;
        let volume: f64 = volume.replace(',', "").parse().ok()?;

        Some(volume.round() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::{ChartQuote, NasdaqQuoteInfo};

    #[test]
    fn test_deserialize_vec_defaults_nulls() {
        let quote: ChartQuote = serde_json::from_str(
            r#"{"close":[1.5,null,2.5],"volume":[10,null,30],"high":[],"low":[null],"open":[1.0]}"#,
        )
        .unwrap();

        assert_eq!(quote.close, vec![1.5, 0.0, 2.5]);
        assert_eq!(quote.volume, vec![10, 0, 30]);
        assert_eq!(quote.low, vec![0.0]);
    }

    #[test]
    fn test_deserialize_vec_reports_invalid_elements() {
        // Previously this stopped at the bad element, giving a confusing error
        let err = serde_json::from_str::<ChartQuote>(
            r#"{"close":[1.5],"volume":[10,"x",30],"high":[],"low":[],"open":[]}"#,
        )
        .unwrap_err();

        assert!(err.to_string().contains("invalid type"), "{}", err);
    }

    fn nasdaq_info(json: &str) -> NasdaqQuoteInfo {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn nasdaq_pre_market_volume() {
        let info = nasdaq_info(
            r#"{"data":{"symbol":"MU","marketStatus":"Pre-Market",
                "primaryData":{"lastSalePrice":"$913.49","volume":"346,943.548348"}}}"#,
        );

        assert_eq!(info.pre_market_volume(), Some(346_944));
    }

    #[test]
    fn nasdaq_no_pre_market_volume_outside_pre_market() {
        let info = nasdaq_info(
            r#"{"data":{"marketStatus":"Market Open","primaryData":{"volume":"12,345"}}}"#,
        );

        assert_eq!(info.pre_market_volume(), None);
    }

    #[test]
    fn nasdaq_unknown_symbol() {
        let info = nasdaq_info(
            r#"{"data":null,"message":null,"status":{"rCode":400,
                "bCodeMessage":[{"code":1001,"errorMessage":"Symbol not exists."}]}}"#,
        );

        assert_eq!(info.pre_market_volume(), None);
    }

    #[test]
    fn nasdaq_empty_volume() {
        let info =
            nasdaq_info(r#"{"data":{"marketStatus":"Pre-Market","primaryData":{"volume":""}}}"#);

        assert_eq!(info.pre_market_volume(), None);
    }
}
