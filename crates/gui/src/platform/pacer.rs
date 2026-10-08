//! Holds redraws to at most one per display refresh, and none while nothing
//! changed.

use std::time::{Duration, Instant};

/// Everything that changes what's on screen asks here instead of asking for a
/// redraw, so the requests between two refreshes become one redraw.
pub struct Pacer {
    /// The shortest time between redraws. `None` where the compositor paces
    /// them, like Wayland, which holds each redraw until it has shown the last.
    interval: Option<Duration>,
    last_frame: Option<Instant>,
    /// Whether something changed since the last redraw.
    dirty: bool,
}

impl Pacer {
    /// Starts dirty, so the first frame draws.
    pub const fn new(interval: Option<Duration>) -> Self {
        Self {
            interval,
            last_frame: None,
            dirty: true,
        }
    }

    /// Paces to a new refresh interval, such as after moving to another
    /// monitor. Does nothing where the compositor paces.
    pub const fn set_interval(&mut self, interval: Duration) {
        if self.interval.is_some() {
            self.interval = Some(interval);
        }
    }

    pub const fn request(&mut self) {
        self.dirty = true;
    }

    pub const fn drew(&mut self, now: Instant) {
        self.last_frame = Some(now);
    }

    /// Whether to redraw now. If not, but one is waiting, when it's due.
    pub fn poll(&mut self, now: Instant) -> (bool, Option<Instant>) {
        if !self.dirty {
            return (false, None);
        }
        let due = self
            .interval
            .zip(self.last_frame)
            .map_or(now, |(interval, last)| last + interval);
        if now < due {
            return (false, Some(due));
        }
        self.dirty = false;
        (true, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERVAL: Duration = Duration::from_millis(10);

    fn pacer(interval: Option<Duration>) -> Pacer {
        Pacer {
            interval,
            last_frame: None,
            dirty: false,
        }
    }

    #[test]
    fn nothing_changed_draws_nothing() {
        let mut pacer = pacer(Some(INTERVAL));
        assert_eq!(pacer.poll(Instant::now()), (false, None));
    }

    #[test]
    fn requests_within_an_interval_wait_for_its_end_as_one() {
        let mut pacer = pacer(Some(INTERVAL));
        let start = Instant::now();
        pacer.drew(start);
        pacer.request();
        pacer.request();
        let early = start + INTERVAL / 2;
        assert_eq!(pacer.poll(early), (false, Some(start + INTERVAL)));
        assert_eq!(pacer.poll(start + INTERVAL), (true, None));
        assert_eq!(pacer.poll(start + INTERVAL), (false, None));
    }

    #[test]
    fn compositor_paced_redraws_at_once() {
        let mut pacer = pacer(None);
        let start = Instant::now();
        pacer.drew(start);
        pacer.request();
        assert_eq!(pacer.poll(start), (true, None));
    }

    #[test]
    fn compositor_pacing_ignores_a_new_interval() {
        let mut pacer = pacer(None);
        pacer.set_interval(INTERVAL);
        assert_eq!(pacer.interval, None);
    }
}
