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
    /// When the last redraw asked for the next, if it did.
    timer: Option<Instant>,
}

impl Pacer {
    /// Starts dirty, so the first frame draws.
    pub const fn new(interval: Option<Duration>) -> Self {
        Self {
            interval,
            last_frame: None,
            dirty: true,
            timer: None,
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

    /// Redraws at `at`, or as soon after as the interval allows. Each redraw
    /// forgets it, so the frame drawn asks again if it still wants one.
    pub fn request_at(&mut self, at: Instant) {
        self.timer = Some(self.timer.map_or(at, |was| was.min(at)));
    }

    /// Returns how long since the last redraw, nothing before the first.
    pub fn drew(&mut self, now: Instant) -> Duration {
        let since = self
            .last_frame
            .map_or(Duration::ZERO, |last| now.saturating_duration_since(last));
        self.last_frame = Some(now);
        self.timer = None;
        since
    }

    /// Whether to redraw now. If not, but one is waiting, when it's due.
    pub fn poll(&mut self, now: Instant) -> (bool, Option<Instant>) {
        let wanted = if self.dirty { Some(now) } else { self.timer };
        let Some(wanted) = wanted else {
            return (false, None);
        };
        let due = self
            .interval
            .zip(self.last_frame)
            .map_or(wanted, |(interval, last)| wanted.max(last + interval));
        if now < due {
            return (false, Some(due));
        }
        self.dirty = false;
        self.timer = None;
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
            timer: None,
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

    #[test]
    fn a_timer_waits_for_its_time_without_redrawing() {
        let mut pacer = pacer(Some(INTERVAL));
        let start = Instant::now();
        pacer.drew(start);
        pacer.request_at(start + INTERVAL * 5);
        assert_eq!(
            pacer.poll(start + INTERVAL),
            (false, Some(start + INTERVAL * 5))
        );
        assert_eq!(pacer.poll(start + INTERVAL * 5), (true, None));
        assert_eq!(pacer.poll(start + INTERVAL * 5), (false, None));
    }

    #[test]
    fn a_timer_sooner_than_the_interval_waits_for_it() {
        let mut pacer = pacer(Some(INTERVAL));
        let start = Instant::now();
        pacer.drew(start);
        pacer.request_at(start);
        assert_eq!(pacer.poll(start), (false, Some(start + INTERVAL)));
    }

    #[test]
    fn a_redraw_forgets_the_timer() {
        let mut pacer = pacer(Some(INTERVAL));
        let start = Instant::now();
        pacer.request_at(start + INTERVAL);
        pacer.drew(start);
        assert_eq!(pacer.poll(start + INTERVAL), (false, None));
    }
}
