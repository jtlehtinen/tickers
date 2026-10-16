use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, StatefulWidget, Widget};

use super::chart::{
    PricesCandlestickChart, PricesKagiChart, PricesLineChart, VolumeBarChart, XAxis,
};
use super::stock::StockState;
use super::{CachableWidget, CacheState};
use crate::common::{format_decimals, ChartType, TimeFrame};
use crate::draw::{add_padding, PaddingDirection};
use crate::theme::style;
use crate::widget::stock;
use crate::{ENABLE_PRE_POST, SHOW_VOLUMES, THEME};

pub struct StockSummaryWidget {}

impl StatefulWidget for StockSummaryWidget {
    type State = StockState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        self.render_cached(area, buf, state);
    }
}

impl CachableWidget<StockState> for StockSummaryWidget {
    fn cache_state_mut(state: &mut StockState) -> &mut CacheState {
        &mut state.cache_state
    }

    fn render(self, mut area: Rect, buf: &mut Buffer, state: &mut <Self as StatefulWidget>::State) {
        let data = state.prices().collect::<Vec<_>>();
        let x_axis = XAxis::new(state, &data);

        let pct_change = state.pct_change(&data);

        let chart_type = state.chart_type;
        let enable_pre_post = *ENABLE_PRE_POST.read();
        let show_volumes = *SHOW_VOLUMES.read() && chart_type != ChartType::Kagi;

        let loaded = state.loaded();

        let currency = match state.profile.as_ref() {
            Some(profile) => profile.price.currency.as_deref().unwrap_or("USD"),
            None => "",
        };

        let title = stock::get_chart_title(&area, state, true);

        Block::default()
            .title(Span::styled(title, style().fg(THEME.text_normal())))
            .borders(Borders::TOP)
            .border_style(style().fg(THEME.border_secondary()))
            .render(area, buf);
        area = add_padding(area, 1, PaddingDirection::Top);

        let mut layout: Vec<Rect> = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(25), Constraint::Min(0)].as_ref())
            .split(area)
            .to_vec();

        {
            layout[0] = add_padding(layout[0], 1, PaddingDirection::Left);
            layout[0] = add_padding(layout[0], 2, PaddingDirection::Right);

            let (high, low) = state.high_low(&data);
            let current_fmt = format_decimals(state.current_price());
            let high_fmt = format_decimals(high);
            let low_fmt = format_decimals(low);

            let (high_pct, low_pct) = if state.time_frame == TimeFrame::Day1 {
                if let Some(prev_close) = state.prev_close_price {
                    (Some(high / prev_close - 1.0), Some(low / prev_close - 1.0))
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };

            let vol = state.reg_mkt_volume.clone().unwrap_or_default();

            let prices = vec![
                Line::from(vec![
                    Span::styled("C: ", style().fg(THEME.text_normal())),
                    Span::styled(
                        if loaded {
                            format!("{}{}", currency, current_fmt)
                        } else {
                            "".to_string()
                        },
                        style()
                            .add_modifier(Modifier::BOLD)
                            .fg(THEME.text_primary()),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("H: ", style().fg(THEME.text_normal())),
                    Span::styled(
                        if loaded {
                            format!("{}{}", currency, high_fmt)
                        } else {
                            "".to_string()
                        },
                        style().fg(THEME.text_secondary()),
                    ),
                    Span::styled(
                        if loaded {
                            high_pct.map_or("".to_string(), |p| {
                                format!("{:>9}", format!("{:.2}%", p * 100.0))
                            })
                        } else {
                            "".to_string()
                        },
                        style().add_modifier(Modifier::BOLD).fg(
                            if high_pct.unwrap_or(0.0) >= 0.0 {
                                THEME.profit()
                            } else {
                                THEME.loss()
                            },
                        ),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("L: ", style().fg(THEME.text_normal())),
                    Span::styled(
                        if loaded {
                            format!("{}{}", currency, low_fmt)
                        } else {
                            "".to_string()
                        },
                        style().fg(THEME.text_secondary()),
                    ),
                    Span::styled(
                        if loaded {
                            low_pct.map_or("".to_string(), |p| {
                                format!("{:>9}", format!("{:.2}%", p * 100.0))
                            })
                        } else {
                            "".to_string()
                        },
                        style()
                            .add_modifier(Modifier::BOLD)
                            .fg(if low_pct.unwrap_or(0.0) >= 0.0 {
                                THEME.profit()
                            } else {
                                THEME.loss()
                            }),
                    ),
                ]),
                Line::default(),
                Line::from(vec![
                    Span::styled("Volume: ", style().fg(THEME.text_normal())),
                    Span::styled(
                        if loaded { vol } else { "".to_string() },
                        style().fg(THEME.text_secondary()),
                    ),
                ]),
            ];

            let pct = vec![Span::styled(
                if loaded {
                    format!("{:>9}", format!("{:.2}%", pct_change * 100.0))
                } else {
                    "".to_string()
                },
                style()
                    .add_modifier(Modifier::BOLD)
                    .fg(if pct_change >= 0.0 {
                        THEME.profit()
                    } else {
                        THEME.loss()
                    }),
            )];

            Paragraph::new(prices)
                .style(style())
                .alignment(Alignment::Left)
                .render(layout[0], buf);

            Paragraph::new(Line::from(pct))
                .style(style())
                .alignment(Alignment::Right)
                .render(layout[0], buf);
        }

        // graph_chunks[0] = prices
        // graph_chunks[1] = volume
        let graph_chunks: Vec<Rect> = if show_volumes {
            Layout::default()
                .constraints([Constraint::Min(0), Constraint::Percentage(25)].as_ref())
                .split(layout[1])
                .to_vec()
        } else {
            Layout::default()
                .constraints([Constraint::Min(0)].as_ref())
                .split(layout[1])
                .to_vec()
        };

        // Draw prices line chart
        match chart_type {
            ChartType::Line => {
                PricesLineChart {
                    data: &data,
                    x_axis: &x_axis,
                    enable_pre_post,
                    is_profit: pct_change >= 0.0,
                    is_summary: true,
                    loaded,
                    show_x_labels: false,
                    show_y_labels: false,
                }
                .render(graph_chunks[0], buf, state);
            }
            ChartType::Candlestick => {
                PricesCandlestickChart {
                    data: &data,
                    x_axis: &x_axis,
                    loaded,
                    show_x_labels: false,
                    show_y_labels: false,
                    is_summary: true,
                }
                .render(graph_chunks[0], buf, state);
            }
            ChartType::Kagi => {
                PricesKagiChart {
                    data: &data,
                    loaded,
                    show_x_labels: false,
                    show_y_labels: false,
                    is_summary: true,
                    kagi_options: state.chart_configuration.kagi_options.clone(),
                }
                .render(graph_chunks[0], buf, state);
            }
        }

        // Show why the chart isn't loading instead of only a spinner
        if !loaded {
            if let Some(error) = state.error() {
                let error_area = add_padding(graph_chunks[0], 10, PaddingDirection::Left);

                stock::render_error(error_area, buf, error);
            }
        }

        // Draw volumes bar chart
        if show_volumes {
            VolumeBarChart {
                data: &data,
                x_axis: &x_axis,
            }
            .render(graph_chunks[1], buf, state);
        }
    }
}
