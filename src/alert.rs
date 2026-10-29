use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;

use anyhow::{format_err, Result};
use chrono::Local;
use serde::{Deserialize, Serialize};

use crate::common::format_price;

/// Which way the price has to move to reach an alert's target
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Triggers once the price is at or above the target
    Rise,
    /// Triggers once the price is at or below the target
    Fall,
}

impl Direction {
    /// The direction from `price` to `target`, `None` if it's already there
    fn towards(price: f64, target: f64) -> Option<Direction> {
        if price < target {
            Some(Direction::Rise)
        } else if price > target {
            Some(Direction::Fall)
        } else {
            None
        }
    }

    fn reached(self, price: f64, target: f64) -> bool {
        match self {
            Direction::Rise => price >= target,
            Direction::Fall => price <= target,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Rise => "≥",
            Direction::Fall => "≤",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    pub symbol: String,
    pub price: f64,
    /// `None` until a price of the symbol is known, to tell which way it has
    /// to move
    #[serde(default)]
    pub direction: Option<Direction>,
    /// Unix time the alert triggered at. Alerts trigger only once
    #[serde(default)]
    pub triggered_at: Option<i64>,
}

impl Alert {
    pub fn is_pending(&self) -> bool {
        self.triggered_at.is_none()
    }

    /// Whether the alert triggers at `price`, the symbol's latest price. The
    /// first price seen sets the direction if it isn't known yet
    fn check(&mut self, price: f64) -> bool {
        if !self.is_pending() {
            return false;
        }

        let direction = match self.direction {
            Some(direction) => direction,
            None => match Direction::towards(price, self.price) {
                Some(direction) => {
                    self.direction = Some(direction);
                    return false;
                }
                // Exactly at the target
                None => Direction::Rise,
            },
        };

        direction.reached(price, self.price)
    }
}

/// The price alerts, saved to the data directory so they persist between runs
#[derive(Debug, Default)]
pub struct Alerts {
    alerts: Vec<Alert>,
    path: Option<PathBuf>,
}

impl Alerts {
    /// Loads the saved alerts. Failing to, alerts still work but aren't saved,
    /// so the file isn't overwritten
    pub fn load() -> Alerts {
        match alerts_path().and_then(|path| Ok((read_alerts(&path)?, path))) {
            Ok((alerts, path)) => Alerts {
                alerts,
                path: Some(path),
            },
            Err(_) => Alerts::default(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Alert> {
        self.alerts.iter()
    }

    pub fn len(&self) -> usize {
        self.alerts.len()
    }

    /// Symbols of the alerts that haven't triggered yet
    pub fn pending_symbols(&self) -> impl Iterator<Item = &str> {
        self.alerts
            .iter()
            .filter(|alert| alert.is_pending())
            .map(|alert| alert.symbol.as_str())
    }

    /// Adds an alert for `symbol` at `price`. `current_price` is the symbol's
    /// latest price, if known, to tell which way the price has to move
    pub fn add(&mut self, symbol: String, price: f64, current_price: Option<f64>) {
        let mut alert = Alert {
            symbol,
            price,
            direction: current_price.and_then(|current| Direction::towards(current, price)),
            triggered_at: None,
        };

        // Already at the target
        if current_price == Some(price) {
            alert.direction = Some(Direction::Rise);
        }

        self.alerts.push(alert);
        self.save();
    }

    pub fn remove(&mut self, idx: usize) {
        if idx < self.alerts.len() {
            self.alerts.remove(idx);
            self.save();
        }
    }

    /// Checks the alerts of `symbol` against its latest `price`, returning
    /// the ones that triggered
    pub fn check(&mut self, symbol: &str, price: f64) -> Vec<Alert> {
        if price <= 0.0 || !price.is_finite() {
            return vec![];
        }

        let mut triggered = vec![];
        let mut changed = false;

        for alert in self.alerts.iter_mut().filter(|a| a.symbol == symbol) {
            let direction = alert.direction;

            if alert.check(price) {
                alert.triggered_at = Some(Local::now().timestamp());
                triggered.push(alert.clone());
            }

            changed |= alert.direction != direction || !alert.is_pending();
        }

        if changed {
            self.save();
        }

        triggered
    }

    fn save(&self) {
        if let Some(path) = &self.path {
            if let Ok(yaml) = serde_yaml::to_string(&self.alerts) {
                let _ = fs::write(path, yaml);
            }
        }
    }
}

fn alerts_path() -> Result<PathBuf> {
    let dir = dirs_next::data_dir()
        .ok_or_else(|| format_err!("Could not get data directory"))?
        .join("tickers");

    fs::create_dir_all(&dir)?;

    Ok(dir.join("alerts.yml"))
}

fn read_alerts(path: &PathBuf) -> Result<Vec<Alert>> {
    if !path.exists() {
        return Ok(vec![]);
    }

    Ok(serde_yaml::from_str::<Option<Vec<Alert>>>(&fs::read_to_string(path)?)?.unwrap_or_default())
}

/// Shows a desktop notification of a triggered alert, at `price`
pub fn notify(alert: &Alert, price: f64) {
    let summary = format!(
        "{} {} {}",
        alert.symbol,
        alert.direction.unwrap_or(Direction::Rise).as_str(),
        format_price(alert.price)
    );
    let body = format!("{} is at {}", alert.symbol, format_price(price));

    // Waited on in the background so it doesn't linger as a zombie process
    thread::spawn(move || {
        let _ = Command::new("notify-send")
            .args(["--app-name=tickers", "--urgency=critical", &summary, &body])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    });
}

/// Parses an alert entered as `SYMBOL PRICE`, or just `PRICE` for
/// `default_symbol`
pub fn parse_alert(input: &str, default_symbol: Option<&str>) -> Result<(String, f64), String> {
    let parts: Vec<_> = input.split_whitespace().collect();

    let (symbol, price) = match parts.as_slice() {
        [price] => (
            default_symbol.ok_or("Enter a symbol and a price")?.to_string(),
            *price,
        ),
        [symbol, price] => (symbol.to_ascii_uppercase(), *price),
        _ => return Err("Enter a symbol and a price, e.g. AAPL 200".into()),
    };

    match price.trim_start_matches('$').parse::<f64>() {
        Ok(price) if price > 0.0 && price.is_finite() => Ok((symbol, price)),
        _ => Err(format!("Invalid price: {}", price)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alert(price: f64, direction: Option<Direction>) -> Alert {
        Alert {
            symbol: "MU".into(),
            price,
            direction,
            triggered_at: None,
        }
    }

    #[test]
    fn rise_alerts_trigger_at_or_above_the_target() {
        assert!(!alert(100.0, Some(Direction::Rise)).check(99.99));
        assert!(alert(100.0, Some(Direction::Rise)).check(100.0));
        assert!(alert(100.0, Some(Direction::Rise)).check(105.0));
    }

    #[test]
    fn fall_alerts_trigger_at_or_below_the_target() {
        assert!(!alert(100.0, Some(Direction::Fall)).check(100.01));
        assert!(alert(100.0, Some(Direction::Fall)).check(100.0));
        assert!(alert(100.0, Some(Direction::Fall)).check(95.0));
    }

    #[test]
    fn first_price_sets_unknown_direction() {
        let mut a = alert(100.0, None);
        assert!(!a.check(90.0));
        assert_eq!(a.direction, Some(Direction::Rise));
        assert!(a.check(101.0));

        let mut a = alert(100.0, None);
        assert!(!a.check(110.0));
        assert_eq!(a.direction, Some(Direction::Fall));

        assert!(alert(100.0, None).check(100.0));
    }

    #[test]
    fn alerts_trigger_once() {
        let mut alerts = Alerts::default();
        alerts.add("MU".into(), 100.0, Some(90.0));
        alerts.add("NVDA".into(), 100.0, Some(90.0));

        assert!(alerts.check("MU", 95.0).is_empty());
        assert_eq!(alerts.check("MU", 100.5).len(), 1);
        assert!(alerts.check("MU", 101.0).is_empty());
        assert_eq!(alerts.pending_symbols().collect::<Vec<_>>(), ["NVDA"]);
    }

    #[test]
    fn parses_alerts() {
        assert_eq!(parse_alert("mu 120.5", None), Ok(("MU".into(), 120.5)));
        assert_eq!(parse_alert(" $99 ", Some("MU")), Ok(("MU".into(), 99.0)));
        assert!(parse_alert("99", None).is_err());
        assert!(parse_alert("MU abc", None).is_err());
        assert!(parse_alert("MU -5", None).is_err());
        assert!(parse_alert("", Some("MU")).is_err());
    }
}
