use anyhow::{anyhow, bail, Result};
use clap::Parser;
use indicatif::{HumanBytes, ProgressBar, ProgressStyle};
use rdm_core::engine::{self, Job, Progress, Stopped};
use rdm_core::limiter::Limiter;
use rdm_core::probe::{self, DEFAULT_UA};
use rdm_core::reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(name = "rdm", version, about = "Fast multi-connection download manager")]
struct Args {
    /// URL to download (http:// or https://)
    url: String,

    /// Output file, or an existing directory (default: current directory, name detected automatically)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Number of parallel connections (1-64)
    #[arg(short = 'c', long, default_value_t = 16)]
    connections: usize,

    /// Speed limit for the whole download, e.g. 500K, 2M
    #[arg(short, long)]
    limit: Option<String>,

    /// Extra request header, can be repeated: -H "Cookie: a=b" -H "Referer: https://site"
    #[arg(short = 'H', long = "header")]
    header: Vec<String>,

    /// How many times a broken chunk is retried before giving up
    #[arg(long, default_value_t = 8)]
    retries: u32,

    /// User-Agent header
    #[arg(long, default_value = DEFAULT_UA)]
    user_agent: String,
}

fn parse_size(s: &str) -> Result<u64> {
    let s = s.trim();
    let (num, mult) = match s.chars().last().map(|c| c.to_ascii_uppercase()) {
        Some('K') => (&s[..s.len() - 1], 1024u64),
        Some('M') => (&s[..s.len() - 1], 1024 * 1024),
        Some('G') => (&s[..s.len() - 1], 1024 * 1024 * 1024),
        _ => (s, 1),
    };
    let n: f64 = num
        .trim()
        .parse()
        .map_err(|_| anyhow!("invalid size: {s}"))?;
    if n <= 0.0 {
        bail!("size must be greater than zero: {s}");
    }
    Ok((n * mult as f64) as u64)
}

/// "file.zip" -> "file (1).zip" -> "file (2).zip" ... if the name is taken.
fn unique_path(path: PathBuf) -> PathBuf {
    let taken = |p: &PathBuf| p.exists() && !engine::part_path(p).exists();
    if !taken(&path) {
        return path;
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let mut n = 1u32;
    loop {
        let candidate = dir.join(format!("{stem} ({n}){ext}"));
        if !candidate.exists() || engine::part_path(&candidate).exists() {
            return candidate;
        }
        n += 1;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let parsed =
        rdm_core::reqwest::Url::parse(&args.url).map_err(|e| anyhow!("invalid URL: {e}"))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        bail!("only http:// and https:// links are supported");
    }

    let limit = match &args.limit {
        Some(s) => parse_size(s)?,
        None => 0,
    };

    let mut headers = HeaderMap::new();
    for h in &args.header {
        let (k, v) = h
            .split_once(':')
            .ok_or_else(|| anyhow!("bad header (use \"Name: value\"): {h}"))?;
        headers.insert(
            HeaderName::from_bytes(k.trim().as_bytes())?,
            HeaderValue::from_str(v.trim())?,
        );
    }

    let client = probe::build_client(&args.user_agent)?;
    let info = probe::probe(&client, &args.url, &headers).await?;

    eprintln!("File   : {}", info.filename);
    match info.total {
        Some(t) => eprintln!("Size   : {}", HumanBytes(t)),
        None => eprintln!("Size   : unknown"),
    }
    eprintln!(
        "Resume : {}",
        if info.ranges { "supported" } else { "not supported" }
    );

    let out = match &args.output {
        Some(p) if p.is_dir() => unique_path(p.join(&info.filename)),
        Some(p) => p.clone(),
        None => unique_path(PathBuf::from(&info.filename)),
    };
    eprintln!("Save to: {}", out.display());
    if info.ranges {
        eprintln!("Connections: {}", args.connections.clamp(1, 64));
    }

    let total = info.total;
    let progress = Arc::new(Progress::default());
    let stop = Arc::new(AtomicBool::new(false));

    // Ctrl+C = graceful pause; run the same command again to continue.
    {
        let stop = stop.clone();
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            stop.store(true, Ordering::SeqCst);
        });
    }

    let pb = match total {
        Some(t) => {
            let pb = ProgressBar::new(t);
            pb.set_style(
                ProgressStyle::with_template(
                    "[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, ETA {eta})",
                )
                .unwrap()
                .progress_chars("=>-"),
            );
            pb
        }
        None => {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::with_template("{spinner} [{elapsed_precise}] {bytes} ({bytes_per_sec})")
                    .unwrap(),
            );
            pb
        }
    };

    let ticker = {
        let progress = progress.clone();
        let pb = pb.clone();
        tokio::spawn(async move {
            loop {
                pb.set_position(progress.downloaded());
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
    };

    let started = Instant::now();
    let result = engine::run(Job {
        client,
        probe: info,
        out: out.clone(),
        connections: args.connections,
        retries: args.retries,
        headers,
        limiter: Arc::new(Limiter::new(limit)),
        progress: progress.clone(),
        stop,
    })
    .await;
    ticker.abort();
    pb.set_position(progress.downloaded());

    match result {
        Ok(()) => {
            pb.finish();
            let secs = started.elapsed().as_secs_f64().max(0.001);
            let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
            eprintln!(
                "Done: {} in {:.1}s (average {}/s)",
                HumanBytes(size),
                secs,
                HumanBytes((progress.session() as f64 / secs) as u64)
            );
            Ok(())
        }
        Err(e) if e.downcast_ref::<Stopped>().is_some() => {
            pb.abandon();
            eprintln!("Paused. Run the same command again to resume.");
            std::process::exit(130);
        }
        Err(e) => {
            pb.abandon();
            Err(e)
        }
    }
}
