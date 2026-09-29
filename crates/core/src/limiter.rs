use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tokio::time::{sleep_until, Duration, Instant};

/// Global speed limiter shared by every connection of every download.
/// The rate can be changed at any moment; 0 means "unlimited".
pub struct Limiter {
    bytes_per_sec: AtomicU64,
    next_free: Mutex<Instant>,
}

impl Limiter {
    pub fn new(bytes_per_sec: u64) -> Self {
        Self {
            bytes_per_sec: AtomicU64::new(bytes_per_sec),
            next_free: Mutex::new(Instant::now()),
        }
    }

    pub fn set_rate(&self, bytes_per_sec: u64) {
        self.bytes_per_sec.store(bytes_per_sec, Ordering::Relaxed);
    }

    pub async fn acquire(&self, n: usize) {
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
