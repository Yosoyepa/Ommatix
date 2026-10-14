use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use crate::Clock;

/// A virtual [`Clock`]: time only moves when someone sleeps or calls
/// [`ManualClock::advance`]. Simulations and tests use it to run settle waits instantly
/// and to get reproducible (zero) stage timings.
#[derive(Debug, Default)]
pub struct ManualClock {
    now: Mutex<Duration>,
}

impl ManualClock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn advance(&self, by: Duration) {
        let mut now = self.now.lock().unwrap_or_else(PoisonError::into_inner);
        *now = now.saturating_add(by);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Duration {
        *self.now.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn sleep(&self, duration: Duration) {
        self.advance(duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleeping_advances_virtual_time() {
        let clock = ManualClock::new();
        assert_eq!(clock.now(), Duration::ZERO);
        clock.sleep(Duration::from_millis(16));
        clock.advance(Duration::from_millis(4));
        assert_eq!(clock.now(), Duration::from_millis(20));
    }
}
