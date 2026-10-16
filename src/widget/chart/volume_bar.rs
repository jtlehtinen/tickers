use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::symbols::bar;
use ratatui::widgets::{BarChart, Block, Borders, StatefulWidget, Widget};
use unicode_width::UnicodeWidthStr;

use super::x_axis::{render_session_bounds, render_ticks, ChartAreas, XAxis};
use crate::common::Price;
use crate::task::MarketState;
use crate::theme::style;
use crate::widget::StockState;
use crate::THEME;

pub struct VolumeBarChart<'a> {
    pub data: &'a [Price],
    pub x_axis: &'a XAxis,
    /// Time labels go under the volumes, when they're shown below prices
    pub show_x_labels: bool,
}

impl StatefulWidget for VolumeBarChart<'_> {
    type State = StockState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        // Same plot area as the price chart above, so bars line up with prices
        let areas = ChartAreas::new(area, self.show_x_labels);
        let plot = areas.plot;

        if self.data.is_empty() || plot.width == 0 {
            return;
        }

        // One bar per column, at the position of its prices on the x axis
        let volumes = self.data.iter().map(|p| p.volume).collect::<Vec<_>>();
        let bars = self
            .x_axis
            .resample_columns(&volumes, plot.width)
            .into_iter()
            .map(|volume| ("", volume))
            .collect::<Vec<_>>();

        // The bar chart keeps its last row for labels, which goes on the row below
        // the plot (the x axis line, drawn over it, or the border below the chart)
        BarChart::default()
            .bar_width(1)
            .bar_gap(0)
            .bar_set(bar::NINE_LEVELS)
            .style(style().fg(THEME.gray()))
            .data(&bars)
            .render(
                Rect {
                    height: plot.height + 1,
                    ..plot
                },
                buf,
            );

        // Axis lines, the y axis line reaching the border below without x labels
        Block::default()
            .borders(if self.show_x_labels {
                Borders::LEFT | Borders::BOTTOM
            } else {
                Borders::LEFT
            })
            .border_style(style().fg(THEME.border_axis()))
            .render(
                Rect {
                    height: areas.chart.height + u16::from(!self.show_x_labels),
                    ..areas.chart
                },
                buf,
            );

        if bars.iter().all(|(_, volume)| *volume == 0) {
            render_no_volume(buf, plot, state.market_state);
        }

        render_session_bounds(buf, plot, self.x_axis, state);

        if let Some(x_labels) = areas.x_labels {
            let ticks = self.x_axis.ticks(plot.width);
            render_ticks(buf, plot, x_labels, &ticks);
        }
    }
}

/// Explains why there are no volumes, centered in `plot`. Yahoo has no
/// pre-market volumes, so the volumes of a day start at the open.
fn render_no_volume(buf: &mut Buffer, plot: Rect, market_state: Option<MarketState>) {
    let message = if market_state == Some(MarketState::Pre) {
        "Volume chart starts at the open"
    } else {
        "No volume data"
    };

    let width = message.width() as u16;

    if plot.height == 0 || width > plot.width {
        return;
    }

    buf.set_string(
        plot.x + (plot.width - width) / 2,
        plot.y + (plot.height - 1) / 2,
        message,
        style().fg(THEME.gray()),
    );
}
