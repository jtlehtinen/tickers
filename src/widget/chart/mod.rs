pub use self::prices_candlestick::PricesCandlestickChart;
pub use self::prices_kagi::PricesKagiChart;
pub use self::prices_line::PricesLineChart;
pub use self::volume_bar::VolumeBarChart;

mod prices_candlestick;
pub mod prices_kagi;
mod prices_line;
mod volume_bar;

const SCROLL_STEP: usize = 2;

/// Resamples `len` items into buckets as if every item was repeated `repeat`
/// times and the result split into chunks of `chunk_size`, which is how the
/// candlestick and volume charts stretch / squash data to fit their width.
///
/// Returns, for each bucket, the `(item index, number of repeats)` pairs it
/// contains, without materializing the repeated items.
fn resample(
    len: usize,
    repeat: usize,
    chunk_size: usize,
) -> impl Iterator<Item = impl Iterator<Item = (usize, usize)>> {
    let total = len * repeat;

    let num_buckets = if chunk_size == 0 {
        0
    } else {
        total.div_ceil(chunk_size)
    };

    (0..num_buckets).map(move |bucket| {
        let start = bucket * chunk_size;
        let end = (start + chunk_size).min(total);

        (start / repeat..=(end - 1) / repeat).map(move |idx| {
            let count = ((idx + 1) * repeat).min(end) - (idx * repeat).max(start);

            (idx, count)
        })
    })
}

#[derive(Debug, Default, Clone, Copy, Hash)]
pub struct ChartState {
    pub max_offset: Option<usize>,
    pub offset: Option<usize>,
    queued_scroll: Option<ChartScrollDirection>,
}

impl ChartState {
    pub fn scroll_left(&mut self) {
        self.queued_scroll = Some(ChartScrollDirection::Left);
    }

    pub fn scroll_right(&mut self) {
        self.queued_scroll = Some(ChartScrollDirection::Right);
    }

    fn scroll(&mut self, direction: ChartScrollDirection, max_offset: usize) {
        if max_offset == 0 {
            return;
        }

        let new_offset = match direction {
            ChartScrollDirection::Left => self.offset.unwrap_or(0) + SCROLL_STEP,
            ChartScrollDirection::Right => self.offset.unwrap_or(0).saturating_sub(SCROLL_STEP),
        };

        self.offset = if new_offset == 0 {
            None
        } else {
            Some(new_offset.min(max_offset))
        };
    }

    fn offset(&mut self, max_offset: usize) -> usize {
        if max_offset == 0 {
            self.max_offset.take();
            self.offset.take();
        }

        self.max_offset = Some(max_offset);

        max_offset - self.offset.map(|o| o.min(max_offset)).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, Hash)]
enum ChartScrollDirection {
    Left,
    Right,
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use super::resample;

    /// The previous allocating implementation
    fn resample_by_repeating(len: usize, repeat: usize, chunk_size: usize) -> Vec<Vec<usize>> {
        (0..len)
            .flat_map(|idx| vec![idx; repeat])
            .chunks(chunk_size)
            .into_iter()
            .map(|chunk| chunk.collect())
            .collect()
    }

    #[test]
    fn resample_matches_repeating() {
        for len in 0..20 {
            for repeat in 0..12 {
                for chunk_size in 1..25 {
                    let expected = resample_by_repeating(len, repeat, chunk_size);

                    let actual = resample(len, repeat, chunk_size)
                        .map(|bucket| {
                            bucket
                                .flat_map(|(idx, count)| vec![idx; count])
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>();

                    assert_eq!(actual, expected, "{} {} {}", len, repeat, chunk_size);
                }
            }
        }
    }

    #[test]
    fn resample_handles_zero_chunk_size() {
        assert_eq!(resample(10, 5, 0).count(), 0);
    }
}
