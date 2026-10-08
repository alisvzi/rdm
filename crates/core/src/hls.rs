//! HLS (`.m3u8`) support for plain, unencrypted, finished (VOD) playlists.
//!
//! Not supported, and refused with a clear message instead of producing a broken file:
//! encrypted streams (AES-128 / SAMPLE-AES / DRM), live streams, byte-range segments and
//! DASH (`.mpd`). Resuming a paused HLS download starts it again from the beginning.
//!
//! The playlist parser is pure string logic with unit tests.

use crate::engine::{Progress, Stopped};
use crate::limiter::Limiter;
use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use reqwest::{header, Client};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

#[derive(Debug, PartialEq)]
pub struct Variant {
    pub url: String,
    pub bandwidth: u64,
    pub resolution: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct Media {
    /// Initialization segment (fMP4 `EXT-X-MAP`), written first.
    pub init: Option<String>,
    pub segments: Vec<String>,
    pub duration: f64,
}

#[derive(Debug, PartialEq)]
pub enum Playlist {
    Master(Vec<Variant>),
    Media(Media),
}

/// True for a URL or content type that looks like an HLS playlist.
pub fn is_hls(url: &str, content_type: Option<&str>) -> bool {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();
    path.ends_with(".m3u8")
        || content_type.map_or(false, |c| {
            let c = c.to_ascii_lowercase();
            c.contains("mpegurl")
        })
}

pub fn is_dash(url: &str, content_type: Option<&str>) -> bool {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();
    path.ends_with(".mpd") || content_type.map_or(false, |c| c.contains("dash+xml"))
}

/// "movie.m3u8" -> "movie.ts"; other names are unchanged.
pub fn output_name(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext))
            if ext.eq_ignore_ascii_case("m3u8") || ext.eq_ignore_ascii_case("m3u") =>
        {
            format!("{stem}.ts")
        }
        _ => name.to_string(),
    }
}

/// Resolves a playlist entry against the playlist address (absolute, root-relative, relative).
pub fn resolve(base: &str, rel: &str) -> String {
    let rel = rel.trim();
    if rel.starts_with("http://") || rel.starts_with("https://") {
        return rel.to_string();
    }
    let base = base.split(['?', '#']).next().unwrap_or(base);
    let scheme_end = base.find("://").map_or(0, |i| i + 3);
    let origin_end = base[scheme_end..]
        .find('/')
        .map_or(base.len(), |i| scheme_end + i);
    if let Some(rest) = rel.strip_prefix("//") {
        let scheme = &base[..scheme_end.saturating_sub(3)];
        return format!("{scheme}://{rest}");
    }
    if rel.starts_with('/') {
        return format!("{}{}", &base[..origin_end], rel);
    }
    let dir_end = base[origin_end..]
        .rfind('/')
        .map_or(origin_end, |i| origin_end + i + 1);
    let dir = if dir_end <= origin_end {
        format!("{}/", &base[..origin_end])
    } else {
        base[..dir_end].to_string()
    };
    // Collapse "./" and "../" segments.
    let mut out = format!("{dir}{rel}");
    while let Some(i) = out.find("/./") {
        out.replace_range(i..i + 2, "");
    }
    while let Some(i) = out.find("/../") {
        let head = &out[..i];
        match head.rfind('/') {
            Some(j) if j >= scheme_end => out.replace_range(j..i + 3, ""),
            _ => break,
        }
    }
    out
}

/// `KEY=value,KEY2="quoted, value"` -> pairs.
fn attributes(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut key = String::new();
    let mut val = String::new();
    let (mut in_val, mut quoted) = (false, false);
    let mut push = |key: &mut String, val: &mut String, out: &mut Vec<(String, String)>| {
        if !key.trim().is_empty() {
            out.push((key.trim().to_ascii_uppercase(), val.clone()));
        }
        key.clear();
        val.clear();
    };
    for c in s.chars() {
        match (c, in_val, quoted) {
            ('=', false, _) => in_val = true,
            ('"', true, _) => quoted = !quoted,
            (',', true, false) => {
                push(&mut key, &mut val, &mut out);
                in_val = false;
            }
            (_, true, _) => val.push(c),
            (_, false, _) => key.push(c),
        }
    }
    push(&mut key, &mut val, &mut out);
    out
}

fn attr<'a>(list: &'a [(String, String)], name: &str) -> Option<&'a str> {
    list.iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

pub fn parse(text: &str, base: &str) -> Result<Playlist> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    if lines.next().map(|l| l.trim_start_matches('\u{feff}')) != Some("#EXTM3U") {
        bail!("this is not a valid HLS playlist (#EXTM3U is missing)");
    }
    let mut variants = Vec::new();
    let mut pending_variant: Option<(u64, Option<String>)> = None;
    let mut segments = Vec::new();
    let mut init = None;
    let mut duration = 0.0f64;
    let mut next_duration: Option<f64> = None;
    let mut ended = false;

    for line in lines {
        if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let a = attributes(rest);
            let bw = attr(&a, "BANDWIDTH")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            pending_variant = Some((bw, attr(&a, "RESOLUTION").map(str::to_string)));
        } else if let Some(rest) = line.strip_prefix("#EXT-X-KEY:") {
            let a = attributes(rest);
            let method = attr(&a, "METHOD").unwrap_or("NONE");
            if !method.eq_ignore_ascii_case("NONE") {
                bail!("this stream is encrypted ({method}); RDM does not download encrypted or DRM-protected streams");
            }
        } else if let Some(rest) = line.strip_prefix("#EXT-X-MAP:") {
            let a = attributes(rest);
            if attr(&a, "BYTERANGE").is_some() {
                bail!("byte-range initialization segments are not supported");
            }
            init = attr(&a, "URI").map(|u| resolve(base, u));
        } else if line.starts_with("#EXT-X-BYTERANGE") {
            bail!("streams that use byte-range segments are not supported");
        } else if let Some(rest) = line.strip_prefix("#EXTINF:") {
            next_duration = rest.split(',').next().and_then(|d| d.trim().parse().ok());
        } else if line == "#EXT-X-ENDLIST" {
            ended = true;
        } else if line.starts_with('#') {
            continue;
        } else if let Some((bw, res)) = pending_variant.take() {
            variants.push(Variant {
                url: resolve(base, line),
                bandwidth: bw,
                resolution: res,
            });
        } else {
            segments.push(resolve(base, line));
            duration += next_duration.take().unwrap_or(0.0);
        }
    }

    if !variants.is_empty() {
        return Ok(Playlist::Master(variants));
    }
    if segments.is_empty() {
        bail!("the playlist has no media segments");
    }
    if !ended {
        bail!("this is a live stream (no end marker); RDM downloads only finished recordings");
    }
    Ok(Playlist::Media(Media {
        init,
        segments,
        duration,
    }))
}

/// The variant with the highest bandwidth.
pub fn best_variant(v: &[Variant]) -> Option<&Variant> {
    v.iter().max_by_key(|x| x.bandwidth)
}

pub struct HlsJob {
    pub client: Client,
    pub url: String,
    pub out: PathBuf,
    pub headers: header::HeaderMap,
    pub retries: u32,
    pub limiter: Arc<Limiter>,
    pub item_limiter: Arc<Limiter>,
    pub progress: Arc<Progress>,
    pub stop: Arc<AtomicBool>,
}

async fn get_bytes(
    job: &HlsJob,
    url: &str,
    limiter: &Limiter,
    item_limiter: &Limiter,
) -> Result<Vec<u8>> {
    let mut attempt = 0u32;
    loop {
        if job.stop.load(Ordering::Relaxed) {
            return Err(anyhow::Error::new(Stopped));
        }
        let res = async {
            let resp = job
                .client
                .get(url)
                .headers(job.headers.clone())
                .send()
                .await?;
            let status = resp.status();
            if !status.is_success() {
                // 4xx other than 408/429 will not get better by retrying.
                let fatal =
                    status.is_client_error() && status.as_u16() != 408 && status.as_u16() != 429;
                return Ok::<_, anyhow::Error>(Err((fatal, anyhow!("HTTP {status} for {url}"))));
            }
            let bytes = resp.bytes().await?;
            Ok(Ok(bytes.to_vec()))
        }
        .await;
        match res {
            Ok(Ok(b)) => {
                item_limiter.acquire(b.len()).await;
                limiter.acquire(b.len()).await;
                return Ok(b);
            }
            Ok(Err((true, e))) => return Err(e),
            Ok(Err((false, e))) | Err(e) => {
                attempt += 1;
                if attempt > job.retries {
                    return Err(e.context("a segment could not be downloaded"));
                }
                job.progress.count_retry();
                tokio::time::sleep(Duration::from_millis(500 * attempt.min(10) as u64)).await;
            }
        }
    }
}

/// Downloads the playlist's segments (several at a time, written in order) into one `.ts`/`.mp4` file.
pub async fn run(job: HlsJob) -> Result<()> {
    job.progress.begin_unknown_size();
    let text =
        String::from_utf8_lossy(&get_bytes(&job, &job.url, &job.limiter, &job.item_limiter).await?)
            .to_string();
    let mut base = job.url.clone();
    let mut parsed = parse(&text, &base)?;
    if let Playlist::Master(v) = &parsed {
        let best = best_variant(v).ok_or_else(|| anyhow!("no stream found in the playlist"))?;
        base = best.url.clone();
        let text = String::from_utf8_lossy(
            &get_bytes(&job, &base, &job.limiter, &job.item_limiter).await?,
        )
        .to_string();
        parsed = parse(&text, &base)?;
    }
    let media = match parsed {
        Playlist::Media(m) => m,
        Playlist::Master(_) => bail!("nested master playlists are not supported"),
    };

    let part = part_of(&job.out);
    let mut file = tokio::fs::File::create(&part)
        .await
        .with_context(|| format!("cannot create {}", part.display()))?;

    if let Some(init) = &media.init {
        let b = get_bytes(&job, init, &job.limiter, &job.item_limiter).await?;
        file.write_all(&b).await?;
        job.progress.add_bytes(b.len() as u64);
    }

    let job_ref = &job;
    let mut stream =
        futures_util::stream::iter(media.segments.iter().cloned())
            .map(|u| async move {
                get_bytes(job_ref, &u, &job_ref.limiter, &job_ref.item_limiter).await
            })
            .buffered(6);
    while let Some(seg) = stream.next().await {
        let b = match seg {
            Ok(b) => b,
            Err(e) => {
                drop(stream);
                let _ = file.flush().await;
                drop(file);
                let _ = std::fs::remove_file(&part);
                return Err(e);
            }
        };
        file.write_all(&b).await?;
        job.progress.add_bytes(b.len() as u64);
    }
    drop(stream);
    file.flush().await?;
    file.sync_all().await?;
    drop(file);
    crate::fsutil::safe_rename(&part, &job.out)
        .with_context(|| format!("cannot move the finished file to {}", job.out.display()))?;
    Ok(())
}

fn part_of(out: &Path) -> PathBuf {
    crate::engine::part_path(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://cdn.example.com/video/index.m3u8?token=abc";

    #[test]
    fn resolves_addresses() {
        assert_eq!(
            resolve(BASE, "seg1.ts"),
            "https://cdn.example.com/video/seg1.ts"
        );
        assert_eq!(resolve(BASE, "/a/b.ts"), "https://cdn.example.com/a/b.ts");
        assert_eq!(resolve(BASE, "https://x.org/s.ts"), "https://x.org/s.ts");
        assert_eq!(resolve(BASE, "//x.org/s.ts"), "https://x.org/s.ts");
        assert_eq!(
            resolve(BASE, "../hi/seg.ts"),
            "https://cdn.example.com/hi/seg.ts"
        );
        assert_eq!(
            resolve(BASE, "./seg.ts"),
            "https://cdn.example.com/video/seg.ts"
        );
        assert_eq!(
            resolve("https://h.com/x.m3u8", "s.ts"),
            "https://h.com/s.ts"
        );
    }

    #[test]
    fn detects_hls_and_names() {
        assert!(is_hls("https://a.com/x.M3U8?t=1", None));
        assert!(is_hls(
            "https://a.com/play",
            Some("application/vnd.apple.mpegurl")
        ));
        assert!(!is_hls("https://a.com/x.mp4", Some("video/mp4")));
        assert!(is_dash("https://a.com/x.mpd", None));
        assert_eq!(output_name("movie.m3u8"), "movie.ts");
        assert_eq!(output_name("movie.mp4"), "movie.mp4");
    }

    #[test]
    fn parses_media_playlist() {
        let t = "#EXTM3U\n#EXT-X-VERSION:3\n#EXTINF:9.5,\nseg1.ts\n#EXTINF:4.5,title\nseg2.ts\n#EXT-X-ENDLIST\n";
        match parse(t, BASE).unwrap() {
            Playlist::Media(m) => {
                assert_eq!(m.segments.len(), 2);
                assert_eq!(m.segments[1], "https://cdn.example.com/video/seg2.ts");
                assert!((m.duration - 14.0).abs() < 1e-9);
                assert!(m.init.is_none());
            }
            _ => panic!("expected a media playlist"),
        }
    }

    #[test]
    fn parses_master_and_picks_best() {
        let t = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360,CODECS=\"avc1.4d401e,mp4a.40.2\"\nlow.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=2400000,RESOLUTION=1280x720\nhigh.m3u8\n";
        match parse(t, BASE).unwrap() {
            Playlist::Master(v) => {
                assert_eq!(v.len(), 2);
                let b = best_variant(&v).unwrap();
                assert_eq!(b.bandwidth, 2_400_000);
                assert_eq!(b.url, "https://cdn.example.com/video/high.m3u8");
                assert_eq!(b.resolution.as_deref(), Some("1280x720"));
            }
            _ => panic!("expected a master playlist"),
        }
    }

    #[test]
    fn refuses_what_it_cannot_do() {
        let enc =
            "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"k.key\"\n#EXTINF:4,\ns.ts\n#EXT-X-ENDLIST\n";
        assert!(parse(enc, BASE)
            .unwrap_err()
            .to_string()
            .contains("encrypted"));
        let live = "#EXTM3U\n#EXTINF:4,\ns.ts\n";
        assert!(parse(live, BASE).unwrap_err().to_string().contains("live"));
        let br = "#EXTM3U\n#EXT-X-BYTERANGE:100@0\n#EXTINF:4,\ns.ts\n#EXT-X-ENDLIST\n";
        assert!(parse(br, BASE).is_err());
        assert!(parse("<html>", BASE).is_err());
        let none = "#EXTM3U\n#EXT-X-KEY:METHOD=NONE\n#EXTINF:4,\ns.ts\n#EXT-X-ENDLIST\n";
        assert!(parse(none, BASE).is_ok());
    }

    #[test]
    fn fmp4_init_segment() {
        let t = "#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:4,\ns1.m4s\n#EXT-X-ENDLIST\n";
        match parse(t, BASE).unwrap() {
            Playlist::Media(m) => assert_eq!(
                m.init.as_deref(),
                Some("https://cdn.example.com/video/init.mp4")
            ),
            _ => panic!(),
        }
    }
}
