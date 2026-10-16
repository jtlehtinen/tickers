use std::time::{Duration, Instant};

use anyhow::{format_err, Result};
use async_std::sync::{Arc, RwLock};
use async_std::task;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use futures::future::BoxFuture;
use lazy_static::lazy_static;
use parking_lot::Mutex;
use task::JoinHandle;

pub use self::company::Company;
pub use self::current_price::{CurrentPrice, MarketState, Offer, Quote};
pub use self::default_timestamps::DefaultTimestamps;
pub use self::options_data::OptionsData;
pub use self::options_dates::OptionsDates;
pub use self::pre_market_volume::PreMarketVolume;
pub use self::prices::Prices;
use crate::api::model::CrumbData;
use crate::api::Unauthorized;
use crate::{CLIENT, DATA_RECEIVED, UPDATE_INTERVAL};

mod company;
mod current_price;
mod default_timestamps;
mod options_data;
mod options_dates;
mod pre_market_volume;
mod prices;

/// How often one-shot tasks are retried until they succeed
const ONE_SHOT_RETRY_INTERVAL: Duration = Duration::from_secs(5);

/// Minimum time between attempts to fetch the Yahoo crumb
const CRUMB_RETRY_INTERVAL: Duration = Duration::from_secs(5);

lazy_static! {
    static ref YAHOO_CRUMB: RwLock<Option<CrumbData>> = Default::default();
    static ref LAST_CRUMB_FAILURE: Mutex<Option<(Instant, String)>> = Default::default();
}

/// Clears the cached crumb if `result` failed because Yahoo rejected it, so the
/// next request fetches a new one
pub async fn check_crumb<T>(result: Result<T>) -> Result<T> {
    if let Err(e) = &result {
        if e.is::<Unauthorized>() {
            YAHOO_CRUMB.write().await.take();
        }
    }

    result
}

/// Returns the Yahoo crumb, fetching it on first use.
///
/// Failed fetches are retried on a later call (throttled by
/// `CRUMB_RETRY_INTERVAL`), so a transient network error at startup doesn't
/// break every request needing the crumb for the rest of the session.
pub async fn crumb() -> Result<CrumbData> {
    if let Some(crumb) = YAHOO_CRUMB.read().await.clone() {
        return Ok(crumb);
    }

    let mut guard = YAHOO_CRUMB.write().await;

    // Another task may have fetched it while we waited for the lock
    if let Some(crumb) = guard.clone() {
        return Ok(crumb);
    }

    if let Some((failed_at, error)) = LAST_CRUMB_FAILURE.lock().as_ref() {
        if failed_at.elapsed() < CRUMB_RETRY_INTERVAL {
            return Err(format_err!("{}", error));
        }
    }

    match CLIENT.get_crumb().await {
        Ok(crumb) => {
            *guard = Some(crumb.clone());
            LAST_CRUMB_FAILURE.lock().take();

            Ok(crumb)
        }
        Err(e) => {
            *LAST_CRUMB_FAILURE.lock() = Some((Instant::now(), format!("{:#}", e)));

            Err(e)
        }
    }
}

/// Trait to define a type that spawns an Async Task to complete background
/// work.
pub trait AsyncTask: 'static {
    type Input: Send + Sync;
    type Response: Send;

    /// Interval that `task` should be executed at
    ///
    /// If `None` is returned, the task exits after it first succeeds (failed
    /// attempts are retried every `ONE_SHOT_RETRY_INTERVAL`)
    fn update_interval(&self) -> Option<Duration>;

    /// Input data needed for the `task`
    fn input(&self) -> Self::Input;

    /// Defines the async task that will get executed and return` Response`
    fn task<'a>(input: Arc<Self::Input>) -> BoxFuture<'a, Result<Self::Response>>;

    /// Runs the task on the async runtime and returns a handle to query updates from
    fn connect(&self) -> AsyncTaskHandle<Self::Response> {
        let (command_sender, command_receiver) = bounded(1);
        let (response_sender, response_receiver) = unbounded::<TaskResult<Self::Response>>();
        let data_received = DATA_RECEIVED.0.clone();

        let update_interval = self.update_interval();
        let input = Arc::new(self.input());

        let interval = update_interval
            .unwrap_or(ONE_SHOT_RETRY_INTERVAL)
            .max(Duration::from_secs(*UPDATE_INTERVAL));

        let handle = task::spawn(async move {
            let mut last_updated: Option<Instant> = None;

            let mut paused = false;

            loop {
                if let Ok(command) = command_receiver.try_recv() {
                    match command {
                        AsyncTaskCommand::Resume => paused = false,
                        AsyncTaskCommand::Pause => paused = true,
                    }
                }

                // Always execute the task initially, then every interval while not paused
                let is_due = match last_updated {
                    None => true,
                    Some(last_updated) => !paused && last_updated.elapsed() >= interval,
                };

                if is_due {
                    let result = <Self as AsyncTask>::task(input.clone()).await;
                    let succeeded = result.is_ok();

                    // Request a redraw to display this data
                    let _ = response_sender.send(result.map_err(|e| format!("{:#}", e)));
                    let _ = data_received.try_send(());

                    last_updated = Some(Instant::now());

                    // One-shot tasks are done once they succeed
                    if succeeded && update_interval.is_none() {
                        return;
                    }
                }

                // Free up some cycles
                task::sleep(Duration::from_millis(500)).await;
            }
        });

        AsyncTaskHandle {
            response: response_receiver,
            handle: Some(handle),
            command_sender,
        }
    }
}

enum AsyncTaskCommand {
    Pause,
    Resume,
}

/// Result of a task, the error being a displayable message
pub type TaskResult<R> = std::result::Result<R, String>;

pub struct AsyncTaskHandle<R> {
    response: Receiver<TaskResult<R>>,
    handle: Option<JoinHandle<()>>,
    command_sender: Sender<AsyncTaskCommand>,
}

impl<R> AsyncTaskHandle<R> {
    pub fn response(&self) -> &Receiver<TaskResult<R>> {
        &self.response
    }

    pub fn pause(&self) {
        let _ = self.command_sender.try_send(AsyncTaskCommand::Pause);
    }

    pub fn resume(&self) {
        let _ = self.command_sender.try_send(AsyncTaskCommand::Resume);
    }
}

impl<R> Drop for AsyncTaskHandle<R> {
    fn drop(&mut self) {
        let handle = self.handle.take().unwrap();
        task::spawn(async { handle.cancel().await });
    }
}
