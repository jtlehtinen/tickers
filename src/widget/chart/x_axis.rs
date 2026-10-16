//! The x axis shared by the price, volume and kagi charts.
//!
//! All charts plot into a plot area of the same horizontal extent (right of
//! the y labels) and map positions to columns exactly like ratatui's braille
//! canvas does. Positions are expressed as a fraction of the plot width, so the
//! price line, candles, volume bars and time labels all line up.

use chrono::{Datelike, FixedOffset, Local, NaiveDateTime, TimeZone, Timelike};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

use crate::common::{Price, TimeFrame};
use crate::theme::style;
use crate::widget::StockState;
use crate::THEME;

/// Width of the gutter left of the plot area holding the y labels, including
/// the column of the y axis line
const Y_LABEL_WIDTH: u16 = 9;

/// Never show more labels than this, even if they'd fit
const MAX_TICKS: usize = 16;

/// Minimum number of blank columns between two labels
const MIN_LABEL_GAP: u16 = 2;

/// Minutes between ticks on a 1D chart, finest first
const SESSION_STEPS: [u32; 7] = [15, 30, 60, 120, 180, 240, 360];

// Calendar boundaries ticks are placed on for other time frames, finest first
const DAY: usize = 0;
const WEEK: usize = 1;
const MONTH: usize = 2;
const QUARTER: usize = 3;
const YEAR: usize = 4;
const CALENDAR_FORMATS: [&str; 5] = ["%b %d", "%b %d", "%b '%y", "%b '%y", "%Y"];

/// Every n'th trend gets a tick on a kagi chart, finest first
const TREND_STRIDES: [usize; 10] = [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000];

/// Horizontal extent `(x, width)` of the plot area within `area`, which starts
/// with the y label gutter
pub fn plot_columns(area: Rect) -> (u16, u16) {
    (
        area.x.saturating_add(Y_LABEL_WIDTH),
        area.width.saturating_sub(Y_LABEL_WIDTH),
    )
}

/// Areas of a price chart
pub struct ChartAreas {
    /// Gutter for the y labels
    pub y_labels: Rect,
    /// The chart including its axis lines (left border, and bottom border if
    /// there are x labels)
    pub chart: Rect,
    /// Where data is plotted, inside the axis lines
    pub plot: Rect,
    /// Row for the x labels, which extends into the y label gutter so labels
    /// near the y axis can be centered on their tick
    pub x_labels: Option<Rect>,
}

impl ChartAreas {
    pub fn new(area: Rect, show_x_labels: bool) -> ChartAreas {
        let show_x_labels = show_x_labels && area.height > 1;

        let mut rest = area;
        let x_labels = show_x_labels.then(|| {
            rest.height -= 1;
            Rect {
                y: rest.bottom(),
                height: 1,
                ..area
            }
        });

        let (plot_x, plot_width) = plot_columns(rest);

        // The y axis line is the column left of the plot area
        let chart = Rect {
            x: plot_x.saturating_sub(1),
            width: plot_width + 1,
            ..rest
        };

        let axis_rows = u16::from(show_x_labels);

        let plot = Rect {
            x: plot_x,
            width: plot_width,
            height: chart.height.saturating_sub(axis_rows),
            ..chart
        };

        let y_labels = Rect {
            width: Y_LABEL_WIDTH.min(rest.width),
            height: plot.height,
            ..rest
        };

        ChartAreas {
            y_labels,
            chart,
            plot,
            x_labels,
        }
    }
}

/// Column within a plot area `width` columns wide that `fraction` (0 = left
/// edge, 1 = right edge) is drawn at. Matches the mapping of ratatui's braille
/// canvas, which the charts are drawn with.
pub fn column(fraction: f64, width: u16) -> Option<u16> {
    if width == 0 || !(0.0..=1.0).contains(&fraction) {
        return None;
    }

    let dot = (fraction * (f64::from(width) * 2.0 - 1.0)) as u16;

    Some((dot / 2).min(width - 1))
}

/// Range of fractions drawn in `column`, the inverse of [`column`]
fn column_fractions(column: u16, width: u16) -> (f64, f64) {
    let dots = f64::from(width) * 2.0 - 1.0;

    let start = f64::from(column) * 2.0 / dots;
    let end = ((f64::from(column) + 1.0) * 2.0 / dots).min(1.0);

    (start, end)
}

/// The x axis of a stock's price and volume charts.
///
/// The axis is divided into slots, each price taking up one slot:
/// - 1D: a slot per minute of the trading session, so prices are placed by
///   their time and the rest of the session is visible as empty space
/// - other time frames: a slot per price, skipping time the market is closed
pub struct XAxis {
    /// Start of the session on a 1D axis
    session_start: Option<i64>,
    num_slots: f64,
    /// Slot of each price
    slots: Vec<f64>,
    ticks: Ticks,
}

impl XAxis {
    pub fn new(state: &StockState, data: &[Price]) -> XAxis {
        if state.time_frame == TimeFrame::Day1 {
            let (start, end) = state.current_trading_start_end();

            Self::session(data, start, end, &Local)
        } else {
            let utc_offset = state
                .chart_meta
                .as_ref()
                .and_then(|m| m.gmtoffset)
                .and_then(|offset| FixedOffset::east_opt(offset as i32))
                .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());

            Self::trading_days(data, &utc_offset)
        }
    }

    /// Time based axis spanning the session from `start` to `end`, with ticks
    /// at round times in `tz`
    fn session<Tz: TimeZone>(data: &[Price], start: i64, end: i64, tz: &Tz) -> XAxis {
        let num_slots = ((end - start) as f64 / 60.0).max(1.0);

        let slots = data
            .iter()
            .map(|p| (p.date - start) as f64 / 60.0)
            .collect();

        // Local offsets are multiples of 15 minutes, so stepping through UTC
        // quarter hours visits every local quarter hour
        let quarter_hour = 15 * 60;
        let first = start + (quarter_hour - start.rem_euclid(quarter_hour)) % quarter_hour;

        let candidates = (first..=end)
            .step_by(quarter_hour as usize)
            .filter_map(|time| {
                let local = tz.timestamp_opt(time, 0).single()?.naive_local();
                let minute_of_day = local.hour() * 60 + local.minute();

                let levels = level_mask(
                    SESSION_STEPS
                        .iter()
                        .map(|step| minute_of_day.is_multiple_of(*step)),
                );

                Some(Tick {
                    fraction: (time - start) as f64 / 60.0 / num_slots,
                    levels,
                    time: local,
                })
            })
            .collect();

        XAxis {
            session_start: Some(start),
            num_slots,
            slots,
            ticks: Ticks {
                candidates,
                formats: vec!["%H:%M"; SESSION_STEPS.len()],
            },
        }
    }

    /// Axis with a slot per price, with ticks at the first price of each day,
    /// week, month, quarter or year in `tz` (the exchange's timezone, so dates
    /// match trading days)
    fn trading_days<Tz: TimeZone>(data: &[Price], tz: &Tz) -> XAxis {
        let num_slots = data.len().max(1) as f64;

        let slots = (0..data.len()).map(|idx| idx as f64).collect();

        let times = data
            .iter()
            .map(|p| Some(tz.timestamp_opt(p.date, 0).single()?.naive_local()))
            .collect::<Vec<_>>();

        let candidates = times
            .iter()
            .enumerate()
            .filter_map(|(idx, time)| {
                let time = (*time)?;

                let levels = match idx.checked_sub(1).and_then(|prev| times[prev]) {
                    // The first price starts a day (intraday data starts at a session
                    // open), but we can't know if it starts a week, month, ...
                    None => 1 << DAY,
                    Some(prev) => {
                        let (date, prev) = (time.date(), prev.date());
                        let quarter = |date: chrono::NaiveDate| (date.month0() / 3, date.year());

                        let is_new = |level: usize, changed: bool| u16::from(changed) << level;

                        is_new(DAY, date != prev)
                            | is_new(WEEK, date.iso_week() != prev.iso_week())
                            | is_new(
                                MONTH,
                                (date.month(), date.year()) != (prev.month(), prev.year()),
                            )
                            | is_new(QUARTER, quarter(date) != quarter(prev))
                            | is_new(YEAR, date.year() != prev.year())
                    }
                };

                (levels != 0).then(|| Tick {
                    fraction: idx as f64 / num_slots,
                    levels,
                    time,
                })
            })
            .collect();

        XAxis {
            session_start: None,
            num_slots,
            slots,
            ticks: Ticks {
                candidates,
                formats: CALENDAR_FORMATS.to_vec(),
            },
        }
    }

    /// Bounds to plot `x` values in
    pub fn bounds(&self) -> [f64; 2] {
        [0.0, self.num_slots]
    }

    pub fn num_slots(&self) -> f64 {
        self.num_slots
    }

    /// Slot of the price at `idx`
    pub fn slot(&self, idx: usize) -> f64 {
        self.slots[idx]
    }

    /// `x` value to plot the price at `idx` at, the middle of its slot
    pub fn x(&self, idx: usize) -> f64 {
        self.slots[idx] + 0.5
    }

    /// Column `time` is drawn at on a 1D axis in a plot area `width` columns
    /// wide, if it's within the session and not on its edges
    pub fn time_column(&self, time: i64, width: u16) -> Option<u16> {
        let fraction = (time - self.session_start?) as f64 / 60.0 / self.num_slots;

        if fraction > 0.0 && fraction < 1.0 {
            column(fraction, width)
        } else {
            None
        }
    }

    /// The labeled ticks to show on a plot area `width` columns wide
    pub fn ticks(&self, width: u16) -> Vec<(u16, String)> {
        self.ticks.select(width)
    }

    /// Averages `values` (one per price) for each column of a plot area `width`
    /// columns wide. Each price covers its slot, prices covering a column only
    /// partially are weighted accordingly, and columns without prices are 0.
    pub fn resample_columns(&self, values: &[u64], width: u16) -> Vec<u64> {
        let mut weighted_sums = vec![0.0; width as usize];
        let mut weights = vec![0.0; width as usize];

        for (idx, value) in values.iter().enumerate().take(self.slots.len()) {
            let (start, end) = (self.slots[idx], self.slots[idx] + 1.0);

            let first = column((start / self.num_slots).max(0.0), width);
            let last = column((end / self.num_slots).min(1.0), width);

            let (Some(first), Some(last)) = (first, last) else {
                continue;
            };

            for col in first..=last {
                let (col_start, col_end) = column_fractions(col, width);
                let overlap = (end.min(col_end * self.num_slots)
                    - start.max(col_start * self.num_slots))
                .max(0.0);

                weighted_sums[col as usize] += *value as f64 * overlap;
                weights[col as usize] += overlap;
            }
        }

        weighted_sums
            .iter()
            .zip(&weights)
            .map(|(sum, weight)| {
                if *weight > 0.0 {
                    (sum / weight).round() as u64
                } else {
                    0
                }
            })
            .collect()
    }
}

/// Ticks for a kagi chart, whose x axis is the trends rather than time.
/// `dates` are the start of each visible trend, which are drawn at
/// `fractions`; `offset` is the index of the first visible trend.
pub fn kagi_ticks(dates: &[i64], fractions: &[f64], offset: usize, time_frame: TimeFrame) -> Ticks {
    let candidates = dates
        .iter()
        .zip(fractions)
        .enumerate()
        .filter_map(|(idx, (date, fraction))| {
            let time = Local.timestamp_opt(*date, 0).single()?.naive_local();

            // Strides are based on the absolute index, so ticks don't move around
            // while scrolling
            let levels = level_mask(
                TREND_STRIDES
                    .iter()
                    .map(|stride| (offset + idx).is_multiple_of(*stride)),
            );

            Some(Tick {
                fraction: *fraction,
                levels,
                time,
            })
        })
        .collect();

    Ticks {
        candidates,
        formats: vec![time_frame.time_format(); TREND_STRIDES.len()],
    }
}

fn level_mask(is_level: impl IntoIterator<Item = bool>) -> u16 {
    is_level
        .into_iter()
        .enumerate()
        .filter(|(_, is_level)| *is_level)
        .fold(0, |mask, (level, _)| mask | 1 << level)
}

struct Tick {
    fraction: f64,
    /// Bit mask of the levels this is a tick at
    levels: u16,
    time: NaiveDateTime,
}

/// Candidate ticks at increasingly coarse levels (e.g. every 15 minutes,
/// every 30 minutes, every hour, ...)
pub struct Ticks {
    candidates: Vec<Tick>,
    /// Label format of each level
    formats: Vec<&'static str>,
}

impl Ticks {
    /// Picks the finest level whose labels fit in a plot area `width` columns
    /// wide, returning the column and label of each tick
    pub fn select(&self, width: u16) -> Vec<(u16, String)> {
        let mut coarsest = vec![];

        for (level, format) in self.formats.iter().enumerate() {
            let ticks = self
                .candidates
                .iter()
                .filter(|tick| tick.levels & 1 << level != 0)
                .filter_map(|tick| {
                    let column = column(tick.fraction, width)?;
                    Some((column, tick.time.format(format).to_string()))
                })
                .collect::<Vec<_>>();

            if ticks.is_empty() {
                continue;
            }

            let kept = drop_colliding(ticks.clone());

            // A level fits if all its labels fit, except possibly the first. That
            // one may start a short period (e.g. a single padding price on the
            // previous day), which shouldn't force a much coarser level. Gaps
            // anywhere else would look like missing data, so use a coarser level
            // instead.
            let fits = kept == ticks || kept == ticks[1..];

            if fits && kept.len() <= MAX_TICKS {
                return kept;
            }

            coarsest = kept;
        }

        coarsest
    }
}

/// Drops ticks whose labels would be too close to the next tick's label. When
/// two collide the later one is kept, as the earlier tick then starts a short
/// (often partial) period.
fn drop_colliding(ticks: Vec<(u16, String)>) -> Vec<(u16, String)> {
    let mut kept: Vec<(u16, String)> = Vec::with_capacity(ticks.len());

    for tick in ticks.into_iter().rev() {
        let fits = kept.last().is_none_or(|next| {
            // Labels are centered on their column
            let needed = (tick.1.width() + next.1.width()).div_ceil(2) as u16 + MIN_LABEL_GAP;

            next.0.saturating_sub(tick.0) >= needed
        });

        if fits {
            kept.push(tick);
        }
    }

    kept.reverse();
    kept
}

/// Draws a tick mark on the x axis line below `plot` for each tick, and the
/// tick's label centered below it in `label_area`. Labels that would overlap
/// the previous label are skipped along with their tick mark.
/// Draws dotted lines at the open and close of the regular session in the
/// empty cells of `plot`, separating it from pre and post market on 1D
pub fn render_session_bounds(buf: &mut Buffer, plot: Rect, x_axis: &XAxis, state: &StockState) {
    let Some((open, close)) = state.regular_session() else {
        return;
    };

    for time in [open, close] {
        let Some(column) = x_axis.time_column(time, plot.width) else {
            continue;
        };

        for y in plot.top()..plot.bottom() {
            let cell = buf.get_mut(plot.x + column, y);

            // Blank braille is empty too
            if matches!(cell.symbol(), " " | "\u{2800}") {
                cell.set_symbol("┊").set_fg(THEME.gray());
            }
        }
    }
}

pub fn render_ticks(buf: &mut Buffer, plot: Rect, label_area: Rect, ticks: &[(u16, String)]) {
    let axis_y = plot.bottom();
    let label_y = label_area.top();

    if label_area.height == 0 || label_y >= buf.area.bottom() || axis_y >= buf.area.bottom() {
        return;
    }

    let mut next_free = label_area.left();

    for (column, label) in ticks {
        let x = plot.x + column;
        let label_width = label.width() as u16;

        if x >= plot.right() || label_width > label_area.width {
            continue;
        }

        let start = x
            .saturating_sub(label_width.saturating_sub(1) / 2)
            .clamp(label_area.left(), label_area.right() - label_width);

        if start < next_free {
            continue;
        }

        buf.get_mut(x, axis_y)
            .set_symbol("┬")
            .set_style(style().fg(THEME.border_axis()));

        buf.set_string(start, label_y, label, style().fg(THEME.text_normal()));

        next_free = start + label_width + 1;
    }
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;

    use super::*;

    const HOUR: i64 = 60 * 60;
    const DAY_SECS: i64 = 24 * HOUR;

    /// Monday 2026-01-05 00:00 UTC
    const MONDAY: i64 = 1_767_571_200;

    fn price(date: i64) -> Price {
        Price {
            close: 1.0,
            date,
            ..Default::default()
        }
    }

    fn labels(ticks: &[(u16, String)]) -> Vec<&str> {
        ticks.iter().map(|(_, label)| label.as_str()).collect()
    }

    #[test]
    fn column_matches_braille_canvas_mapping() {
        let width = 100;

        assert_eq!(column(0.0, width), Some(0));
        assert_eq!(column(1.0, width), Some(99));
        assert_eq!(column(0.5, width), Some(49));
        assert_eq!(column(-0.1, width), None);
        assert_eq!(column(1.1, width), None);

        // Every column's fractions map back to that column
        for col in 0..width {
            let (start, end) = column_fractions(col, width);
            assert_eq!(column((start + end) / 2.0, width), Some(col));
            assert_eq!(column(start + 1e-9, width), Some(col));
        }
    }

    #[test]
    fn session_places_prices_by_time() {
        let start = MONDAY + 8 * HOUR;
        let end = start + 8 * HOUR + 30 * 60;

        // Two hours of data, with a gap of 10 minutes
        let data = (0..120)
            .filter(|minute| !(30..40).contains(minute))
            .map(|minute| price(start + minute * 60))
            .collect::<Vec<_>>();

        let utc = FixedOffset::east_opt(0).unwrap();
        let axis = XAxis::session(&data, start, end, &utc);

        assert_eq!(axis.bounds(), [0.0, 510.0]);
        assert_eq!(axis.slot(0), 0.0);
        assert_eq!(axis.slot(30), 40.0);
        assert_eq!(axis.x(data.len() - 1), 119.5);
    }

    #[test]
    fn session_time_column() {
        let start = MONDAY + 8 * HOUR;
        let end = start + 10 * HOUR;

        let utc = FixedOffset::east_opt(0).unwrap();
        let axis = XAxis::session(&[], start, end, &utc);

        // 10 hours in 100 columns
        assert_eq!(axis.time_column(start + 2 * HOUR, 100), Some(19));
        assert_eq!(axis.time_column(start + 5 * HOUR, 100), Some(49));
    }

    #[test]
    fn session_time_column_excludes_edges() {
        let start = MONDAY + 8 * HOUR;
        let end = start + 10 * HOUR;

        let utc = FixedOffset::east_opt(0).unwrap();
        let axis = XAxis::session(&[], start, end, &utc);

        assert_eq!(axis.time_column(start, 100), None);
        assert_eq!(axis.time_column(end, 100), None);
        assert_eq!(axis.time_column(start - HOUR, 100), None);
        assert_eq!(axis.time_column(end + HOUR, 100), None);
    }

    #[test]
    fn trading_days_have_no_time_column() {
        let utc = FixedOffset::east_opt(0).unwrap();
        let data = vec![price(MONDAY), price(MONDAY + DAY_SECS)];
        let axis = XAxis::trading_days(&data, &utc);

        assert_eq!(axis.time_column(MONDAY + HOUR, 100), None);
    }

    #[test]
    fn session_ticks_are_round_times_across_the_whole_session() {
        // Stockholm, 09:00 - 17:30 CET, viewed from UTC+2
        let start = MONDAY + 8 * HOUR;
        let end = start + 8 * HOUR + 30 * 60;
        let tz = FixedOffset::east_opt(2 * HOUR as i32).unwrap();

        let data = (0..120)
            .map(|minute| price(start + minute * 60))
            .collect::<Vec<_>>();
        let axis = XAxis::session(&data, start, end, &tz);

        let width = 130;
        let ticks = axis.ticks(width);

        // 10:00 - 18:30 local, including the part of the session yet to happen
        assert_eq!(
            labels(&ticks),
            vec!["10:00", "11:00", "12:00", "13:00", "14:00", "15:00", "16:00", "17:00", "18:00"]
        );

        // Ticks are where those times are plotted, not spread over the data
        assert_eq!(Some(ticks[1].0), column(60.0 / 510.0, width));
        assert_eq!(Some(ticks[8].0), column(480.0 / 510.0, width));
    }

    #[test]
    fn session_ticks_get_coarser_on_narrow_plots() {
        let start = MONDAY + 14 * HOUR + 30 * 60;
        let end = start + 6 * HOUR + 30 * 60;
        let utc = FixedOffset::east_opt(0).unwrap();
        let axis = XAxis::session(&[], start, end, &utc);

        // Every 30 minutes, from 14:30 to 21:00
        assert_eq!(labels(&axis.ticks(200)).len(), 14);
        assert_eq!(
            labels(&axis.ticks(60)),
            vec!["15:00", "16:00", "17:00", "18:00", "19:00", "20:00", "21:00"]
        );
        assert_eq!(labels(&axis.ticks(30)), vec!["16:00", "18:00", "20:00"]);
    }

    #[test]
    fn trading_days_tick_at_day_boundaries() {
        // 1W of hourly bars, 7 bars per day
        let utc = FixedOffset::east_opt(0).unwrap();
        let data = (0..5)
            .flat_map(|day| {
                (0..7).map(move |hour| price(MONDAY + day * DAY_SECS + (14 + hour) * HOUR))
            })
            .collect::<Vec<_>>();

        let axis = XAxis::trading_days(&data, &utc);
        let ticks = axis.ticks(130);

        assert_eq!(
            labels(&ticks),
            vec!["Jan 05", "Jan 06", "Jan 07", "Jan 08", "Jan 09"]
        );
        assert_eq!(ticks[1].0, column(7.0 / 35.0, 130).unwrap());
    }

    #[test]
    fn trading_days_use_the_exchange_date() {
        // A session from 23:00 to 05:00 UTC is a single trading day at UTC+11
        let tz = FixedOffset::east_opt(11 * HOUR as i32).unwrap();
        let data = (0..2)
            .flat_map(|day| {
                (0..6).map(move |hour| price(MONDAY - HOUR + day * DAY_SECS + hour * HOUR))
            })
            .collect::<Vec<_>>();

        let axis = XAxis::trading_days(&data, &tz);

        assert_eq!(labels(&axis.ticks(130)), vec!["Jan 05", "Jan 06"]);
    }

    #[test]
    fn trading_days_use_coarser_calendar_units_for_long_ranges() {
        // ~1Y of daily bars
        let utc = FixedOffset::east_opt(0).unwrap();
        let data = (0..365)
            .map(|day| MONDAY + day * DAY_SECS)
            .filter(|date| (date / DAY_SECS + 3) % 7 < 5)
            .map(price)
            .collect::<Vec<_>>();

        let axis = XAxis::trading_days(&data, &utc);

        let wide = labels(&axis.ticks(130))
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();
        // Each month start from Feb '26 to Jan '27
        assert_eq!(wide.len(), 12);
        assert_eq!(wide[0], "Feb '26");
        assert_eq!(wide[11], "Jan '27");

        assert_eq!(
            labels(&axis.ticks(60)),
            vec!["Apr '26", "Jul '26", "Oct '26", "Jan '27"]
        );
    }

    #[test]
    fn resample_columns_aligns_values_with_slots() {
        let start = MONDAY;
        let data = (0..10)
            .map(|minute| price(start + minute * 60))
            .collect::<Vec<_>>();
        let utc = FixedOffset::east_opt(0).unwrap();

        // 10 minutes of data in a 40 minute session, 40 columns wide
        let axis = XAxis::session(&data, start, start + 40 * 60, &utc);
        let values = (1..=10).map(|v| v * 100).collect::<Vec<_>>();
        let columns = axis.resample_columns(&values, 40);

        assert_eq!(columns.len(), 40);

        // Values are only in the first quarter, the rest of the session is empty
        assert!(columns[..10].iter().all(|v| *v > 0));
        assert!(columns[11..].iter().all(|v| *v == 0));

        // Values are at the column of their slot. Columns are slightly wider than
        // a slot (the braille mapping spreads 40 slots over 39.5 columns), so they
        // pick up a little of the next slot.
        for slot in 0..10 {
            let col = column((slot as f64 + 0.5) / 40.0, 40).unwrap() as usize;
            let expected = (slot + 1) as f64 * 100.0;

            assert!(
                (columns[col] as f64 - expected).abs() / expected < 0.03,
                "{:?}",
                columns
            );
        }
    }

    #[test]
    fn resample_columns_averages_when_squashing() {
        let utc = FixedOffset::east_opt(0).unwrap();
        let data = (0..100)
            .map(|idx| price(MONDAY + idx * DAY_SECS))
            .collect::<Vec<_>>();
        let axis = XAxis::trading_days(&data, &utc);

        let columns = axis.resample_columns(&vec![50; 100], 10);

        assert_eq!(columns, vec![50; 10]);
    }

    #[test]
    fn drop_colliding_keeps_a_gap_and_prefers_later_ticks() {
        let tick = |column: u16| (column, String::from("10:00"));

        // 5 wide labels centered on their column need 7 columns between ticks
        assert_eq!(
            drop_colliding(vec![tick(0), tick(7)]),
            vec![tick(0), tick(7)]
        );
        assert_eq!(drop_colliding(vec![tick(0), tick(6)]), vec![tick(6)]);
        assert_eq!(
            drop_colliding(vec![tick(0), tick(1), tick(20), tick(40)]),
            vec![tick(1), tick(20), tick(40)]
        );
    }

    #[test]
    fn colliding_levels_fall_back_to_a_coarser_level() {
        // Ticks every 30 minutes would be ~6 columns apart, too close for their
        // labels. Rather than dropping every other one, use hourly ticks.
        let utc = FixedOffset::east_opt(0).unwrap();
        let start = MONDAY + 10 * HOUR;
        let axis = XAxis::session(&[], start, start + 4 * HOUR, &utc);

        let ticks = axis.ticks(50);
        let labels = labels(&ticks);

        assert_eq!(labels, vec!["10:00", "11:00", "12:00", "13:00", "14:00"]);
    }

    #[test]
    fn trading_days_tolerate_a_short_first_day() {
        // A single padding price on the previous trading day before 5 full days,
        // which lands in the same column as the next day's tick
        let utc = FixedOffset::east_opt(0).unwrap();
        let data = std::iter::once(price(MONDAY - 3 * DAY_SECS + 14 * HOUR))
            .chain((0..5).flat_map(|day| {
                (0..84).map(move |bar| price(MONDAY + day * DAY_SECS + 14 * HOUR + bar * 300))
            }))
            .collect::<Vec<_>>();

        let axis = XAxis::trading_days(&data, &utc);

        assert_eq!(
            labels(&axis.ticks(128)),
            vec!["Jan 05", "Jan 06", "Jan 07", "Jan 08", "Jan 09"]
        );
    }
}
