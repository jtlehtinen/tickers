use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::canvas::{Canvas, Line, Rectangle};
use ratatui::widgets::{Block, Borders, StatefulWidget, Widget};

use super::x_axis::{column, render_session_bounds, render_ticks, ChartAreas, XAxis};
use super::y_axis::render_y_labels;
use crate::common::{Price, TimeFrame};
use crate::draw::{add_padding, PaddingDirection};
use crate::theme::style;
use crate::widget::StockState;
use crate::{HIDE_PREV_CLOSE, THEME};

#[derive(Debug)]
struct Candle {
    open: f64,
    close: f64,
    high: f64,
    low: f64,
}

pub struct PricesCandlestickChart<'a> {
    pub loaded: bool,
    pub data: &'a [Price],
    pub is_summary: bool,
    pub show_x_labels: bool,
    pub show_y_labels: bool,
    pub x_axis: &'a XAxis,
}

impl StatefulWidget for PricesCandlestickChart<'_> {
    type State = StockState;

    fn render(self, mut area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if area.width <= 9 || area.height <= 3 {
            return;
        }

        if !self.is_summary {
            Block::default()
                .borders(Borders::TOP)
                .border_style(style().fg(THEME.border_secondary()))
                .render(area, buf);
            area = add_padding(area, 1, PaddingDirection::Top);
        }

        let (min, max) = state.chart_bounds(self.data, true);

        let areas = ChartAreas::new(area, self.show_x_labels);

        // Draw y labels
        if self.loaded && self.show_y_labels {
            let y_area = areas.y_labels;

            render_y_labels(buf, y_area, state, min, max);
        }

        // Each candle is 2 columns wide and covers an equal part of the x axis
        let num_candles = areas.plot.width / 2;
        let num_slots = self.x_axis.num_slots();

        let mut candles: Vec<Option<Candle>> = (0..num_candles).map(|_| None).collect();

        let current_price = state.current_price();
        let last_idx = self.data.len().saturating_sub(1);

        for (idx, price) in self.data.iter().enumerate() {
            let slot = self.x_axis.slot(idx);

            if price.close <= 0.0 || !(0.0..num_slots).contains(&slot) {
                continue;
            }

            let mut price = *price;

            // The latest price is still forming, use the current price for it
            if idx == last_idx && current_price > 0.0 {
                price.close = current_price;
                price.high = price.high.max(current_price);
                price.low = price.low.min(current_price);
            }

            let bucket = ((slot / num_slots) * f64::from(num_candles)) as usize;

            match candles.get_mut(bucket) {
                Some(Some(candle)) => {
                    candle.close = price.close;
                    candle.high = candle.high.max(price.high);
                    candle.low = candle.low.min(price.low);
                }
                Some(candle @ None) => {
                    *candle = Some(Candle {
                        open: price.open,
                        close: price.close,
                        high: price.high,
                        low: price.low,
                    });
                }
                None => {}
            }
        }

        // Wicks cut off by the top or bottom of the chart, as the fraction of the
        // x axis they're at, whether they're cut off at the top, and their color
        let clipped_wicks = candles
            .iter()
            .enumerate()
            .filter_map(|(idx, candle)| Some((idx, candle.as_ref()?)))
            .flat_map(|(idx, candle)| {
                let fraction = (idx as f64 * 4.0 + 2.0) / (f64::from(num_candles) * 4.0);
                let color = candle_color(candle);

                [
                    (candle.high > max).then_some((fraction, true, color)),
                    (candle.low < min).then_some((fraction, false, color)),
                ]
            })
            .flatten()
            .collect::<Vec<_>>();

        if self.loaded {
            Canvas::default()
                .background_color(THEME.background())
                .block(
                    Block::default()
                        .style(style())
                        .borders(if self.show_x_labels {
                            Borders::LEFT | Borders::BOTTOM
                        } else {
                            Borders::LEFT
                        })
                        .border_style(style().fg(THEME.border_axis())),
                )
                .x_bounds([0.0, num_candles as f64 * 4.0])
                .y_bounds(state.y_bounds(min, max))
                .paint(move |ctx| {
                    if let (true, true, false, Some(prev_close_price)) = (
                        state.time_frame == TimeFrame::Day1,
                        self.loaded,
                        *HIDE_PREV_CLOSE,
                        state.prev_close_price,
                    ) {
                        ctx.draw(&Line {
                            x1: 0.0,
                            x2: num_candles as f64 * 4.0,
                            y1: prev_close_price,
                            y2: prev_close_price,
                            color: THEME.gray(),
                        })
                    }

                    ctx.layer();

                    for (idx, candle) in candles.iter().enumerate() {
                        if let Some(candle) = candle {
                            let color = candle_color(candle);

                            ctx.draw(&Rectangle {
                                x: idx as f64 * 4.0 + 1.0,
                                y: candle.open.min(candle.close),
                                width: 2.0,
                                height: candle.open.max(candle.close)
                                    - candle.open.min(candle.close),
                                color,
                            });

                            ctx.draw(&Line {
                                x1: idx as f64 * 4.0 + 2.0,
                                x2: idx as f64 * 4.0 + 2.0,
                                y1: candle.low,
                                y2: candle.open.min(candle.close),
                                color,
                            });

                            ctx.draw(&Line {
                                x1: idx as f64 * 4.0 + 2.0,
                                x2: idx as f64 * 4.0 + 2.0,
                                y1: candle.high,
                                y2: candle.open.max(candle.close),
                                color,
                            });
                        }
                    }
                })
                .render(areas.chart, buf);

            // Mark where wicks go beyond the chart
            let plot = areas.plot;
            for (fraction, above, color) in clipped_wicks {
                if let (Some(column), true) = (column(fraction, plot.width), plot.height > 0) {
                    let (row, marker) = if above {
                        (plot.top(), "▲")
                    } else {
                        (plot.bottom() - 1, "▼")
                    };

                    buf.get_mut(plot.x + column, row)
                        .set_symbol(marker)
                        .set_fg(color);
                }
            }
        } else {
            Block::default()
                .borders(if self.show_x_labels {
                    Borders::LEFT | Borders::BOTTOM
                } else {
                    Borders::LEFT
                })
                .border_style(style().fg(THEME.border_axis()))
                .render(areas.chart, buf);

            render_session_bounds(buf, areas.plot, self.x_axis, state);
        }

        // Draw x labels, after the chart so tick marks go on top of its axis line
        if let (Some(x_labels), true) = (areas.x_labels, self.loaded) {
            let ticks = self.x_axis.ticks(areas.plot.width);
            render_ticks(buf, areas.plot, x_labels, &ticks);
        }
    }
}

fn candle_color(candle: &Candle) -> ratatui::style::Color {
    if candle.close.gt(&candle.open) {
        THEME.profit()
    } else {
        THEME.loss()
    }
}
