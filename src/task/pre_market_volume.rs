use async_std::sync::Arc;
use futures::future::BoxFuture;

use super::*;
use crate::api::AssetClass;

/// Returns today's pre-market volume from Nasdaq, since Yahoo reports the
/// previous session's volume until the market opens. `None` outside of
/// pre-market.
pub struct PreMarketVolume {
    symbol: String,
    asset_class: AssetClass,
}

impl PreMarketVolume {
    pub fn new(symbol: String, asset_class: AssetClass) -> PreMarketVolume {
        PreMarketVolume {
            symbol,
            asset_class,
        }
    }
}

impl AsyncTask for PreMarketVolume {
    type Input = (String, AssetClass);
    type Response = Option<u64>;

    /// Nasdaq updates pre-market data every 30 seconds
    fn update_interval(&self) -> Option<Duration> {
        Some(Duration::from_secs(15))
    }

    fn input(&self) -> Self::Input {
        (self.symbol.clone(), self.asset_class)
    }

    fn task<'a>(input: Arc<Self::Input>) -> BoxFuture<'a, Result<Self::Response>> {
        Box::pin(async move {
            let (symbol, asset_class) = input.as_ref();

            crate::CLIENT
                .get_pre_market_volume(symbol, *asset_class)
                .await
        })
    }
}
