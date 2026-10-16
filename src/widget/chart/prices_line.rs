use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::symbols::Marker;
use ratatui::widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, StatefulWidget, Widget};

use super::x_axis::{render_session_bounds, render_ticks, ChartAreas, XAxis};
use super::y_axis::render_y_labels;
use crate::common::{cast_historical_as_price, zeros_as_pre, Price, TimeFrame, TradingPeriod};
use crate::draw::{add_padding, PaddingDirection};
use crate::theme::style;
use crate::widget::StockState;
use crate::{HIDE_PREV_CLOSE, THEME};

pub struct PricesLineChart<'a> {
    pub loaded: bool,
    pub enable_pre_post: bool,
    pub show_x_labels: bool,
    pub show_y_labels: bool,
    pub is_profit: bool,
    pub is_summary: bool,
    pub data: &'a [Price],
    pub x_axis: &'a XAxis,
}

impl StatefulWidget for PricesLineChart<'_> {
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

        // Only closes are drawn, so highs and lows don't need to be in range
        let (min, max) = state.chart_bounds(self.data, false);
        let x_bounds = self.x_axis.bounds();

        let mut prices: Vec<_> = self.data.iter().map(cast_historical_as_price).collect();

        // The latest price is still forming, use the current price for it. Every
        // point needs a position on the x axis, so only replace existing prices.
        if let Some(last) = prices.last_mut() {
            *last = state.current_price();
        }
        zeros_as_pre(&mut prices);

        // Need more than one price for GraphType::Line to work
        let graph_type = if prices.len() <= 2 {
            GraphType::Scatter
        } else {
            GraphType::Line
        };

        let trading_period = state.current_trading_period(self.data);

        // Prices are plotted where their time is on the x axis
        let to_point = |(idx, price): (usize, &f64)| (self.x_axis.x(idx), *price);

        let (reg_prices, pre_prices, post_prices) = if self.loaded {
            let (start_idx, end_idx) = state.regular_start_end_idx(self.data);

            if self.enable_pre_post && state.time_frame == TimeFrame::Day1 {
                (
                    prices
                        .iter()
                        .enumerate()
                        .filter(|(idx, _)| {
                            if let Some(start) = start_idx {
                                *idx >= start
                            } else {
                                false
                            }
                        })
                        .filter(|(idx, _)| {
                            if let Some(end) = end_idx {
                                *idx <= end
                            } else {
                                true
                            }
                        })
                        .map(to_point)
                        .collect::<Vec<(f64, f64)>>(),
                    {
                        let pre_end_idx = if let Some(start_idx) = start_idx {
                            start_idx
                        } else {
                            prices.len()
                        };

                        if pre_end_idx > 0 {
                            Some(
                                prices
                                    .iter()
                                    .enumerate()
                                    .filter(|(idx, _)| *idx <= pre_end_idx)
                                    .map(to_point)
                                    .collect::<Vec<(f64, f64)>>(),
                            )
                        } else {
                            None
                        }
                    },
                    {
                        end_idx.map(|post_start_idx| {
                            prices
                                .iter()
                                .enumerate()
                                .filter(|(idx, _)| *idx >= post_start_idx)
                                .map(to_point)
                                .collect::<Vec<(f64, f64)>>()
                        })
                    },
                )
            } else {
                (
                    prices
                        .iter()
                        .enumerate()
                        .map(to_point)
                        .collect::<Vec<(f64, f64)>>(),
                    None,
                    None,
                )
            }
        } else {
            (vec![], None, None)
        };

        let prev_close_line = state
            .prev_close_price
            .filter(|_| state.time_frame == TimeFrame::Day1 && self.loaded && !*HIDE_PREV_CLOSE)
            .map(|prev_close| vec![(x_bounds[0], prev_close), (x_bounds[1], prev_close)]);

        let mut datasets = vec![Dataset::default()
            .marker(Marker::Braille)
            .style(style().fg(
                if trading_period != TradingPeriod::Regular && self.enable_pre_post {
                    THEME.gray()
                } else if self.is_profit {
                    THEME.profit()
                } else {
                    THEME.loss()
                },
            ))
            .graph_type(graph_type)
            .data(&reg_prices)];

        if let Some(data) = post_prices.as_ref() {
            datasets.push(
                Dataset::default()
                    .marker(Marker::Braille)
                    .style(style().fg(if trading_period != TradingPeriod::Post {
                        THEME.gray()
                    } else if self.is_profit {
                        THEME.profit()
                    } else {
                        THEME.loss()
                    }))
                    .graph_type(GraphType::Line)
                    .data(data),
            );
        }

        if let Some(data) = pre_prices.as_ref() {
            datasets.insert(
                0,
                Dataset::default()
                    .marker(Marker::Braille)
                    .style(style().fg(if trading_period != TradingPeriod::Pre {
                        THEME.gray()
                    } else if self.is_profit {
                        THEME.profit()
                    } else {
                        THEME.loss()
                    }))
                    .graph_type(GraphType::Line)
                    .data(data),
            );
        }

        if let Some(data) = prev_close_line.as_ref() {
            datasets.insert(
                0,
                Dataset::default()
                    .marker(Marker::Braille)
                    .style(style().fg(THEME.gray()))
                    .graph_type(GraphType::Line)
                    .data(data),
            );
        }

        let chart = Chart::new(datasets)
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
            .style(style())
            .x_axis(Axis::default().bounds(x_bounds))
            .y_axis(Axis::default().bounds(state.y_bounds(min, max)));

        let areas = ChartAreas::new(area, self.show_x_labels);

        // Draw y labels
        if self.loaded && self.show_y_labels {
            let y_area = areas.y_labels;

            render_y_labels(buf, y_area, state, min, max);
        }

        chart.render(areas.chart, buf);

        if self.loaded {
            render_session_bounds(buf, areas.plot, self.x_axis, state);
        }

        // Draw x labels, after the chart so tick marks go on top of its axis line
        if let (Some(x_labels), true) = (areas.x_labels, self.loaded) {
            let ticks = self.x_axis.ticks(areas.plot.width);
            render_ticks(buf, areas.plot, x_labels, &ticks);
        }
    }
}
