use super::*;
use crate::api::model::{ChartMeta, CompanyData};
use crate::api::AssetClass;
use crate::common::*;
use crate::task::*;

pub struct StockService {
    symbol: String,
    current_price_handle: AsyncTaskHandle<Quote>,
    prices_handle: AsyncTaskHandle<(TimeFrame, ChartMeta, Vec<Price>)>,
    company_handle: AsyncTaskHandle<CompanyData>,
    /// Only for US listings, started once their asset class is known
    pre_market_volume_handle: Option<AsyncTaskHandle<Option<u64>>>,
}

impl StockService {
    pub fn new(symbol: String, time_frame: TimeFrame) -> StockService {
        let task = CurrentPrice::new(symbol.clone());
        let current_price_handle = task.connect();

        let task = Prices::new(symbol.clone(), time_frame);
        let prices_handle = task.connect();

        let task = Company::new(symbol.clone());
        let company_handle = task.connect();

        StockService {
            symbol,
            current_price_handle,
            prices_handle,
            company_handle,
            pre_market_volume_handle: None,
        }
    }

    /// Starts fetching the pre-market volume, if not already
    pub fn track_pre_market_volume(&mut self, asset_class: AssetClass) {
        if self.pre_market_volume_handle.is_none() {
            let task = PreMarketVolume::new(self.symbol.clone(), asset_class);

            self.pre_market_volume_handle = Some(task.connect());
        }
    }

    pub fn update_time_frame(&mut self, time_frame: TimeFrame) {
        let task = Prices::new(self.symbol.clone(), time_frame);
        let prices_handle = task.connect();

        self.prices_handle = prices_handle;
    }
}

#[derive(Debug)]
pub enum Update {
    NewPrice(Quote),
    Prices((TimeFrame, ChartMeta, Vec<Price>)),
    CompanyData(Box<CompanyData>),
    PreMarketVolume(Option<u64>),
    CurrentPriceError(String),
    PricesError(String),
}

impl Service for StockService {
    type Update = Update;

    fn updates(&self) -> Vec<Self::Update> {
        let mut updates = vec![];

        let current_price_updates = self
            .current_price_handle
            .response()
            .try_iter()
            .map(|result| result.map_or_else(Update::CurrentPriceError, Update::NewPrice));
        updates.extend(current_price_updates);

        let prices_updates = self
            .prices_handle
            .response()
            .try_iter()
            .map(|result| result.map_or_else(Update::PricesError, Update::Prices));
        updates.extend(prices_updates);

        let company_updates = self
            .company_handle
            .response()
            .try_iter()
            .filter_map(Result::ok)
            .map(Box::new)
            .map(Update::CompanyData);
        updates.extend(company_updates);

        // Errors keep the last volume, it's refetched soon
        if let Some(handle) = self.pre_market_volume_handle.as_ref() {
            let pre_market_volume_updates = handle
                .response()
                .try_iter()
                .filter_map(Result::ok)
                .map(Update::PreMarketVolume);
            updates.extend(pre_market_volume_updates);
        }

        updates
    }

    /// Keeps updating the quote in the background, since the tabs show the
    /// change of every stock
    fn pause(&self) {
        self.current_price_handle.background();
        self.prices_handle.pause();
        self.company_handle.pause();

        if let Some(handle) = self.pre_market_volume_handle.as_ref() {
            handle.pause();
        }
    }

    fn resume(&self) {
        self.current_price_handle.resume();
        self.prices_handle.resume();
        self.company_handle.resume();

        if let Some(handle) = self.pre_market_volume_handle.as_ref() {
            handle.resume();
        }
    }
}
