use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tokio::time::{sleep_until, Duration, Instant};

/// Global speed limiter shared by every connection of every download.
/// The rate can be changed at any moment; 0 means "unlimited".
pub struct Limiter {
    /// A second limiter that applies on top of this one (queue limit above a download limit).
    parent: Option<std::sync::Arc<Limiter>>,
    bytes_per_sec: AtomicU64,
    next_free: Mutex<Instant>,
}

impl Limiter {
    pub fn new(bytes_per_sec: u64) -> Self {
        Self {
            parent: None,
            bytes_per_sec: AtomicU64::new(bytes_per_sec),
            next_free: Mutex::new(Instant::now()),
        }
    }

    /// A limiter whose bytes also have to pass through `parent`.
    pub fn with_parent(bytes_per_sec: u64, parent: std::sync::Arc<Limiter>) -> Self {
        Self {
            parent: Some(parent),
            ..Self::new(bytes_per_sec)
        }
    }

    pub fn set_rate(&self, bytes_per_sec: u64) {
        self.bytes_per_sec.store(bytes_per_sec, Ordering::Relaxed);
    }

    pub async fn acquire(&self, n: usize) {
        self.acquire_own(n).await;
        let mut cur = self.parent.as_ref();
        while let Some(p) = cur {
            p.acquire_own(n).await;
            cur = p.parent.as_ref();
        }
    }

    async fn acquire_own(&self, n: usize) {
        let rate = self.bytes_per_sec.load(Ordering::Relaxed);
        if rate == 0 {
            return;
        }
        let start = {
            let mut next = self.next_free.lock().unwrap();
            let now = Instant::now();
            let start = if *next > now { *next } else { now };
            *next = start + Duration::from_secs_f64(n as f64 / rate as f64);
            start
        };
        sleep_until(start).await;
    }
}
