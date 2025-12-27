use dashmap::DashMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::config::ModelSettings;

#[derive(Clone)]
pub struct RateLimiter {
    limiters: Arc<DashMap<(String, String), Arc<ModelLimiter>>>,
}

struct ModelLimiter {
    rps: Option<SlidingWindow>,
    rpm: Option<SlidingWindow>,
    concurrent: Option<ConcurrentLimiter>,
}

struct SlidingWindow {
    window: Duration,
    max_requests: u32,
    timestamps: Mutex<VecDeque<Instant>>,
}

struct ConcurrentLimiter {
    max: u32,
    current: AtomicU32,
}

pub struct ConcurrentGuard {
    limiter: Option<Arc<ModelLimiter>>,
}

impl Drop for ConcurrentGuard {
    fn drop(&mut self) {
        if let Some(ref limiter) = self.limiter {
            if let Some(ref concurrent) = limiter.concurrent {
                concurrent.current.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }
}

impl SlidingWindow {
    fn new(window: Duration, max_requests: u32) -> Self {
        Self {
            window,
            max_requests,
            timestamps: Mutex::new(VecDeque::new()),
        }
    }

    fn try_acquire(&self) -> bool {
        let now = Instant::now();
        let cutoff = now - self.window;

        let mut timestamps = self.timestamps.lock().unwrap();

        while timestamps.front().is_some_and(|&t| t < cutoff) {
            timestamps.pop_front();
        }

        if timestamps.len() < self.max_requests as usize {
            timestamps.push_back(now);
            true
        } else {
            false
        }
    }
}

impl ConcurrentLimiter {
    fn new(max: u32) -> Self {
        Self {
            max,
            current: AtomicU32::new(0),
        }
    }

    fn try_acquire(&self) -> bool {
        loop {
            let current = self.current.load(Ordering::SeqCst);
            if current >= self.max {
                return false;
            }
            if self
                .current
                .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return true;
            }
        }
    }
}

impl ModelLimiter {
    fn new(config: &ModelSettings) -> Self {
        Self {
            rps: config
                .rps
                .map(|limit| SlidingWindow::new(Duration::from_secs(1), limit)),
            rpm: config
                .rpm
                .map(|limit| SlidingWindow::new(Duration::from_secs(60), limit)),
            concurrent: config.concurrent.map(ConcurrentLimiter::new),
        }
    }

    fn try_acquire_rate(&self) -> bool {
        if let Some(ref rps) = self.rps {
            if !rps.try_acquire() {
                return false;
            }
        }
        if let Some(ref rpm) = self.rpm {
            if !rpm.try_acquire() {
                return false;
            }
        }
        true
    }

    fn try_acquire_concurrent(&self) -> bool {
        match &self.concurrent {
            Some(c) => c.try_acquire(),
            None => true,
        }
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            limiters: Arc::new(DashMap::new()),
        }
    }

    pub fn register(&self, provider: String, model: String, config: ModelSettings) {
        self.limiters
            .insert((provider, model), Arc::new(ModelLimiter::new(&config)));
    }

    pub fn try_acquire(&self, provider: &str, model: &str) -> Result<ConcurrentGuard, ()> {
        let key = (provider.to_string(), model.to_string());

        let limiter = match self.limiters.get(&key) {
            Some(l) => Arc::clone(&l),
            None => return Ok(ConcurrentGuard { limiter: None }),
        };

        // 先檢查 concurrent（不消耗 quota），再檢查 rate
        if !limiter.try_acquire_concurrent() {
            return Err(());
        }

        if !limiter.try_acquire_rate() {
            // rate 失敗，釋放 concurrent
            if let Some(ref c) = limiter.concurrent {
                c.current.fetch_sub(1, Ordering::SeqCst);
            }
            return Err(());
        }

        Ok(ConcurrentGuard {
            limiter: Some(limiter),
        })
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concurrent_limit() {
        let limiter = RateLimiter::new();
        limiter.register(
            "test".into(),
            "model".into(),
            ModelSettings {
                rps: None,
                rpm: None,
                concurrent: Some(2),
                timeout_secs: None,
            },
        );

        let g1 = limiter.try_acquire("test", "model");
        assert!(g1.is_ok());

        let g2 = limiter.try_acquire("test", "model");
        assert!(g2.is_ok());

        // 第三個應該被拒絕
        let g3 = limiter.try_acquire("test", "model");
        assert!(g3.is_err());

        // 釋放一個後應該可以再拿
        drop(g1);
        let g4 = limiter.try_acquire("test", "model");
        assert!(g4.is_ok());
    }

    #[test]
    fn test_rps_limit() {
        let limiter = RateLimiter::new();
        limiter.register(
            "test".into(),
            "model".into(),
            ModelSettings {
                rps: Some(3),
                rpm: None,
                concurrent: None,
                timeout_secs: None,
            },
        );

        // 前 3 個應該成功
        for _ in 0..3 {
            assert!(limiter.try_acquire("test", "model").is_ok());
        }

        // 第 4 個應該被拒絕（同一秒內）
        assert!(limiter.try_acquire("test", "model").is_err());
    }

    #[test]
    fn test_unregistered_provider_passes() {
        let limiter = RateLimiter::new();

        // 未註冊的 provider 應該直接通過
        assert!(limiter.try_acquire("unknown", "model").is_ok());
    }

    #[test]
    fn test_combined_limits() {
        let limiter = RateLimiter::new();
        limiter.register(
            "test".into(),
            "model".into(),
            ModelSettings {
                rps: Some(5),
                rpm: Some(10),
                concurrent: Some(2),
                timeout_secs: None,
            },
        );

        // concurrent = 2，只能同時持有 2 個
        let g1 = limiter.try_acquire("test", "model").unwrap();
        let g2 = limiter.try_acquire("test", "model").unwrap();
        assert!(limiter.try_acquire("test", "model").is_err());

        drop(g1);
        drop(g2);

        // 已經消耗了 2 個 rps，再拿 3 個應該成功
        for _ in 0..3 {
            let _ = limiter.try_acquire("test", "model").unwrap();
        }

        // rps = 5，已經用了 5 個，應該被拒絕
        assert!(limiter.try_acquire("test", "model").is_err());
    }
}
