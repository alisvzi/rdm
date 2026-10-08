//! `rdm-bench` — measures real download speed for different connection counts.
//!
//! It only measures; it makes no claim about other download managers. For a fair comparison
//! run the same file, at the same time, on the same network, with each program, several times.
//!
//!   rdm-bench https://example.com/big.iso --connections 1,4,8,16 --repeat 3 --max-mib 200

use anyhow::{anyhow, bail, Result};
use clap::Parser;
use rdm_core::engine::{self, Job, Progress, Stopped};
use rdm_core::limiter::Limiter;
use rdm_core::probe::{self, DEFAULT_UA};
use rdm_core::reqwest::header::HeaderMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(
    name = "rdm-bench",
    version,
    about = "Download speed benchmark for RDM"
)]
struct Args {
    /// Direct link to a large file (http:// or https://)
    url: String,
    /// Connection counts to try, comma separated (0 = automatic)
    #[arg(short, long, default_value = "1,4,8,16")]
    connections: String,
    /// Runs per connection count
    #[arg(short, long, default_value_t = 3)]
    repeat: usize,
    /// Stop each run after this many MiB (saves data). 0 = download the whole file.
    #[arg(long, default_value_t = 100)]
    max_mib: u64,
    /// Write results to this CSV file as well
    #[arg(long)]
    csv: Option<PathBuf>,
}

fn median(v: &mut Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n == 0 {
        0.0
    } else if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

async fn one_run(url: &str, conns: usize, max_bytes: u64) -> Result<f64> {
    let client = probe::build_client(DEFAULT_UA)?;
    let info = probe::probe(&client, url, &HeaderMap::new()).await?;
    if !info.ranges {
        bail!("the server does not support Range requests; a connection comparison is meaningless");
    }
    let out = std::env::temp_dir().join(format!("rdm-bench-{}.bin", std::process::id()));
    engine::remove_partial(&out);
    let progress = Arc::new(Progress::default());
    let stop = Arc::new(AtomicBool::new(false));

    // Stops the run once enough data has arrived.
    let watcher = {
        let (p, s) = (progress.clone(), stop.clone());
        tokio::spawn(async move {
            while max_bytes > 0 && !s.load(Ordering::Relaxed) {
                if p.session() >= max_bytes {
                    s.store(true, Ordering::SeqCst);
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
    };

    let t0 = Instant::now();
    let res = engine::run(Job {
        client,
        probe: info,
        out: out.clone(),
        connections: conns,
        retries: 3,
        headers: HeaderMap::new(),
        limiter: Arc::new(Limiter::new(0)),
        item_limiter: Arc::new(Limiter::new(0)),
        use_if_range: true,
        host_cap: 0,
        progress: progress.clone(),
        stop: stop.clone(),
    })
    .await;
    let secs = t0.elapsed().as_secs_f64().max(0.001);
    watcher.abort();
    let bytes = progress.session();
    engine::remove_partial(&out);
    let _ = std::fs::remove_file(&out);
    match res {
        Ok(()) => {}
        Err(e) if e.downcast_ref::<Stopped>().is_some() => {}
        Err(e) => return Err(e),
    }
    Ok(bytes as f64 / secs / (1024.0 * 1024.0))
}

#[tokio::main]
async fn main() -> Result<()> {
    let a = Args::parse();
    let counts: Vec<usize> = a
        .connections
        .split(',')
        .map(|c| {
            c.trim()
                .parse::<usize>()
                .map_err(|_| anyhow!("bad connection count: {c}"))
        })
        .collect::<Result<_>>()?;
    let max_bytes = a.max_mib * 1024 * 1024;

    println!(
        "{:<12} {:>10} {:>10} {:>10}   runs (MiB/s)",
        "connections", "median", "best", "worst"
    );
    let mut csv = String::from("connections,run,mib_per_s\n");
    for &c in &counts {
        let mut speeds = Vec::new();
        for i in 0..a.repeat.max(1) {
            let s = one_run(&a.url, c, max_bytes).await?;
            csv.push_str(&format!("{c},{},{s:.3}\n", i + 1));
            speeds.push(s);
        }
        let runs: Vec<String> = speeds.iter().map(|s| format!("{s:.2}")).collect();
        let best = speeds.iter().cloned().fold(0.0, f64::max);
        let worst = speeds.iter().cloned().fold(f64::MAX, f64::min);
        let label = if c == 0 {
            "auto".to_string()
        } else {
            c.to_string()
        };
        println!(
            "{:<12} {:>10.2} {:>10.2} {:>10.2}   {}",
            label,
            median(&mut speeds),
            best,
            worst,
            runs.join(" ")
        );
    }
    if let Some(p) = a.csv {
        std::fs::write(&p, csv)?;
        println!("saved: {}", p.display());
    }
    println!("\nNote: results depend on your network, the server and the time of day. They say nothing about other programs unless you measure those the same way.");
    Ok(())
}
