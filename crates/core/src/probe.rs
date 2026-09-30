use anyhow::{bail, Context, Result};
use percent_encoding::percent_decode_str;
use reqwest::{header, Client, StatusCode, Url};
use std::time::Duration;

pub const DEFAULT_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

pub struct Probe {
    /// Final URL after redirects (saves a redirect round-trip per chunk).
    pub url: Url,
    pub total: Option<u64>,
    /// True if the server honours Range requests and the size is known.
    pub ranges: bool,
    pub filename: String,
    /// ETag or Last-Modified, used to make sure a resumed file is still the same.
    pub validator: Option<String>,
}

/// HTTP/1.1 only: HTTP/2 would multiplex every request over ONE TCP connection,
/// which defeats the purpose of opening many parallel connections.
pub fn build_client(user_agent: &str) -> Result<Client> {
    Ok(Client::builder()
        .user_agent(user_agent)
        .http1_only()
        .tcp_nodelay(true)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(64)
        .build()?)
}

/// Asks the server for the first byte only. This works on servers that reject HEAD
/// and tells us at once whether ranges are supported and what the total size is.
pub async fn probe(client: &Client, url: &str, headers: &header::HeaderMap) -> Result<Probe> {
    let resp = client
        .get(url)
        .headers(headers.clone())
        .header(header::RANGE, "bytes=0-0")
        .send()
        .await
        .context("could not connect to the server")?;

    let status = resp.status();
    if !status.is_success() {
        bail!("server responded with HTTP {}", status);
    }

    let final_url = resp.url().clone();
    let h = resp.headers();

    let (total, ranges) = if status == StatusCode::PARTIAL_CONTENT {
        let t = h
            .get(header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.rsplit('/').next())
            .and_then(|s| s.trim().parse::<u64>().ok());
        (t, t.map_or(false, |v| v > 0))
    } else {
        (resp.content_length(), false)
    };

    let validator = h
        .get(header::ETAG)
        .or_else(|| h.get(header::LAST_MODIFIED))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let filename = h
        .get(header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .and_then(filename_from_disposition)
        .or_else(|| filename_from_url(&final_url))
        .map(|n| sanitize_filename(&n))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "download.bin".to_string());

    Ok(Probe {
        url: final_url,
        total,
        ranges,
        filename,
        validator,
    })
}

fn filename_from_disposition(v: &str) -> Option<String> {
    // RFC 5987: filename*=UTF-8''%D8%B3.zip
    for part in v.split(';') {
        let part = part.trim();
        if part.len() > 10 && part[..10].eq_ignore_ascii_case("filename*=") {
            let rest = part[10..].trim_matches('"');
            if let Some(idx) = rest.find("''") {
                let enc = &rest[idx + 2..];
                return Some(percent_decode_str(enc).decode_utf8_lossy().into_owned());
            }
        }
    }
    for part in v.split(';') {
        let part = part.trim();
        if part.len() > 9 && part[..9].eq_ignore_ascii_case("filename=") {
            return Some(part[9..].trim_matches('"').to_string());
        }
    }
    None
}

fn filename_from_url(url: &Url) -> Option<String> {
    let last = url.path_segments()?.filter(|s| !s.is_empty()).last()?;
    Some(percent_decode_str(last).decode_utf8_lossy().into_owned())
}

/// Removes path parts and characters Windows does not allow in file names.
pub fn sanitize_filename(name: &str) -> String {
    let base = name
        .rsplit(|c: char| c == '/' || c == '\\')
        .next()
        .unwrap_or(name);
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    cleaned.trim().trim_end_matches('.').to_string()
}
