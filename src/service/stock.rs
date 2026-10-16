use super::*;
use crate::api::model::{ChartMeta, CompanyData};
use crate::api::AssetClass;
use crate::common::*;
use crate::task::*;

pub struct StockService {
    symbol: String,
    current_price_handle: AsyncTaskHandle<(Quote, CompanyData)>,
    prices_handle: AsyncTaskHandle<(TimeFrame, ChartMeta, Vec<Price>)>,
    /// Only for US listings, started once their asset class is known
    nasdaq_handle: Option<AsyncTaskHandle<NasdaqData>>,
}

impl StockService {
    pub fn new(symbol: String, time_frame: TimeFrame) -> StockService {
        let task = CurrentPrice::new(symbol.clone());
        let current_price_handle = task.connect();

        let task = Prices::new(symbol.clone(), time_frame);
        let prices_handle = task.connect();

        StockService {
            symbol,
            current_price_handle,
            prices_handle,
            nasdaq_handle: None,
        }
    }

    /// Starts fetching Nasdaq's quote, if not already
    pub fn track_nasdaq_quote(&mut self, asset_class: AssetClass) {
        if self.nasdaq_handle.is_none() {
            let task = NasdaqQuote::new(self.symbol.clone(), asset_class);

            self.nasdaq_handle = Some(task.connect());
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
    NasdaqQuote(NasdaqData),
    CurrentPriceError(String),
    PricesError(String),
}

impl Service for StockService {
    type Update = Update;

    fn updates(&self) -> Vec<Self::Update> {
        let mut updates = vec![];

        for result in self.current_price_handle.response().try_iter() {
            match result {
                Ok((quote, company_data)) => {
                    updates.push(Update::NewPrice(quote));
                    updates.push(Update::CompanyData(Box::new(company_data)));
                }
                Err(error) => updates.push(Update::CurrentPriceError(error)),
            }
        }

        let prices_updates = self
            .prices_handle
            .response()
            .try_iter()
            .map(|result| result.map_or_else(Update::PricesError, Update::Prices));
        updates.extend(prices_updates);

        // Errors keep the last quote, it's refetched soon
        if let Some(handle) = self.nasdaq_handle.as_ref() {
            let nasdaq_updates = handle
                .response()
                .try_iter()
                .filter_map(Result::ok)
                .map(Update::NasdaqQuote);
            updates.extend(nasdaq_updates);
        }

        updates
    }

    /// Keeps updating the quote in the background, since the tabs show the
    /// change of every stock
    fn pause(&self) {
        self.current_price_handle.background();
        self.prices_handle.pause();

        if let Some(handle) = self.nasdaq_handle.as_ref() {
            handle.pause();
        }
    }

    fn resume(&self) {
        self.current_price_handle.resume();
        self.prices_handle.resume();

        if let Some(handle) = self.nasdaq_handle.as_ref() {
            handle.resume();
        }
    }
}
