use chrono::{Local, TimeZone};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState, Paragraph, StatefulWidget, Widget};

use super::block;
use crate::alert::{parse_alert, Alerts};
use crate::common::format_price;
use crate::draw::{add_padding, PaddingDirection};
use crate::theme::style;
use crate::THEME;

pub const ALERTS_WIDTH: u16 = 50;
pub const ALERTS_HEIGHT: u16 = 16;

#[derive(Debug, Default)]
pub struct AlertsState {
    selected: usize,
    /// The alert being entered, if any
    input: Option<String>,
    error: Option<String>,
}

impl AlertsState {
    pub fn is_entering(&self) -> bool {
        self.input.is_some()
    }

    /// Starts entering a new alert, prefilled with `symbol`
    pub fn new_alert(&mut self, symbol: Option<&str>) {
        self.input = Some(symbol.map(|s| format!("{} ", s)).unwrap_or_default());
        self.error = None;
    }

    pub fn add_char(&mut self, c: char) {
        if let Some(input) = self.input.as_mut() {
            input.push(c);
            self.error = None;
        }
    }

    pub fn del_char(&mut self) {
        if let Some(input) = self.input.as_mut() {
            input.pop();
            self.error = None;
        }
    }

    pub fn cancel(&mut self) {
        self.input = None;
        self.error = None;
    }

    /// Adds the entered alert, `current_price` being the latest price of a
    /// symbol, if known. Invalid input is kept to be fixed
    pub fn enter(&mut self, alerts: &mut Alerts, current_price: impl Fn(&str) -> Option<f64>) {
        let Some(input) = self.input.as_deref() else {
            return;
        };

        match parse_alert(input, None) {
            Ok((symbol, price)) => {
                let current = current_price(&symbol);
                alerts.add(symbol, price, current);

                self.selected = alerts.len() - 1;
                self.cancel();
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub fn delete(&mut self, alerts: &mut Alerts) {
        alerts.remove(self.selected);
        self.selected = self.selected.min(alerts.len().saturating_sub(1));
    }

    pub fn previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn next(&mut self, alerts: &Alerts) {
        self.selected = (self.selected + 1).min(alerts.len().saturating_sub(1));
    }
}

pub struct AlertsWidget<'a> {
    pub alerts: &'a Alerts,
}

impl AlertsWidget<'_> {
    /// The pane, centered in `area`
    pub fn get_rect(area: Rect) -> Rect {
        let width = ALERTS_WIDTH.min(area.width);
        let height = ALERTS_HEIGHT.min(area.height);

        Rect {
            x: area.x + (area.width - width) / 2,
            y: area.y + (area.height - height) / 2,
            width,
            height,
        }
    }
}

impl StatefulWidget for AlertsWidget<'_> {
    type State = AlertsState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        Clear.render(area, buf);
        block::new(" Price Alerts ").style(style()).render(area, buf);

        let mut area = add_padding(area, 1, PaddingDirection::All);
        area = add_padding(area, 1, PaddingDirection::Left);
        area = add_padding(area, 1, PaddingDirection::Right);

        // layout[0] - Alerts
        // layout[1] - Input / error
        // layout[2] - Key hints
        let layout = Layout::default()
            .constraints([
                Constraint::Min(0),
                Constraint::Length(2),
                Constraint::Length(1),
            ])
            .split(area);

        if self.alerts.len() == 0 {
            Paragraph::new(Span::styled(
                "No alerts, press n to add one",
                style().fg(THEME.gray()),
            ))
            .render(layout[0], buf);
        } else {
            let symbol_width = self
                .alerts
                .iter()
                .map(|a| a.symbol.chars().count())
                .max()
                .unwrap_or(0);

            let items: Vec<_> = self
                .alerts
                .iter()
                .map(|alert| {
                    let condition = match alert.direction {
                        Some(direction) => {
                            format!("{} {}", direction.as_str(), format_price(alert.price))
                        }
                        None => format!("@ {}", format_price(alert.price)),
                    };

                    let status = match alert.triggered_at {
                        Some(time) => Span::styled(
                            Local
                                .timestamp_opt(time, 0)
                                .single()
                                .map(|t| format!("triggered {}", t.format("%b %d %H:%M")))
                                .unwrap_or_else(|| "triggered".into()),
                            style().fg(THEME.gray()),
                        ),
                        None => Span::styled("waiting", style().fg(THEME.text_normal())),
                    };

                    let fg = if alert.is_pending() {
                        THEME.text_secondary()
                    } else {
                        THEME.gray()
                    };

                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("{:<symbol_width$}  ", alert.symbol),
                            style().fg(fg).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(format!("{:<14}", condition), style().fg(fg)),
                        status,
                    ]))
                })
                .collect();

            let mut list_state = ListState::default();
            list_state.select(Some(state.selected));

            // The focused highlight is drawn reversed so the text takes the background
            // color, the item colors don't contrast with a light highlight
            let highlight_style = if state.is_entering() {
                style().bg(THEME.highlight_unfocused())
            } else {
                style()
                    .fg(THEME.highlight_focused())
                    .add_modifier(Modifier::REVERSED)
            };

            StatefulWidget::render(
                List::new(items).highlight_style(highlight_style),
                layout[0],
                buf,
                &mut list_state,
            );
        }

        let mut input_lines = vec![];

        if let Some(input) = &state.input {
            input_lines.push(Line::from(vec![
                Span::styled("> ", style().fg(THEME.text_normal())),
                Span::styled(
                    input.as_str(),
                    style()
                        .add_modifier(Modifier::BOLD)
                        .fg(THEME.text_secondary()),
                ),
            ]));
        }

        if let Some(error) = &state.error {
            input_lines.push(Line::from(Span::styled(
                error.as_str(),
                style().add_modifier(Modifier::BOLD).fg(THEME.loss()),
            )));
        }

        Paragraph::new(input_lines).render(layout[1], buf);

        let hints = if state.is_entering() {
            "SYMBOL PRICE, <Enter>: add, <Esc>: cancel"
        } else {
            "n: new, d: delete, <Esc>: close"
        };

        Paragraph::new(Span::styled(hints, style().fg(THEME.gray()))).render(layout[2], buf);
    }
}
