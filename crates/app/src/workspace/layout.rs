//! Band sizes: the [`Divider`]s that set them and [`fit`], which keeps the
//! main view at its minimum.

use gui::{
    canvas::Rect,
    input::{Button, Cursor},
    ui::{Element, Size, Ui},
};

use super::place::Band;
use super::{MIN_DOCK, MIN_MAIN, Workspace};

/// A value one [`Divider`] sets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Split {
    /// The band's width or height. Grows into the main view.
    Size(Band),
    /// The first dock's share of the band, while both its docks have tabs.
    Ratio(Band),
}

/// A line between two boxes, dragged along `axis` to set `split`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Divider {
    split: Split,
    axis: usize,
    /// The split as laid out, which is less than asked for when the window
    /// is too small.
    pub(super) value: f32,
    /// How far the value moves per logical pixel the pointer moves.
    rate: f32,
    min: f32,
    max: f32,
}

/// The divider being dragged.
#[derive(Clone, Copy, Debug)]
pub(super) struct Drag {
    split: Split,
    /// Where the pointer has taken the split before it's clamped, so past a
    /// limit the divider waits for the pointer to come back to it.
    wanted: f32,
}

/// What the bands are laid out from this pass.
pub(super) struct Pass {
    /// Indexed by [`Band`], after [`fit`].
    pub(super) sizes: [f32; Band::COUNT],
    /// How far the bands along each axis can grow before the main view
    /// reaches [`MIN_MAIN`].
    pub(super) spare: [f32; 2],
    pub(super) drag: Option<Drag>,
}

impl Workspace {
    /// As thick as the gap it stands in, and drawn in the line colour while
    /// it's under the pointer or dragged.
    pub(super) fn divider(&mut self, ui: &mut Ui, pass: &Pass, divider: Divider) {
        let theme = ui.theme();
        let mut size = [Size::Grow; 2];
        size[divider.axis] = Size::Fixed(theme.size.gap);
        let element = Element {
            size,
            cursor: Some(if divider.axis == 0 {
                Cursor::ResizeH
            } else {
                Cursor::ResizeV
            }),
            ..Element::DEFAULT
        };
        ui.element(element, |ui| {
            let dragged = ui.dragged(Button::Left);
            let Some(moved) = dragged.map(|moved| moved[divider.axis]) else {
                return;
            };
            let started = pass.drag.filter(|drag| drag.split == divider.split);
            // A press that hasn't moved leaves the split as the user set it,
            // not as `fit` laid it out.
            if started.is_none() && moved == 0. {
                return;
            }
            if let (None, Split::Size(band)) = (started, divider.split) {
                // What's on screen becomes what's asked for along this axis,
                // or `fit` would share the shrinking out anew as this band
                // moves, and push the others the other way.
                for other in Band::ALL
                    .into_iter()
                    .filter(|other| other.axis() == band.axis())
                {
                    self.sizes[other.index()] = pass.sizes[other.index()];
                }
            }
            let from = started.map_or(divider.value, |drag| drag.wanted);
            let wanted = moved.mul_add(divider.rate, from);
            self.drag = Some(Drag {
                split: divider.split,
                wanted,
            });
            let value = divider.clamp(wanted);
            match divider.split {
                Split::Size(band) => self.sizes[band.index()] = value,
                Split::Ratio(band) => self.ratios[band.index()] = value,
            }
        });
    }
}

impl Divider {
    /// On the inner edge of `band`, laid out `value` big, which can grow by
    /// `spare`.
    pub(super) fn size(band: Band, value: f32, spare: f32) -> Self {
        let axis = band.axis();
        let min = MIN_DOCK[axis];
        Self {
            split: Split::Size(band),
            axis,
            value,
            rate: band.grows(),
            min,
            max: (value + spare).floor().max(min),
        }
    }

    /// Between the docks of `band`, which are `extent` long together across
    /// it, so the first's share is `value`.
    pub(super) fn ratio(band: Band, value: f32, extent: f32) -> Self {
        let axis = 1 - band.axis();
        // A collapsed band would make the rate infinite.
        let extent = extent.max(1.);
        let min = (MIN_DOCK[axis] / extent).min(0.5);
        let max = 1. - min;
        Self {
            split: Split::Ratio(band),
            axis,
            value: value.clamp(min, max),
            rate: extent.recip(),
            min,
            max,
        }
    }

    /// `wanted` within its limits. Sizes land on whole pixels so dock edges
    /// stay sharp.
    fn clamp(&self, wanted: f32) -> f32 {
        let value = wanted.clamp(self.min, self.max);
        match self.split {
            Split::Size(_) => value.round(),
            Split::Ratio(_) => value,
        }
    }
}

/// `rect`'s width along axis 0, or height along axis 1.
pub(super) const fn extent(rect: Rect, axis: usize) -> f32 {
    if axis == 0 { rect.w } else { rect.h }
}

/// `rect` cut in two along `axis`, the first `at` long, with `gap` between.
pub(super) fn cut(rect: Rect, axis: usize, at: f32, gap: f32) -> [Rect; 2] {
    let second = extent(rect, axis) - at - gap;
    if axis == 0 {
        [
            Rect { w: at, ..rect },
            Rect {
                x: rect.x + at + gap,
                w: second,
                ..rect
            },
        ]
    } else {
        [
            Rect { h: at, ..rect },
            Rect {
                y: rect.y + at + gap,
                h: second,
                ..rect
            },
        ]
    }
}

/// Lays out `wanted` sizes for the `used` bands in a workspace `inner` big,
/// with a `divider` thick divider beside each band. Returns the sizes and
/// how far the bands along each axis can still grow.
///
/// Where the bands along an axis would leave the main view under
/// [`MIN_MAIN`], each gives up space in proportion to how far it's above
/// [`MIN_DOCK`], so they all reach it together. A main view already under its
/// limit stops growth but doesn't pull the dividers back, which would make
/// them jump on click.
pub(super) fn fit(
    wanted: [f32; Band::COUNT],
    used: [bool; Band::COUNT],
    inner: [f32; 2],
    divider: f32,
) -> ([f32; Band::COUNT], [f32; 2]) {
    let mut sizes = wanted;
    let mut spare = [0.; 2];
    for (axis, spare) in spare.iter_mut().enumerate() {
        let bands = || {
            Band::ALL
                .into_iter()
                .filter(move |band| band.axis() == axis && used[band.index()])
        };
        let smallest = MIN_DOCK[axis];
        let mut budget = inner[axis] - MIN_MAIN[axis];
        let (mut total, mut floor) = (0., 0.);
        for band in bands() {
            budget -= divider;
            total += sizes[band.index()];
            floor += smallest;
        }
        if total > budget {
            let excess = total - floor;
            let keep = if excess > 0. {
                ((budget - floor) / excess).clamp(0., 1.)
            } else {
                0.
            };
            total = 0.;
            for band in bands() {
                let size = &mut sizes[band.index()];
                *size = (*size - smallest).mul_add(keep, smallest);
                total += *size;
            }
        }
        *spare = (budget - total).max(0.);
    }
    (sizes, spare)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAP: f32 = 8.;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn used(bands: &[Band]) -> [bool; Band::COUNT] {
        Band::ALL.map(|band| bands.contains(&band))
    }

    fn defaults() -> [f32; Band::COUNT] {
        Band::ALL.map(Band::default_size)
    }

    #[test]
    fn fit_keeps_sizes_with_room() {
        let used = used(&[Band::LeftInner, Band::RightInner, Band::BelowMain]);
        let (sizes, spare) = fit(defaults(), used, [1440., 900.], GAP);
        assert_eq!(sizes.map(f32::to_bits), defaults().map(f32::to_bits));
        // 1440 less the main view, two dividers, 240 and 280.
        assert!(close(spare[0], 264.));
        // 900 less the main view, one divider and 240.
        assert!(close(spare[1], 292.));
    }

    #[test]
    fn fit_shrinks_in_proportion_above_the_minimum() {
        let used = used(&[Band::LeftInner, Band::RightInner]);
        let (sizes, spare) = fit(defaults(), used, [1000., 900.], GAP);
        let left = sizes[Band::LeftInner.index()];
        let right = sizes[Band::RightInner.index()];
        assert!(close(left + right, 1000. - 640. - 2. * GAP));
        assert!(close(
            (left - MIN_DOCK[0]) / (right - MIN_DOCK[0]),
            (240. - MIN_DOCK[0]) / (280. - MIN_DOCK[0]),
        ));
        assert!(close(spare[0], 0.));
    }

    #[test]
    fn fit_stops_at_the_minimum() {
        let used = used(&[Band::LeftInner, Band::RightInner]);
        let (sizes, _) = fit(defaults(), used, [500., 900.], GAP);
        assert!(close(sizes[Band::LeftInner.index()], MIN_DOCK[0]));
        assert!(close(sizes[Band::RightInner.index()], MIN_DOCK[0]));
    }

    #[test]
    fn fit_ignores_unused_bands() {
        let used = used(&[Band::LeftInner]);
        let (sizes, spare) = fit(defaults(), used, [1000., 900.], GAP);
        assert_eq!(sizes.map(f32::to_bits), defaults().map(f32::to_bits));
        assert!(close(spare[0], 1000. - 640. - GAP - 240.));
        assert!(close(spare[1], 900. - 360.));
    }

    #[test]
    fn fit_keeps_what_it_laid_out() {
        let used = used(&[Band::LeftInner, Band::RightInner]);
        let (mut laid, _) = fit(defaults(), used, [1000., 900.], GAP);
        let left = laid[Band::LeftInner.index()];
        laid[Band::RightInner.index()] -= 20.;
        let (sizes, spare) = fit(laid, used, [1000., 900.], GAP);
        assert!(close(sizes[Band::LeftInner.index()], left));
        assert!(close(spare[0], 20.));
    }

    #[test]
    fn size_divider_clamps_to_whole_pixels() {
        let divider = Divider::size(Band::RightInner, 280., 50.5);
        assert!(close(divider.clamp(0.), MIN_DOCK[0]));
        assert!(close(divider.clamp(1000.), 330.));
        assert!(close(divider.clamp(300.4), 300.));
    }

    #[test]
    fn ratio_divider_leaves_each_dock_its_minimum() {
        let divider = Divider::ratio(Band::LeftInner, 0.5, 600.);
        assert!(close(divider.clamp(0.), MIN_DOCK[1] / 600.));
        assert!(close(divider.clamp(1.), 1. - MIN_DOCK[1] / 600.));
        // Too short for both, so it stays split evenly.
        let divider = Divider::ratio(Band::LeftInner, 0.9, 200.);
        assert!(close(divider.value, 0.5));
    }
}
