use std::hash::{Hash, Hasher};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::canvas::{Canvas, Line};
use ratatui::widgets::{Block, Borders, StatefulWidget, Widget};
use serde::Deserialize;

use super::x_axis::{kagi_ticks, render_ticks, ChartAreas};
use super::y_axis::render_y_labels;
use crate::common::{Price, TimeFrame};
use crate::draw::{add_padding, PaddingDirection};
use crate::theme::style;
use crate::widget::chart_configuration::{KagiOptions, KagiReversalOption};
use crate::widget::StockState;
use crate::{HIDE_PREV_CLOSE, THEME};

#[derive(Debug, Clone, Copy)]
struct Trend {
    direction: TrendDirection,
    first_price: Price,
    last_price: Price,
    breakpoint: Option<Breakpoint>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TrendDirection {
    Up,
    Down,
}

impl TrendDirection {
    fn reverse(self) -> TrendDirection {
        match self {
            TrendDirection::Up => TrendDirection::Down,
            TrendDirection::Down => TrendDirection::Up,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Breakpoint {
    price: Price,
    kind: BreakpointKind,
}

#[derive(Debug, Clone, Copy)]
enum BreakpointKind {
    Yang,
    Ying,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ReversalOption {
    #[serde(rename = "pct")]
    Pct(f64),
    #[serde(rename = "amount")]
    Amount(f64),
}

impl Hash for ReversalOption {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            ReversalOption::Pct(amount) => {
                0.hash(state);
                amount.to_bits().hash(state);
            }
            ReversalOption::Amount(amount) => {
                1.hash(state);
                amount.to_bits().hash(state);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Hash, Deserialize)]
pub enum PriceOption {
    #[serde(rename = "close")]
    Close,
    #[serde(rename = "high_low")]
    HighLow,
}

#[derive(Clone, Copy)]
enum ComparisonType {
    Gt,
    Lt,
}

fn choose_price(price: &Price, option: PriceOption, comparison: ComparisonType) -> f64 {
    match option {
        PriceOption::Close => price.close,
        PriceOption::HighLow => match comparison {
            ComparisonType::Gt => price.high,
            ComparisonType::Lt => price.low,
        },
    }
}

fn calculate_trends(
    data: &[Price],
    reversal_option: ReversalOption,
    price_option: PriceOption,
) -> Vec<Trend> {
    let mut trends = vec![];

    // Filter out 0 prices
    let data = match price_option {
        PriceOption::Close => data.iter().filter(|p| p.close.gt(&0.0)).collect::<Vec<_>>(),
        PriceOption::HighLow => data.iter().filter(|p| p.low.gt(&0.0)).collect::<Vec<_>>(),
    };

    // Exit if data is empty
    if data.is_empty() {
        return trends;
    }

    let first_price = **data.first().unwrap();

    // Find initial trend direction
    let mut initial_direction = TrendDirection::Up;
    for price in data[1..].iter() {
        let first_price_gt = choose_price(&first_price, price_option, ComparisonType::Gt);
        let first_price_lt = choose_price(&first_price, price_option, ComparisonType::Lt);

        let price_gt = choose_price(price, price_option, ComparisonType::Gt);
        let price_lt = choose_price(price, price_option, ComparisonType::Lt);

        if price_gt.gt(&first_price_gt) {
            initial_direction = TrendDirection::Up;
            break;
        } else if price_lt.lt(&first_price_lt) {
            initial_direction = TrendDirection::Down;
            break;
        }
    }

    let mut curr_trend: Trend = Trend {
        direction: initial_direction,
        first_price,
        last_price: first_price,
        breakpoint: None,
    };

    for (idx, price) in data[1..].iter().enumerate() {
        let (reversal_amount, diff) = {
            let current_price = match curr_trend.direction {
                TrendDirection::Up => choose_price(price, price_option, ComparisonType::Lt),
                TrendDirection::Down => choose_price(price, price_option, ComparisonType::Gt),
            };
            let last_price = match curr_trend.direction {
                TrendDirection::Up => {
                    choose_price(&curr_trend.last_price, price_option, ComparisonType::Gt)
                }
                TrendDirection::Down => {
                    choose_price(&curr_trend.last_price, price_option, ComparisonType::Lt)
                }
            };

            match reversal_option {
                ReversalOption::Pct(reversal_amount) => {
                    (reversal_amount, current_price / last_price - 1.0)
                }
                ReversalOption::Amount(reversal_amount) => {
                    (reversal_amount, current_price - last_price)
                }
            }
        };

        let is_reversal = match curr_trend.direction {
            TrendDirection::Up => diff < -reversal_amount,
            TrendDirection::Down => diff > reversal_amount,
        };

        // Calculate breakpoint
        if let Some(prev_trend) = trends.last() {
            match curr_trend.direction {
                TrendDirection::Up => {
                    let current_price = choose_price(price, price_option, ComparisonType::Gt);
                    let breakpoint_price =
                        choose_price(&prev_trend.first_price, price_option, ComparisonType::Gt);

                    if current_price.gt(&breakpoint_price) {
                        curr_trend.breakpoint = Some(Breakpoint {
                            kind: BreakpointKind::Yang,
                            price: prev_trend.first_price,
                        })
                    }
                }
                TrendDirection::Down => {
                    let current_price = choose_price(price, price_option, ComparisonType::Lt);
                    let breakpoint_price =
                        choose_price(&prev_trend.first_price, price_option, ComparisonType::Lt);

                    if current_price.lt(&breakpoint_price) {
                        curr_trend.breakpoint = Some(Breakpoint {
                            kind: BreakpointKind::Ying,
                            price: prev_trend.first_price,
                        })
                    }
                }
            }
        }

        // Set last / low / high of trend where applicable
        match curr_trend.direction {
            TrendDirection::Up => {
                let current_price = choose_price(price, price_option, ComparisonType::Gt);
                let last_price =
                    choose_price(&curr_trend.last_price, price_option, ComparisonType::Gt);

                if current_price.gt(&last_price) {
                    curr_trend.last_price = **price;
                }
            }
            TrendDirection::Down => {
                let current_price = choose_price(price, price_option, ComparisonType::Lt);
                let last_price =
                    choose_price(&curr_trend.last_price, price_option, ComparisonType::Lt);

                if current_price.lt(&last_price) {
                    curr_trend.last_price = **price;
                }
            }
        }

        // Store this trend and start the next one
        if is_reversal || idx == data[1..].len() - 1 {
            trends.push(curr_trend);

            curr_trend = Trend {
                direction: curr_trend.direction.reverse(),
                first_price: curr_trend.last_price,
                last_price: **price,
                breakpoint: None,
            }
        }
    }

    trends
}

pub struct PricesKagiChart<'a> {
    pub loaded: bool,
    pub data: &'a [Price],
    pub is_summary: bool,
    pub show_x_labels: bool,
    pub show_y_labels: bool,
    pub kagi_options: KagiOptions,
}

impl PricesKagiChart<'_> {
    fn min_max(
        &self,
        data: &[Trend],
        time_frame: TimeFrame,
        prev_close_price: Option<f64>,
    ) -> (f64, f64) {
        let (mut max, mut min) = self.high_low(data);

        if time_frame == TimeFrame::Day1 && !*HIDE_PREV_CLOSE {
            if let Some(prev_close) = prev_close_price {
                if prev_close.le(&min) {
                    min = prev_close;
                }

                if prev_close.gt(&max) {
                    max = prev_close;
                }
            }
        }

        (min, max)
    }

    fn high_low(&self, data: &[Trend]) -> (f64, f64) {
        let high = data
            .iter()
            .map(|t| {
                if t.direction == TrendDirection::Up {
                    t.last_price
                } else {
                    t.first_price
                }
            })
            .max_by(|a, b| a.high.partial_cmp(&b.high).unwrap())
            .map(|p| p.high)
            .unwrap_or(1.0);
        let low = data
            .iter()
            .map(|t| {
                if t.direction == TrendDirection::Up {
                    t.first_price
                } else {
                    t.last_price
                }
            })
            .min_by(|a, b| a.low.partial_cmp(&b.low).unwrap())
            .map(|p| p.low)
            .unwrap_or(0.0);

        (high, low)
    }
}

impl StatefulWidget for PricesKagiChart<'_> {
    type State = StockState;

    fn render(self, mut area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if area.width <= 9 || area.height <= 3 {
            return;
        }

        let default_reversal_option = match state.time_frame {
            TimeFrame::Day1 => ReversalOption::Pct(0.01),
            _ => ReversalOption::Pct(0.04),
        };

        let reversal_option = self
            .kagi_options
            .reversal_option
            .as_ref()
            .map(|o| match o {
                KagiReversalOption::Single(option) => *option,
                KagiReversalOption::ByTimeFrame(options_by_timeframe) => options_by_timeframe
                    .get(&state.time_frame)
                    .copied()
                    .unwrap_or(default_reversal_option),
            })
            .unwrap_or(default_reversal_option);

        let price_option = self.kagi_options.price_option.unwrap_or(PriceOption::Close);

        let kagi_trends = calculate_trends(self.data, reversal_option, price_option);

        if !self.is_summary {
            Block::default()
                .borders(Borders::TOP)
                .border_style(style().fg(THEME.border_secondary()))
                .render(area, buf);
            area = add_padding(area, 1, PaddingDirection::Top);
        }

        let areas = ChartAreas::new(area, self.show_x_labels);

        let width = areas.plot.width;
        let num_trends_can_render = width as f64 / 1.5;
        let num_trends = kagi_trends.len() as f64;
        let max_offset = if num_trends > num_trends_can_render {
            (num_trends - num_trends_can_render).ceil() as usize
        } else {
            0
        };

        let chart_width = num_trends_can_render * 3.0;

        let offset = if self.is_summary {
            max_offset
        } else if let Some(chart_state) = state.chart_state_mut() {
            if let Some(direction) = chart_state.queued_scroll.take() {
                chart_state.scroll(direction, max_offset);
            }

            chart_state.offset(max_offset)
        } else {
            max_offset
        };

        let visible_trends =
            &kagi_trends[offset..offset + num_trends_can_render.min(num_trends).floor() as usize];

        let (min, max) = self.min_max(visible_trends, state.time_frame, state.prev_close_price);

        let first_direction = kagi_trends.first().map(|t| t.direction);
        let time_frame = state.time_frame;

        // Draw y labels
        if self.loaded && self.show_y_labels {
            let y_area = areas.y_labels;

            render_y_labels(buf, y_area, state, min, max);
        }

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
                .x_bounds([0.0, chart_width])
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
                            x2: chart_width,
                            y1: prev_close_price,
                            y2: prev_close_price,
                            color: THEME.gray(),
                        });
                    }

                    ctx.layer();

                    let mut color = if let Some(first_direction) = first_direction {
                        match first_direction {
                            TrendDirection::Up => THEME.profit(),
                            TrendDirection::Down => THEME.loss(),
                        }
                    } else {
                        THEME.profit()
                    };

                    for (idx, trend) in visible_trends.iter().enumerate() {
                        let start = choose_price(
                            &trend.first_price,
                            price_option,
                            if trend.direction == TrendDirection::Up {
                                ComparisonType::Lt
                            } else {
                                ComparisonType::Gt
                            },
                        );
                        let mid = if let Some(breakpoint) = &trend.breakpoint {
                            choose_price(
                                &breakpoint.price,
                                price_option,
                                if trend.direction == TrendDirection::Up {
                                    ComparisonType::Gt
                                } else {
                                    ComparisonType::Lt
                                },
                            )
                        } else {
                            choose_price(
                                &trend.last_price,
                                price_option,
                                if trend.direction == TrendDirection::Up {
                                    ComparisonType::Gt
                                } else {
                                    ComparisonType::Lt
                                },
                            )
                        };
                        let end = choose_price(
                            &trend.last_price,
                            price_option,
                            if trend.direction == TrendDirection::Up {
                                ComparisonType::Gt
                            } else {
                                ComparisonType::Lt
                            },
                        );

                        // Draw connector to prev line
                        ctx.draw(&Line {
                            x1: (idx as f64 * 3.0 - 1.0).max(0.0),
                            x2: idx as f64 * 3.0 + 2.0,
                            y1: start,
                            y2: start,
                            color,
                        });

                        // Draw through mid (mid = end if no breakpoint)
                        ctx.draw(&Line {
                            x1: idx as f64 * 3.0 + 2.0,
                            x2: idx as f64 * 3.0 + 2.0,
                            y1: start,
                            y2: mid,
                            color,
                        });

                        // If there's a midpoint, change colors and draw through end
                        if let Some(breakpoint) = &trend.breakpoint {
                            color = match breakpoint.kind {
                                BreakpointKind::Yang => THEME.profit(),
                                BreakpointKind::Ying => THEME.loss(),
                            };

                            ctx.draw(&Line {
                                x1: idx as f64 * 3.0 + 2.0,
                                x2: idx as f64 * 3.0 + 2.0,
                                y1: mid,
                                y2: end,
                                color,
                            });
                        }
                    }
                })
                .render(areas.chart, buf);
        } else {
            Block::default()
                .borders(if self.show_x_labels {
                    Borders::LEFT | Borders::BOTTOM
                } else {
                    Borders::LEFT
                })
                .border_style(style().fg(THEME.border_axis()))
                .render(areas.chart, buf);
        }

        // Draw x labels, after the chart so tick marks go on top of its axis line.
        // Each trend's vertical line is drawn at `idx * 3 + 2`.
        if let (Some(x_labels), true) = (areas.x_labels, self.loaded) {
            let dates = visible_trends
                .iter()
                .map(|t| t.first_price.date)
                .collect::<Vec<_>>();
            let fractions = (0..visible_trends.len())
                .map(|idx| (idx as f64 * 3.0 + 2.0) / chart_width)
                .collect::<Vec<_>>();

            let ticks = kagi_ticks(&dates, &fractions, offset, time_frame).select(width);
            render_ticks(buf, areas.plot, x_labels, &ticks);
        }
    }
}
