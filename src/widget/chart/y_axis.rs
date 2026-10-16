use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Span;

use crate::common::{price_decimals, TimeFrame};
use crate::theme::style;
use crate::widget::StockState;
use crate::{HIDE_PREV_CLOSE, THEME};

/// Least number of rows from one price label to the next
const MIN_LABEL_SPACING: u16 = 5;

/// Most digits after the decimal point of a price label
const MAX_DECIMALS: usize = 8;

/// Draws price labels at round prices, and the previous close on the row of
/// its line, in the y label gutter `area` of a chart with prices from `min` to
/// `max`
pub fn render_y_labels(buf: &mut Buffer, area: Rect, state: &StockState, min: f64, max: f64) {
    if area.height == 0 {
        return;
    }

    let prev_close = state
        .prev_close_price
        .filter(|_| state.time_frame == TimeFrame::Day1 && !*HIDE_PREV_CLOSE);
    let prev_close_row = prev_close.map(|price| row(price, min, max, area.height));

    let (prices, decimals) = label_prices(min, max, area.height);

    for price in prices {
        let row = row(price, min, max, area.height);

        // The previous close takes the row over
        if prev_close_row != Some(row) {
            let label = Span::styled(
                format!("{:>8.*}", decimals, price),
                style().fg(THEME.text_normal()),
            );
            buf.set_span(area.left(), area.top() + row, &label, area.width);
        }
    }

    if let (Some(price), Some(row)) = (prev_close, prev_close_row) {
        let label = Span::styled(
            format!("{:>8.*}", price_decimals(price), price),
            style().fg(THEME.text_normal()).bg(THEME.gray()),
        );
        buf.set_span(area.left(), area.top() + row, &label, area.width);
    }
}

/// Round prices to label on a chart `height` rows tall with prices from `min`
/// to `max`, and the decimals to show them with. Labels are a multiple of a
/// step of 1, 2 or 5 times a power of ten, the smallest step spacing them at
/// least `MIN_LABEL_SPACING` rows apart (or less on short charts, so there are
/// at least two labels).
fn label_prices(min: f64, max: f64, height: u16) -> (Vec<f64>, usize) {
    let range = max - min;

    if height == 0 || !range.is_finite() || range <= 0.0 {
        return (vec![min], price_decimals(min));
    }

    let spacing = MIN_LABEL_SPACING.min(height / 2).max(1);
    let rows_per_unit = (f64::from(height) * 4.0 - 1.0) / 4.0 / range;

    let mut step = Step::at_least(f64::from(spacing) / rows_per_unit);

    loop {
        // Allow for rounding errors, e.g. 0.00001 / 0.000005 being just above 2
        let first = (min / step.value() - 1e-9).ceil() as i64;
        let last = (max / step.value() + 1e-9).floor() as i64;

        if last - first >= 1 || step.exponent < -(MAX_DECIMALS as i32) {
            let prices = (first..=last).map(|n| n as f64 * step.value()).collect();
            let decimals = (-step.exponent).clamp(0, MAX_DECIMALS as i32) as usize;

            return (prices, decimals);
        }

        step = step.smaller();
    }
}

/// Step between labels, `mantissa` (1, 2 or 5) times 10 to the `exponent`
#[derive(Debug, Clone, Copy, PartialEq)]
struct Step {
    mantissa: u8,
    exponent: i32,
}

impl Step {
    /// Smallest step of at least `min`
    fn at_least(min: f64) -> Step {
        let exponent = min.log10().floor() as i32;

        let step = [1, 2, 5]
            .into_iter()
            .map(|mantissa| Step { mantissa, exponent })
            .find(|step| step.value() >= min * (1.0 - 1e-9));

        step.unwrap_or(Step {
            mantissa: 1,
            exponent: exponent + 1,
        })
    }

    /// The next smaller step
    fn smaller(self) -> Step {
        match self.mantissa {
            5 => Step {
                mantissa: 2,
                ..self
            },
            2 => Step {
                mantissa: 1,
                ..self
            },
            _ => Step {
                mantissa: 5,
                exponent: self.exponent - 1,
            },
        }
    }

    fn value(self) -> f64 {
        f64::from(self.mantissa) * 10f64.powi(self.exponent)
    }
}

/// Row (from the top) that `price` is drawn on in a braille canvas `height`
/// rows tall, with y bounds of `min` to `max`. Matches ratatui's mapping of
/// points to the 4 dot rows of each cell.
fn row(price: f64, min: f64, max: f64, height: u16) -> u16 {
    let dots = f64::from(height) * 4.0 - 1.0;
    let range = max - min;

    let dot = if range > 0.0 {
        ((max - price) / range * dots).clamp(0.0, dots)
    } else {
        0.0
    };

    (dot as u16 / 4).min(height - 1)
}

#[cfg(test)]
mod tests {
    use super::{label_prices, row, Step};

    #[test]
    fn max_is_on_the_top_row() {
        assert_eq!(row(110.0, 100.0, 110.0, 10), 0);
    }

    #[test]
    fn min_is_on_the_bottom_row() {
        assert_eq!(row(100.0, 100.0, 110.0, 10), 9);
    }

    #[test]
    fn middle_is_in_the_middle() {
        assert_eq!(row(105.0, 100.0, 110.0, 10), 4);
    }

    #[test]
    fn flat_prices_are_on_the_top_row() {
        assert_eq!(row(100.0, 100.0, 100.0, 10), 0);
    }

    fn assert_prices(min: f64, max: f64, height: u16, expected: &[f64], decimals: usize) {
        let (prices, actual_decimals) = label_prices(min, max, height);

        assert_eq!(prices.len(), expected.len(), "{:?}", prices);
        for (price, expected) in prices.iter().zip(expected) {
            assert!((price - expected).abs() < 1e-9, "{:?}", prices);
        }
        assert_eq!(actual_decimals, decimals);
    }

    #[test]
    fn labels_are_round_prices() {
        // MU's 1D chart, 40 rows tall
        assert_prices(914.5, 937.53, 40, &[915.0, 920.0, 925.0, 930.0, 935.0], 0);
    }

    #[test]
    fn labels_have_the_decimals_of_their_step() {
        assert_prices(
            9.95,
            10.28,
            40,
            &[9.95, 10.0, 10.05, 10.1, 10.15, 10.2, 10.25],
            2,
        );
        assert_prices(0.00001, 0.00002, 20, &[0.00001, 0.000015, 0.00002], 6);
    }

    #[test]
    fn labels_are_spaced_apart() {
        let (prices, _) = label_prices(100.0, 200.0, 40);

        let rows = prices
            .iter()
            .map(|price| row(*price, 100.0, 200.0, 40))
            .collect::<Vec<_>>();

        assert!(
            rows.windows(2).all(|rows| rows[0] - rows[1] >= 5),
            "{:?}",
            rows
        );
    }

    #[test]
    fn short_charts_have_two_labels() {
        assert_prices(914.5, 937.53, 6, &[920.0, 930.0], 0);
        assert_prices(914.5, 937.53, 2, &[920.0, 930.0], 0);
    }

    #[test]
    fn flat_prices_have_one_label() {
        assert_prices(5.0, 5.0, 10, &[5.0], 2);
    }

    #[test]
    fn steps() {
        assert_eq!(
            Step::at_least(2.9),
            Step {
                mantissa: 5,
                exponent: 0
            }
        );
        assert_eq!(
            Step::at_least(0.04),
            Step {
                mantissa: 5,
                exponent: -2
            }
        );
        assert_eq!(
            Step::at_least(6.0),
            Step {
                mantissa: 1,
                exponent: 1
            }
        );
        assert_eq!(
            Step::at_least(20.0),
            Step {
                mantissa: 2,
                exponent: 1
            }
        );
    }
}
