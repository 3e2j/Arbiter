//! What changes over time rather than all at once.
//!
//! A [`Tween`] runs between 0 and 1 by [`Ui::elapsed`], so it takes as long at
//! any frame rate, and asks for frames through [`Ui::animate`] until it
//! arrives. What it moves, and how, is the caller's: put its progress through
//! a curve, such as [`ease_out`] or one of the caller's own, and onto any
//! [`Element`](crate::ui::Element) value, such as an offset or a colour's
//! alpha.
//!
//! ```ignore
//! let shown = ease_out(self.panel.step(ui, 0.2));
//! element.offset[0] = (1. - shown) * -width;
//! ```

use crate::ui::Ui;

/// Progress from 0 to 1, heading for one end or the other. The caller keeps
/// it, as with a component's state.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Tween {
    at: f32,
    forward: bool,
}

impl Tween {
    /// At 0, staying there.
    pub const START: Self = Self {
        at: 0.,
        forward: false,
    };
    /// At 1, staying there.
    pub const END: Self = Self {
        at: 1.,
        forward: true,
    };

    /// Heads for 1 when `forward`, 0 otherwise, from wherever it is, so one
    /// sent back partway takes as long to return as it went.
    pub const fn go(&mut self, forward: bool) {
        self.forward = forward;
    }

    /// Goes straight to 1 when `forward`, 0 otherwise.
    pub const fn jump(&mut self, forward: bool) {
        self.forward = forward;
        self.at = if forward { 1. } else { 0. };
    }

    /// Where it is, without moving it.
    #[must_use]
    pub const fn progress(self) -> f32 {
        self.at
    }

    /// Moves toward where it's heading by this frame's elapsed time, taking
    /// `seconds` for the whole way, and asks for the next frame until it
    /// arrives. Once a pass. Returns where it is.
    pub fn step(&mut self, ui: &mut Ui, seconds: f32) -> f32 {
        let by = if seconds > 0. {
            ui.elapsed() / seconds
        } else {
            f32::INFINITY
        };
        let arrived = if self.forward {
            self.at = (self.at + by).min(1.);
            self.at >= 1.
        } else {
            self.at = (self.at - by).max(0.);
            self.at <= 0.
        };
        if !arrived {
            ui.animate();
        }
        self.at
    }
}

/// Starts slow and speeds up.
#[must_use]
pub const fn ease_in(t: f32) -> f32 {
    t * t * t
}

/// Starts fast and slows down to arrive, as something entering does.
#[must_use]
pub const fn ease_out(t: f32) -> f32 {
    let u = 1. - t;
    1. - u * u * u
}

/// Slow at both ends.
#[must_use]
pub const fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4. * t * t * t
    } else {
        let u = 2. - 2. * t;
        1. - u * u * u / 2.
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::ui::testing::Kept;

    const SECONDS: f32 = 0.5;

    /// Sends a tween forward, then runs `frames` frames evenly over `seconds`,
    /// returning where it ends up.
    fn after(seconds: f32, frames: u16) -> f32 {
        let mut kept = Kept::default();
        let mut tween = Tween::START;
        tween.go(true);
        let mut at = 0.;
        for _ in 0..frames {
            kept.input.set_elapsed(seconds / f32::from(frames));
            at = kept.pass(|ui| tween.step(ui, SECONDS));
        }
        at
    }

    #[test]
    fn moves_as_far_at_any_frame_rate() {
        for frames in [6, 30, 144] {
            assert!((after(SECONDS / 2., frames) - 0.5).abs() < 1e-3);
            assert!((after(SECONDS * 2., frames) - 1.).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn asks_for_frames_only_while_moving() {
        let mut kept = Kept::default();
        let mut tween = Tween::START;
        kept.pass(|ui| tween.step(ui, SECONDS));
        assert_eq!(kept.out.redraw_in, None);
        tween.go(true);
        kept.pass(|ui| tween.step(ui, SECONDS));
        assert_eq!(kept.out.redraw_in, Some(Duration::ZERO));
        kept.input.set_elapsed(SECONDS);
        kept.pass(|ui| tween.step(ui, SECONDS));
        assert_eq!(kept.out.redraw_in, None);
    }

    #[test]
    fn a_pass_run_again_doesnt_move_it() {
        let mut kept = Kept::default();
        let mut tween = Tween::START;
        tween.go(true);
        kept.input.set_elapsed(SECONDS / 2.);
        kept.pass(|ui| tween.step(ui, SECONDS));
        // Kept clears the input between passes, as the host does.
        let again = kept.pass(|ui| tween.step(ui, SECONDS));
        assert!((again - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn sent_back_partway_it_returns_from_where_it_is() {
        let mut kept = Kept::default();
        let mut tween = Tween::START;
        tween.go(true);
        kept.input.set_elapsed(SECONDS / 4.);
        kept.pass(|ui| tween.step(ui, SECONDS));
        tween.go(false);
        kept.input.set_elapsed(SECONDS / 8.);
        let at = kept.pass(|ui| tween.step(ui, SECONDS));
        assert!((at - 0.125).abs() < f32::EPSILON);
    }

    #[test]
    fn no_time_arrives_at_once() {
        let mut kept = Kept::default();
        let mut tween = Tween::START;
        tween.go(true);
        let at = kept.pass(|ui| tween.step(ui, 0.));
        assert!((at - 1.).abs() < f32::EPSILON);
        assert_eq!(kept.out.redraw_in, None);
    }

    #[test]
    fn curves_keep_their_ends() {
        for curve in [ease_in, ease_out, ease_in_out] {
            assert!(curve(0.).abs() < f32::EPSILON);
            assert!((curve(1.) - 1.).abs() < f32::EPSILON);
        }
        assert!((ease_in_out(0.5) - 0.5).abs() < f32::EPSILON);
    }
}
