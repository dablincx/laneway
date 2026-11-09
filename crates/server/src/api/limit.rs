//! Brakes on login guessing: after too many failures for one login name or one IP, further
//! attempts are refused for a while. In memory, per process.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_FAILURES: u32 = 10;
const WINDOW: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Default)]
pub struct LoginLimiter {
    failures: Arc<Mutex<HashMap<String, (u32, Instant)>>>,
}

impl LoginLimiter {
    /// `false` = too many recent failures for any of these keys.
    pub fn allowed(&self, keys: &[String]) -> bool {
        let mut map = self.failures.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        map.retain(|_, (_, since)| since.elapsed() < WINDOW);
        keys.iter().all(|k| map.get(k).is_none_or(|(n, _)| *n < MAX_FAILURES))
    }

    pub fn failed(&self, keys: &[String]) {
        let mut map = self.failures.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for key in keys {
            let entry = map.entry(key.clone()).or_insert((0, Instant::now()));
            entry.0 += 1;
        }
    }

    pub fn succeeded(&self, keys: &[String]) {
        let mut map = self.failures.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for key in keys {
            map.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_after_too_many_failures() {
        let limiter = LoginLimiter::default();
        let keys = vec!["login:alex".to_owned()];
        for _ in 0..MAX_FAILURES {
            assert!(limiter.allowed(&keys));
            limiter.failed(&keys);
        }
        assert!(!limiter.allowed(&keys));
        assert!(limiter.allowed(&["login:other".to_owned()]));
        limiter.succeeded(&keys);
        assert!(limiter.allowed(&keys));
    }
}
