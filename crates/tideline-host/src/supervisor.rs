use std::time::{Duration, Instant};

const CRASH_WINDOW: Duration = Duration::from_secs(30);
pub const RESTART_BACKOFF: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashDecision {
    RestartAfter(Duration),
    StickStopped,
}

#[derive(Debug, Default)]
pub struct CrashTracker {
    last_crash: Option<Instant>,
    crashes_in_window: u32,
}

impl CrashTracker {
    pub fn record(&mut self, now: Instant, _exit_code: i32) -> CrashDecision {
        if let Some(prev) = self.last_crash {
            if now.duration_since(prev) <= CRASH_WINDOW {
                self.crashes_in_window += 1;
            } else {
                self.crashes_in_window = 1;
            }
        } else {
            self.crashes_in_window = 1;
        }
        self.last_crash = Some(now);
        if self.crashes_in_window >= 2 {
            CrashDecision::StickStopped
        } else {
            CrashDecision::RestartAfter(RESTART_BACKOFF)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_crash_restarts_after_backoff() {
        let mut t = CrashTracker::default();
        let now = Instant::now();
        assert_eq!(
            t.record(now, -1),
            CrashDecision::RestartAfter(RESTART_BACKOFF)
        );
    }

    #[test]
    fn second_crash_in_window_sticks() {
        let mut t = CrashTracker::default();
        let now = Instant::now();
        t.record(now, -1);
        let decision = t.record(now + Duration::from_secs(5), -1);
        assert_eq!(decision, CrashDecision::StickStopped);
    }

    #[test]
    fn second_crash_outside_window_restarts() {
        let mut t = CrashTracker::default();
        let now = Instant::now();
        t.record(now, -1);
        let decision = t.record(now + Duration::from_secs(40), -1);
        assert_eq!(decision, CrashDecision::RestartAfter(RESTART_BACKOFF));
    }
}
