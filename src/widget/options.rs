use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use chrono::DateTime;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, List, ListItem, ListState, Paragraph, Row, StatefulWidget, Table,
    TableState, Widget,
};

use super::{block, CachableWidget, CacheState};
use crate::api::model::{OptionsContract, OptionsData, OptionsQuote};
use crate::common::format_money;
use crate::draw::{add_padding, PaddingDirection};
use crate::service::{self, Service};
use crate::theme::style;
use crate::THEME;

#[derive(Clone, Copy, PartialEq, Hash)]
enum OptionType {
    Call,
    Put,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum SelectionMode {
    Dates,
    Options,
}

pub struct OptionsState {
    options_service: service::options::OptionsService,
    exp_dates: Vec<i64>,
    exp_date: Option<i64>,
    data: HashMap<i64, OptionsData>,
    selected_type: OptionType,
    pub selection_mode: SelectionMode,
    selected_option: Option<usize>,
    quote: Option<OptionsQuote>,
    cache_state: CacheState,
}

impl Hash for OptionsState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.exp_dates.hash(state);
        self.exp_date.hash(state);
        self.data().hash(state);
        self.selected_type.hash(state);
        self.selection_mode.hash(state);
        self.selected_option.hash(state);
        self.quote.hash(state);
    }
}

impl OptionsState {
    pub fn new(symbol: String) -> OptionsState {
        let options_service = service::options::OptionsService::new(symbol);

        OptionsState {
            options_service,
            exp_dates: vec![],
            exp_date: None,
            data: HashMap::new(),
            selected_type: OptionType::Call,
            selection_mode: SelectionMode::Dates,
            selected_option: None,
            quote: None,
            cache_state: Default::default(),
        }
    }

    pub fn data(&self) -> Option<&OptionsData> {
        if let Some(date) = self.exp_date {
            self.data.get(&date)
        } else {
            None
        }
    }

    fn set_exp_date(&mut self, date: i64) {
        self.exp_date = Some(date);

        self.options_service.set_expiration_date(date);

        self.selected_option.take();

        if self.data().is_some() {
            self.set_selected_as_closest();
        }
    }

    pub fn toggle_option_type(&mut self) {
        match self.selected_type {
            OptionType::Call => self.selected_type = OptionType::Put,
            OptionType::Put => self.selected_type = OptionType::Call,
        }

        if self.data().is_some() {
            self.set_selected_as_closest();
        }
    }

    /// Calls or puts (depending on the selected type) for the selected date
    fn selected_range(&self) -> &[OptionsContract] {
        match (self.data(), self.selected_type) {
            (Some(data), OptionType::Call) => &data.calls,
            (Some(data), OptionType::Put) => &data.puts,
            (None, _) => &[],
        }
    }

    fn set_selected_as_closest(&mut self) {
        let selected_range = self.selected_range();

        if selected_range.is_empty() {
            self.selected_option = None;
            return;
        }

        let market_price = if let Some(ref quote) = self.quote {
            quote.regular_market_price
        } else {
            0.0
        };

        let mut closest_idx = selected_range
            .iter()
            .position(|c| c.strike < market_price)
            .unwrap_or_default();

        if closest_idx > 0 && self.selected_type == OptionType::Call {
            closest_idx -= 1;
        }

        self.selected_option = Some(closest_idx);
    }

    pub fn previous_date(&mut self) {
        if let Some(idx) = self
            .exp_dates
            .iter()
            .position(|d| *d == self.exp_date.unwrap_or_default())
        {
            let new_idx = if idx == 0 {
                self.exp_dates.len() - 1
            } else {
                idx - 1
            };

            self.set_exp_date(self.exp_dates[new_idx]);
        }
    }

    pub fn next_date(&mut self) {
        if let Some(idx) = self
            .exp_dates
            .iter()
            .position(|d| *d == self.exp_date.unwrap_or_default())
        {
            let new_idx = (idx + 1) % self.exp_dates.len();

            self.set_exp_date(self.exp_dates[new_idx]);
        }
    }

    pub fn previous_option(&mut self) {
        let len = self.selected_range().len();

        if let (Some(idx), true) = (self.selected_option, len > 0) {
            let new_idx = if idx == 0 { len - 1 } else { idx - 1 };

            self.selected_option = Some(new_idx);
        }
    }

    pub fn next_option(&mut self) {
        let len = self.selected_range().len();

        if let (Some(idx), true) = (self.selected_option, len > 0) {
            self.selected_option = Some((idx + 1) % len);
        }
    }

    pub fn selection_mode_left(&mut self) {
        if self.selection_mode == SelectionMode::Options {
            self.selection_mode = SelectionMode::Dates;
        }
    }

    pub fn selection_mode_right(&mut self) {
        if self.selection_mode == SelectionMode::Dates {
            self.selection_mode = SelectionMode::Options;
        }
    }

    pub fn update(&mut self) {
        let updates = self.options_service.updates();

        for update in updates {
            match update {
                service::options::Update::ExpirationDates(dates) => {
                    let prev_len = self.exp_dates.len();

                    self.exp_dates = dates;

                    if prev_len == 0 && !self.exp_dates.is_empty() {
                        self.set_exp_date(self.exp_dates[0]);
                    }
                }
                service::options::Update::OptionsData(mut header) => {
                    if header.options.len() == 1 {
                        header.options[0].calls.reverse();
                        header.options[0].puts.reverse();

                        self.quote = Some(header.quote);

                        let Some(exp_date) = self.exp_date else {
                            continue;
                        };

                        self.data.insert(exp_date, header.options.remove(0));

                        if self.selected_option.is_none() {
                            self.set_selected_as_closest();
                        }
                    }
                }
            }
        }
    }
}

pub struct OptionsWidget {}

impl StatefulWidget for OptionsWidget {
    type State = OptionsState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        self.render_cached(area, buf, state);
    }
}

impl CachableWidget<OptionsState> for OptionsWidget {
    fn cache_state_mut(state: &mut OptionsState) -> &mut CacheState {
        &mut state.cache_state
    }

    fn render(self, mut area: Rect, buf: &mut Buffer, state: &mut OptionsState) {
        block::new(" Options ").render(area, buf);
        area = add_padding(area, 1, PaddingDirection::All);

        // chunks[0] - call / put selector
        // chunks[1] - option info
        // chunks[2] - remainder (date selector | option selector)
        let mut chunks: Vec<Rect> = Layout::default()
            .constraints(
                [
                    Constraint::Length(2),
                    Constraint::Length(8),
                    Constraint::Min(0),
                ]
                .as_ref(),
            )
            .split(area)
            .to_vec();

        // Draw call / put selector
        {
            let call_put_selector = vec![
                Span::styled(
                    "Call",
                    style().fg(THEME.profit()).add_modifier(
                        if state.selected_type == OptionType::Call {
                            Modifier::BOLD | Modifier::UNDERLINED
                        } else {
                            Modifier::empty()
                        },
                    ),
                ),
                Span::styled(" | ", style()),
                Span::styled(
                    "Put",
                    style().fg(THEME.loss()).add_modifier(
                        if state.selected_type == OptionType::Put {
                            Modifier::BOLD | Modifier::UNDERLINED
                        } else {
                            Modifier::empty()
                        },
                    ),
                ),
            ];

            chunks[0] = add_padding(chunks[0], 1, PaddingDirection::Left);
            chunks[0] = add_padding(chunks[0], 1, PaddingDirection::Right);

            Block::default()
                .style(style().fg(THEME.border_secondary()))
                .borders(Borders::BOTTOM)
                .render(chunks[0], buf);

            chunks[0] = add_padding(chunks[0], 1, PaddingDirection::Bottom);

            Paragraph::new(Line::from(call_put_selector))
                .style(style().fg(THEME.text_normal()))
                .alignment(Alignment::Center)
                .render(chunks[0], buf);
        }

        // selector_chunks[0] - date selector
        // selector_chunks[1] - option selector
        let mut selector_chunks: Vec<Rect> = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(12), Constraint::Min(0)].as_ref())
            .split(chunks[2])
            .to_vec();

        // Draw date selector
        {
            selector_chunks[0] = add_padding(selector_chunks[0], 1, PaddingDirection::Left);

            Block::default()
                .style(style().fg(THEME.border_secondary()))
                .borders(Borders::RIGHT)
                .render(selector_chunks[0], buf);
            selector_chunks[0] = add_padding(selector_chunks[0], 2, PaddingDirection::Right);

            let dates = state
                .exp_dates
                .iter()
                .map(|d| {
                    let date = DateTime::from_timestamp(*d, 0)
                        .map(|date| date.format("%b-%d-%y").to_string())
                        .unwrap_or_default();
                    ListItem::new(Span::styled(date, style()))
                })
                .collect::<Vec<_>>();

            let list = List::new(dates)
                .style(style().fg(THEME.text_normal()))
                .highlight_style(style().bg(if state.selection_mode == SelectionMode::Dates {
                    THEME.highlight_focused()
                } else {
                    THEME.highlight_unfocused()
                }));

            let mut list_state = ListState::default();
            if let Some(idx) = state
                .exp_dates
                .iter()
                .position(|d| *d == state.exp_date.unwrap_or_default())
            {
                list_state.select(Some(idx));
            }

            Paragraph::new(Span::styled("Date", style().fg(THEME.text_secondary())))
                .render(selector_chunks[0], buf);

            selector_chunks[0] = add_padding(selector_chunks[0], 2, PaddingDirection::Top);

            <List as StatefulWidget>::render(list, selector_chunks[0], buf, &mut list_state);
        }

        // Draw options data
        {
            selector_chunks[1] = add_padding(selector_chunks[1], 1, PaddingDirection::Left);

            if let Some(data) = state.data() {
                let selected_data = if state.selected_type == OptionType::Call {
                    &data.calls[..]
                } else {
                    &data.puts[..]
                };

                let rows = selected_data
                    .iter()
                    .map(|d| {
                        Row::new(vec![
                            Cell::from(format!("{: <7.2}", d.strike)),
                            Cell::from(format!("{: <7.2}", d.last_price)),
                            Cell::from(format!("{: >7.2}%", d.percent_change)),
                        ])
                        .style(style().fg(if d.percent_change >= 0.0 {
                            THEME.profit()
                        } else {
                            THEME.loss()
                        }))
                    })
                    .collect::<Vec<_>>();

                let header = Row::new(vec!["Strike", "Price", "% Change"])
                    .style(style().fg(THEME.text_secondary()))
                    .bottom_margin(1);
                let table = Table::new(
                    rows,
                    [
                        Constraint::Length(8),
                        Constraint::Length(8),
                        Constraint::Min(0),
                    ],
                )
                .header(header)
                .style(style().fg(THEME.text_normal()))
                .row_highlight_style(
                    style()
                        .bg(if state.selection_mode == SelectionMode::Options {
                            THEME.highlight_focused()
                        } else {
                            THEME.highlight_unfocused()
                        })
                        .fg(THEME.text_normal()),
                )
                .column_spacing(2);

                let mut table_state = TableState::default();
                if let Some(idx) = state.selected_option {
                    table_state.select(Some(idx));
                }

                selector_chunks[1] = add_padding(selector_chunks[1], 1, PaddingDirection::Right);

                <Table as StatefulWidget>::render(table, selector_chunks[1], buf, &mut table_state);
            }
        }

        // Draw selected option info
        {
            chunks[1] = add_padding(chunks[1], 1, PaddingDirection::Left);
            chunks[1] = add_padding(chunks[1], 1, PaddingDirection::Right);

            Block::default()
                .style(style().fg(THEME.border_secondary()))
                .borders(Borders::BOTTOM)
                .render(chunks[1], buf);

            chunks[1] = add_padding(chunks[1], 1, PaddingDirection::Bottom);

            if let Some(idx) = state.selected_option {
                if let Some(option) = state.selected_range().get(idx) {
                    let mut columns: Vec<Rect> = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Length(20), Constraint::Length(20)].as_ref())
                        .split(chunks[1])
                        .to_vec();

                    columns[1] = add_padding(columns[1], 2, PaddingDirection::Left);

                    let currency = option.currency.as_deref().unwrap_or("USD");

                    let column_0 = vec![
                        Line::from(Span::styled(
                            right_align(
                                "Strike:",
                                format_money(currency, &format!("{:.2}", option.strike), 0),
                                19,
                            ),
                            style(),
                        )),
                        Line::default(),
                        Line::from(Span::styled(
                            right_align("Price:", format!("{:.2}", option.last_price), 15),
                            style(),
                        )),
                        Line::default(),
                        Line::from(Span::styled(
                            right_align(
                                "Bid:",
                                format!("{:.2}", option.bid.unwrap_or_default()),
                                15,
                            ),
                            style(),
                        )),
                        Line::default(),
                        Line::from(Span::styled(
                            right_align(
                                "Ask:",
                                format!("{:.2}", option.ask.unwrap_or_default()),
                                15,
                            ),
                            style(),
                        )),
                    ];

                    let column_1 = vec![
                        Line::from(Span::styled(
                            right_align(
                                "Volume:",
                                option.volume.unwrap_or_default().to_string(),
                                18,
                            ),
                            style(),
                        )),
                        Line::default(),
                        Line::from(Span::styled(
                            right_align(
                                "Open Int:",
                                option.open_interest.unwrap_or_default().to_string(),
                                18,
                            ),
                            style(),
                        )),
                        Line::default(),
                        Line::from(Span::styled(
                            right_align(
                                "Implied Vol:",
                                format!(
                                    "{:.0}%",
                                    option.implied_volatility.unwrap_or_default() * 100.0
                                ),
                                18,
                            ),
                            style(),
                        )),
                    ];

                    Paragraph::new(column_0)
                        .style(style().fg(THEME.text_normal()))
                        .render(columns[0], buf);
                    Paragraph::new(column_1)
                        .style(style().fg(THEME.text_normal()))
                        .render(columns[1], buf);
                }
            }
        }
    }
}

/// Formats `label` followed by `value` right aligned so the line is `width`
/// characters wide. Values that don't fit are appended without padding.
fn right_align(label: &str, value: String, width: usize) -> String {
    format!(
        "{label}{value:>value_width$}",
        value_width = width.saturating_sub(label.len())
    )
}

#[cfg(test)]
mod tests {
    use super::right_align;

    #[test]
    fn right_align_pads_to_width() {
        assert_eq!(right_align("Price:", "1.25".into(), 15), "Price:     1.25");
        assert_eq!(
            right_align("Strike:", "100.00 USD".into(), 19),
            "Strike:  100.00 USD"
        );
    }

    #[test]
    fn right_align_does_not_panic_on_wide_values() {
        // Previously computed as `15 - (len + 6)` which underflowed
        assert_eq!(
            right_align("Price:", "1234567.89".into(), 15),
            "Price:1234567.89"
        );
        assert_eq!(
            right_align("Implied Vol:", "1234567%".into(), 18),
            "Implied Vol:1234567%"
        );
    }
}
