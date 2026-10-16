use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::symbols::bar;
use ratatui::widgets::{BarChart, Block, Borders, StatefulWidget, Widget};

use super::x_axis::{plot_columns, render_session_bounds, XAxis};
use crate::common::Price;
use crate::theme::style;
use crate::widget::StockState;
use crate::THEME;

pub struct VolumeBarChart<'a> {
    pub data: &'a [Price],
    pub x_axis: &'a XAxis,
}

impl StatefulWidget for VolumeBarChart<'_> {
    type State = StockState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let mut volume_chunks = area;
        volume_chunks.height += 1;

        // Same plot area as the price chart above, so bars line up with prices
        let (plot_x, plot_width) = plot_columns(volume_chunks);

        if self.data.is_empty() || plot_width == 0 {
            return;
        }

        // One bar per column, at the position of its prices on the x axis
        let volumes = self.data.iter().map(|p| p.volume).collect::<Vec<_>>();
        let bars = self
            .x_axis
            .resample_columns(&volumes, plot_width)
            .into_iter()
            .map(|volume| ("", volume))
            .collect::<Vec<_>>();

        // y axis line
        Block::default()
            .borders(Borders::LEFT)
            .border_style(style().fg(THEME.border_axis()))
            .render(
                Rect {
                    x: plot_x - 1,
                    width: 1,
                    ..volume_chunks
                },
                buf,
            );

        let plot = Rect {
            x: plot_x,
            width: plot_width,
            ..volume_chunks
        };

        BarChart::default()
            .bar_width(1)
            .bar_gap(0)
            .bar_set(bar::NINE_LEVELS)
            .style(style().fg(THEME.gray()))
            .data(&bars)
            .render(plot, buf);

        render_session_bounds(buf, plot, self.x_axis, state);
    }
}
