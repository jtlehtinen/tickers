use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders};

use crate::theme::style;
use crate::THEME;

pub fn new(title: &'_ str) -> Block<'_> {
    with_title(Line::from(Span::styled(
        title,
        style().fg(THEME.text_normal()),
    )))
}

/// A block whose title is already styled
pub fn with_title(title: Line<'_>) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(style().fg(THEME.border_primary()))
        .title(title)
}
