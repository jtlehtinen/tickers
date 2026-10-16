use async_std::sync::Arc;
use futures::future::BoxFuture;

use super::*;
use crate::api::model::NasdaqOffer;
use crate::api::AssetClass;
use crate::common::format_volume;

/// Returns what Nasdaq has and Yahoo lacks or gets wrong for US listings:
/// today's pre-market volume (Yahoo reports the previous session's until the
/// market opens), and the best bid / ask (Yahoo's are stale, often crossed or
/// dollars wide)
pub struct NasdaqQuote {
    symbol: String,
    asset_class: AssetClass,
}

impl NasdaqQuote {
    pub fn new(symbol: String, asset_class: AssetClass) -> NasdaqQuote {
        NasdaqQuote {
            symbol,
            asset_class,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Hash)]
pub struct NasdaqData {
    /// `None` outside of pre-market
    pub pre_market_volume: Option<u64>,
    pub bid: Option<Offer>,
    pub ask: Option<Offer>,
}

impl AsyncTask for NasdaqQuote {
    type Input = (String, AssetClass);
    type Response = NasdaqData;

    /// Nasdaq's bid / ask change every few seconds
    fn update_interval(&self) -> Option<Duration> {
        Some(Duration::from_secs(2))
    }

    fn input(&self) -> Self::Input {
        (self.symbol.clone(), self.asset_class)
    }

    fn task<'a>(input: Arc<Self::Input>) -> BoxFuture<'a, Result<Self::Response>> {
        Box::pin(async move {
            let (symbol, asset_class) = input.as_ref();

            let info = crate::CLIENT.get_nasdaq_quote(symbol, *asset_class).await?;

            Ok(NasdaqData {
                pre_market_volume: info.pre_market_volume(),
                bid: info.bid().map(offer),
                ask: info.ask().map(offer),
            })
        })
    }
}

fn offer(offer: NasdaqOffer) -> Offer {
    Offer {
        price: offer.price,
        size: offer.size.map(format_volume),
    }
}
