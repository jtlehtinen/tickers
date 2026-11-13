//! Detects rapid price moves: a rise or decline of over `THRESHOLD` within
//! `WINDOW`, reported as soon as the price crosses it

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use crate::alert::send_notification;
use crate::common::format_price;

const WINDOW: Duration = Duration::from_secs(5 * 60);
/// Fraction of the starting price
const THRESHOLD: f64 = 0.04;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Move {
    Rise,
    Decline,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RapidMove {
    pub symbol: String,
    pub direction: Move,
    pub from: f64,
    pub to: f64,
}

impl RapidMove {
    pub fn notify(&self) {
        let change = (self.to - self.from) / self.from * 100.0;
        let summary = format!("{} {:+.2}% in under 5 minutes", self.symbol, change);
        let verb = match self.direction {
            Move::Rise => "rose",
            Move::Decline => "declined",
        };
        let body = format!(
            "{} {} from {} to {}",
            self.symbol,
            verb,
            format_price(self.from),
            format_price(self.to)
        );
        send_notification(&summary, &body);
    }
}

/// Prices seen of each symbol in the last `WINDOW`
#[derive(Default)]
pub struct MoveMonitor {
    samples: HashMap<String, VecDeque<(Instant, f64)>>,
}

impl MoveMonitor {
    /// Records `price` of `symbol` seen at `now`, returning the move it
    /// completes, compared to the lowest / highest price in the window
    pub fn record(&mut self, symbol: &str, price: f64, now: Instant) -> Option<RapidMove> {
        if price <= 0.0 || !price.is_finite() {
            return None;
        }

        let samples = self.samples.entry(symbol.to_string()).or_default();

        while samples
            .front()
            .is_some_and(|(at, _)| now.saturating_duration_since(*at) >= WINDOW)
        {
            samples.pop_front();
        }

        let low = samples.iter().map(|(_, p)| *p).reduce(f64::min);
        let high = samples.iter().map(|(_, p)| *p).reduce(f64::max);

        let detected = match (low, high) {
            (Some(low), _) if price > low * (1.0 + THRESHOLD) => Some((Move::Rise, low)),
            (_, Some(high)) if price < high * (1.0 - THRESHOLD) => Some((Move::Decline, high)),
            _ => None,
        };

        // A reported move starts over from its price, so it's notified once
        if detected.is_some() {
            samples.clear();
        }
        samples.push_back((now, price));

        detected.map(|(direction, from)| RapidMove {
            symbol: symbol.to_string(),
            direction,
            from,
            to: price,
        })
    }

    /// Forgets the symbols no longer watched
    pub fn retain(&mut self, mut watched: impl FnMut(&str) -> bool) {
        self.samples.retain(|symbol, _| watched(symbol));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(start: Instant, s: u64) -> Instant {
        start + Duration::from_secs(s)
    }

    #[test]
    fn detects_rise_immediately_and_once() {
        let mut m = MoveMonitor::default();
        let t = Instant::now();
        assert!(m.record("MU", 100.0, t).is_none());
        assert!(m.record("MU", 103.0, secs(t, 30)).is_none());
        assert!(m.record("MU", 104.0, secs(t, 60)).is_none()); // exactly 4% isn't over
        let mv = m.record("MU", 104.1, secs(t, 90)).unwrap();
        assert_eq!(mv.direction, Move::Rise);
        assert_eq!(mv.from, 100.0);
        assert!(m.record("MU", 104.2, secs(t, 100)).is_none());
    }

    #[test]
    fn detects_decline_from_the_window_high() {
        let mut m = MoveMonitor::default();
        let t = Instant::now();
        m.record("MU", 100.0, t);
        m.record("MU", 105.0, secs(t, 60)); // rise of 5% reported
        let mv = m.record("MU", 100.5, secs(t, 120)).unwrap();
        assert_eq!(mv.direction, Move::Decline);
        assert_eq!(mv.from, 105.0);
    }

    #[test]
    fn ignores_moves_slower_than_the_window() {
        let mut m = MoveMonitor::default();
        let t = Instant::now();
        m.record("MU", 100.0, t);
        m.record("MU", 102.0, secs(t, 200));
        assert!(m.record("MU", 104.5, secs(t, 300)).is_none());
        assert!(m.record("NVDA", 104.5, secs(t, 300)).is_none());
    }
}
