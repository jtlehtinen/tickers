use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Span;

use crate::common::{price_decimals, TimeFrame};
use crate::theme::style;
use crate::widget::StockState;
use crate::{HIDE_PREV_CLOSE, THEME};

/// Draws the min, mid and max price labels, and the previous close on the
/// row of its line, in the y label gutter `area` of a chart with prices
/// from `min` to `max`
pub fn render_y_labels(buf: &mut Buffer, area: Rect, state: &StockState, min: f64, max: f64) {
    if area.height == 0 {
        return;
    }

    // Same decimals for every label, from the largest label so it fits,
    // unless all labels are below 1 and need more decimals
    let decimals = if max.abs() >= 1.0 {
        price_decimals(max)
    } else {
        price_decimals(min)
    };

    let prev_close = state
        .prev_close_price
        .filter(|_| state.time_frame == TimeFrame::Day1 && !*HIDE_PREV_CLOSE);
    let prev_close_row = prev_close.map(|price| row(price, min, max, area.height));

    let bottom = area.height - 1;
    let labels = [
        (min, bottom),
        ((min + max) / 2.0, bottom - bottom / 2),
        (max, 0),
    ];

    for (price, row) in labels {
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
            format!("{:>8.*}", decimals, price),
            style().fg(THEME.text_normal()).bg(THEME.gray()),
        );
        buf.set_span(area.left(), area.top() + row, &label, area.width);
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
    use super::row;

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
}
