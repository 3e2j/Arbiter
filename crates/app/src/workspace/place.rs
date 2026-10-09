//! Where tabs sit: [`Place`]s, sets of them, and the [`Band`]s that hold them.

/// A fixed place in the window that holds tabs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Place {
    /// What the bands leave in the middle of the window. It's in no band, and
    /// takes its space with or without tabs.
    Main,
    LeftOuterTop,
    LeftOuterBottom,
    LeftInnerTop,
    LeftInnerBottom,
    RightInnerTop,
    RightInnerBottom,
    RightOuterTop,
    RightOuterBottom,
    BelowMain,
    BottomLeft,
    BottomRight,
}

impl Place {
    pub const COUNT: usize = Self::ALL.len();

    pub(super) const ALL: [Self; 12] = [
        Self::Main,
        Self::LeftOuterTop,
        Self::LeftOuterBottom,
        Self::LeftInnerTop,
        Self::LeftInnerBottom,
        Self::RightInnerTop,
        Self::RightInnerBottom,
        Self::RightOuterTop,
        Self::RightOuterBottom,
        Self::BelowMain,
        Self::BottomLeft,
        Self::BottomRight,
    ];

    pub(super) const fn index(self) -> usize {
        self as usize
    }
}

/// A set of [`Place`]s, one bit each.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Places(u16);

impl Places {
    pub const ALL: Self = Self::new(&Place::ALL);
    pub const MAIN: Self = Self::new(&[Place::Main]);
    /// The columns either side of the main view.
    pub const SIDES: Self = Self::new(&[
        Place::LeftOuterTop,
        Place::LeftOuterBottom,
        Place::LeftInnerTop,
        Place::LeftInnerBottom,
        Place::RightInnerTop,
        Place::RightInnerBottom,
        Place::RightOuterTop,
        Place::RightOuterBottom,
    ]);
    /// The rows below the main view.
    pub const BOTTOM: Self = Self::new(&[Place::BelowMain, Place::BottomLeft, Place::BottomRight]);
    /// Every place in a band, so all but [`Place::Main`].
    pub const BANDS: Self = Self::SIDES.union(Self::BOTTOM);

    const fn new(places: &[Place]) -> Self {
        let mut set = Self(0);
        let mut rest = places;
        while let [place, tail @ ..] = rest {
            set.0 |= Self::bit(*place);
            rest = tail;
        }
        set
    }

    const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    const fn bit(place: Place) -> u16 {
        1 << place as u16
    }

    pub(super) const fn contains(self, place: Place) -> bool {
        self.0 & Self::bit(place) != 0
    }
}

/// A strip cut off one edge of the window, holding up to two docks side by
/// side across it.
///
/// Listed in the order they're cut. The bottom row goes first so it spans the
/// window. Outer columns go before inner ones so they sit against the window
/// edge. `BelowMain` goes last so it spans only the main view.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Band {
    BottomRow,
    LeftOuter,
    LeftInner,
    RightOuter,
    RightInner,
    BelowMain,
}

impl Band {
    pub(super) const COUNT: usize = Self::ALL.len();

    pub(super) const ALL: [Self; 6] = [
        Self::BottomRow,
        Self::LeftOuter,
        Self::LeftInner,
        Self::RightOuter,
        Self::RightInner,
        Self::BelowMain,
    ];

    /// The bands against the main view. A tab dragged over the main view can
    /// open one of these. The others only take tabs while they're open.
    pub(super) const AROUND_MAIN: [Self; 3] = [Self::LeftInner, Self::RightInner, Self::BelowMain];

    /// The band holding `place`, or `None` for [`Place::Main`].
    pub(super) fn of(place: Place) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|band| band.docks().contains(&place))
    }

    /// Top then bottom for a column, left then right for a row.
    pub(super) const fn docks(self) -> &'static [Place] {
        match self {
            Self::BottomRow => &[Place::BottomLeft, Place::BottomRight],
            Self::LeftOuter => &[Place::LeftOuterTop, Place::LeftOuterBottom],
            Self::LeftInner => &[Place::LeftInnerTop, Place::LeftInnerBottom],
            Self::RightOuter => &[Place::RightOuterTop, Place::RightOuterBottom],
            Self::RightInner => &[Place::RightInnerTop, Place::RightInnerBottom],
            Self::BelowMain => &[Place::BelowMain],
        }
    }

    /// Its width for a column, or its height for a row, in logical pixels.
    pub(super) const fn default_size(self) -> f32 {
        match self {
            Self::BottomRow => 200.,
            Self::LeftOuter | Self::LeftInner | Self::RightOuter | Self::BelowMain => 240.,
            Self::RightInner => 280.,
        }
    }

    /// The axis its size is along: 0 for a column's width, 1 for a row's
    /// height. A row is cut off the bottom, so it spans across and stacks its
    /// docks left to right.
    pub(super) const fn axis(self) -> usize {
        match self {
            Self::BottomRow | Self::BelowMain => 1,
            Self::LeftOuter | Self::LeftInner | Self::RightOuter | Self::RightInner => 0,
        }
    }

    /// How its size changes as its divider moves right or down. The divider
    /// is on its inner edge, so it's after the band on the left and before it
    /// everywhere else.
    pub(super) const fn grows(self) -> f32 {
        match self {
            Self::LeftOuter | Self::LeftInner => 1.,
            Self::BottomRow | Self::RightOuter | Self::RightInner | Self::BelowMain => -1.,
        }
    }

    pub(super) const fn index(self) -> usize {
        self as usize
    }
}
