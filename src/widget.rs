use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::widgets::StatefulWidget;

pub use self::add_stock::{AddStockState, AddStockWidget};
pub use self::alerts::{AlertsState, AlertsWidget};
pub use self::chart_configuration::{ChartConfigurationWidget, KagiOptions};
pub use self::help::{HelpWidget, HELP_HEIGHT, HELP_WIDTH};
pub use self::options::{OptionsState, OptionsWidget};
pub use self::stock::{StockState, StockWidget};
pub use self::stock_summary::StockSummaryWidget;

mod add_stock;
mod alerts;
pub mod block;
mod chart;
pub mod chart_configuration;
mod help;
pub mod options;
mod stock;
mod stock_summary;

pub trait CachableWidget<T: Hash>: StatefulWidget<State = T> + Sized {
    fn cache_state_mut(state: &mut <Self as StatefulWidget>::State) -> &mut CacheState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut <Self as StatefulWidget>::State);

    fn render_cached(
        self,
        area: Rect,
        buf: &mut Buffer,
        state: &mut <Self as StatefulWidget>::State,
    ) {
        // Hash our state
        let mut hasher = DefaultHasher::default();
        state.hash(&mut hasher);
        let hash = hasher.finish();

        // Only cells within the buffer can be cached / restored
        let cached_area = area.intersection(buf.area);

        let cache = <Self as CachableWidget<T>>::cache_state_mut(state);

        // If current hash and layout matches previous, use cached cells instead of re-rendering
        if hash == cache.prev_hash && area == cache.prev_area {
            for ((x, y), cached_cell) in positions(cached_area).zip(&cache.prev_content) {
                buf[(x, y)] = cached_cell.clone();
            }
        }
        // Otherwise re-render and store those values in the cache
        else {
            <Self as CachableWidget<T>>::render(self, area, buf, state);

            let cache = <Self as CachableWidget<T>>::cache_state_mut(state);
            cache.prev_hash = hash;
            cache.prev_area = area;
            cache.prev_content = positions(cached_area)
                .map(|(x, y)| buf[(x, y)].clone())
                .collect();
        }
    }
}

/// All `(x, y)` positions within `area`, row by row
fn positions(area: Rect) -> impl Iterator<Item = (u16, u16)> {
    (area.top()..area.bottom()).flat_map(move |y| (area.left()..area.right()).map(move |x| (x, y)))
}

#[derive(Debug, Clone, Default)]
pub struct CacheState {
    prev_area: Rect,
    prev_hash: u64,
    /// Cells of `prev_area` (clipped to the buffer), row by row
    prev_content: Vec<Cell>,
}

#[cfg(test)]
mod tests {
    use std::cell::Cell as StdCell;

    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::StatefulWidget;

    use super::{CachableWidget, CacheState};

    #[derive(Default)]
    struct TestState {
        value: u8,
        renders: StdCell<usize>,
        cache_state: CacheState,
    }

    impl std::hash::Hash for TestState {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.value.hash(state);
        }
    }

    struct TestWidget;

    impl StatefulWidget for TestWidget {
        type State = TestState;

        fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
            self.render_cached(area, buf, state);
        }
    }

    impl CachableWidget<TestState> for TestWidget {
        fn cache_state_mut(state: &mut TestState) -> &mut CacheState {
            &mut state.cache_state
        }

        fn render(self, area: Rect, buf: &mut Buffer, state: &mut TestState) {
            state.renders.set(state.renders.get() + 1);

            for y in area.top()..area.bottom() {
                for x in area.left()..area.right() {
                    let symbol =
                        format!("{}", (x as usize + y as usize + state.value as usize) % 10);
                    buf[(x, y)].set_symbol(&symbol);
                }
            }
        }
    }

    fn render_frame(buf_area: Rect, area: Rect, state: &mut TestState) -> Buffer {
        // Each frame starts from an empty buffer, like ratatui's double buffering
        let mut buf = Buffer::empty(buf_area);
        StatefulWidget::render(TestWidget, area, &mut buf, state);
        buf
    }

    #[test]
    fn cache_restores_identical_content() {
        let buf_area = Rect::new(0, 0, 250, 200);
        let area = Rect::new(5, 150, 240, 40);

        let mut state = TestState::default();

        let first = render_frame(buf_area, area, &mut state);
        let second = render_frame(buf_area, area, &mut state);

        assert_eq!(state.renders.get(), 1);
        assert_eq!(first, second);
    }

    #[test]
    fn cache_only_touches_its_area() {
        let buf_area = Rect::new(0, 0, 20, 10);
        let area = Rect::new(2, 3, 5, 4);

        let mut state = TestState::default();
        render_frame(buf_area, area, &mut state);
        let cached = render_frame(buf_area, area, &mut state);

        for y in 0..10 {
            for x in 0..20 {
                let inside = (2..7).contains(&x) && (3..7).contains(&y);
                assert_eq!(cached[(x, y)].symbol() == " ", !inside, "({}, {})", x, y);
            }
        }
    }

    #[test]
    fn cache_rerenders_on_state_or_area_change() {
        let buf_area = Rect::new(0, 0, 20, 10);
        let mut state = TestState::default();

        render_frame(buf_area, Rect::new(0, 0, 10, 5), &mut state);
        state.value = 1;
        render_frame(buf_area, Rect::new(0, 0, 10, 5), &mut state);
        render_frame(buf_area, Rect::new(0, 0, 10, 6), &mut state);

        assert_eq!(state.renders.get(), 3);
    }
}
