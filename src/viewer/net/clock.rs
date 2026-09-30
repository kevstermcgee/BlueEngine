//! Where the server's notion of "now" comes from, so time can be advanced by a test instead of waited for.
//!
//! Timeouts, the handshake rate limit and the 60-second reconnect reservation all compare instants. Reading the
//! operating system's clock at those points meant the only way to test them was to sleep (and the 60-second window
//! could not be tested at all). A [`Clock`] is either the real clock or a manual one that moves only when told to:
//!
//! ```
//! use std::time::Duration;
//! use vesper3d::viewer::net::Clock;
//! let clock = Clock::manual();
//! let start = clock.now();
//! clock.advance(Duration::from_secs(60));
//! assert_eq!(clock.now() - start, Duration::from_secs(60));
//! ```
//!
//! Only decisions about timeouts and windows use it. Pacing the real-time loop (sleeping until the next tick) keeps
//! reading the real clock, because it exists to wait real time.
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// The real clock, or a manual one shared by every clone (advance one handle, every holder sees it).
#[derive(Clone, Debug, Default)]
pub struct Clock(Option<Arc<Mutex<Instant>>>);

impl Clock {
    /// The operating system's monotonic clock.
    pub fn real() -> Self {
        Self(None)
    }
    /// A clock that starts at the current instant and then moves only through [`Self::advance`].
    pub fn manual() -> Self {
        Self(Some(Arc::new(Mutex::new(Instant::now()))))
    }
    pub fn is_manual(&self) -> bool {
        self.0.is_some()
    }
    pub fn now(&self) -> Instant {
        match &self.0 {
            Some(now) => *now.lock().unwrap_or_else(|p| p.into_inner()),
            None => Instant::now(),
        }
    }
    /// Move a manual clock forward. A real clock cannot be advanced, so this does nothing for it.
    pub fn advance(&self, by: Duration) {
        if let Some(now) = &self.0 {
            let mut now = now.lock().unwrap_or_else(|p| p.into_inner());
            *now += by;
        }
    }
}
