use anyhow::{bail, Context, Result};
use percent_encoding::percent_decode_str;
use reqwest::{header, Client, StatusCode, Url};
use std::time::Duration;

pub const DEFAULT_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

#[derive(Clone)]
pub struct Probe {
    /// Final URL after redirects (saves a redirect round-trip per chunk).
    pub url: Url,
    pub total: Option<u64>,
    /// True if the server honours Range requests and the size is known.
    pub ranges: bool,
    pub filename: String,
    /// ETag or Last-Modified, used to make sure a resumed file is still the same.
    pub validator: Option<String>,
    /// HTTP status of the probe answer (206 or 200).
    pub http_status: u16,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub accept_ranges: Option<String>,
    pub server: Option<String>,
    /// Whole-file checksum announced by the server, stored as "sha256:<hex>".
    pub digest: Option<String>,
}

/// How requests reach the internet.
#[derive(Clone, Debug, Default)]
pub struct ProxyConfig {
    /// "system" (default) | "none" | "manual"
    pub mode: String,
    /// http://host:port, https://host:port, socks5://host:port or socks5h://host:port
    pub url: String,
    pub user: String,
    pub password: String,
    /// Hosts that skip the proxy, comma separated ("localhost, *.example.com, 10.0.0.0/8").
    pub bypass: String,
    /// Let the server negotiate HTTP/2 (otherwise HTTP/1.1 only, best for many parallel connections).
    pub http_auto: bool,
}

/// Checks a proxy address typed by the user.
pub fn validate_proxy_url(url: &str) -> Result<()> {
    let u = Url::parse(url.trim()).map_err(|e| anyhow::anyhow!("invalid proxy address: {e}"))?;
    if !matches!(u.scheme(), "http" | "https" | "socks5" | "socks5h") {
        bail!("proxy address must start with http://, https://, socks5:// or socks5h://");
    }
    if u.host_str().map_or(true, |h| h.is_empty()) {
        bail!("proxy address has no host name");
    }
    Ok(())
}

pub fn build_client(user_agent: &str) -> Result<Client> {
    build_client_with(user_agent, &ProxyConfig::default())
}

/// HTTP/1.1 only: HTTP/2 would multiplex every request over ONE TCP connection,
/// which defeats the purpose of opening many parallel connections.
pub fn build_client_with(user_agent: &str, proxy: &ProxyConfig) -> Result<Client> {
    let mut b = Client::builder()
        .user_agent(user_agent)
        .tcp_nodelay(true)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(64);
    b = if proxy.http_auto {
        b.http2_adaptive_window(true)
    } else {
        b.http1_only()
    };
    match proxy.mode.as_str() {
        "none" => b = b.no_proxy(),
        "manual" => {
            validate_proxy_url(&proxy.url)?;
            let mut p = reqwest::Proxy::all(proxy.url.trim())?;
            if !proxy.user.is_empty() {
                p = p.basic_auth(&proxy.user, &proxy.password);
            }
            if !proxy.bypass.trim().is_empty() {
                p = p.no_proxy(reqwest::NoProxy::from_string(proxy.bypass.trim()));
            }
            b = b.proxy(p);
        }
        _ => {} // "system": reqwest reads the Windows/environment proxy by itself
    }
    Ok(b.build()?)
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

    let text = |name: header::HeaderName| {
        h.get(name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    };
    // Repr-Digest (RFC 9530) describes the whole file. Content-Digest only describes this
    // response body, so it is usable only when the answer was a complete 200 response.
    let name_repr = header::HeaderName::from_static("repr-digest");
    let name_content = header::HeaderName::from_static("content-digest");
    let digest = text(name_repr)
        .or_else(|| {
            if status == StatusCode::OK {
                text(name_content)
            } else {
                None
            }
        })
        .and_then(|v| crate::integrity::parse_digest_header(&v))
        .map(|(a, hex)| crate::integrity::format_hash(a, &hex));
    let etag = text(header::ETAG);
    let last_modified = text(header::LAST_MODIFIED);
    let content_type = text(header::CONTENT_TYPE);
    let accept_ranges = text(header::ACCEPT_RANGES);
    let server = text(header::SERVER);

    Ok(Probe {
        url: final_url,
        total,
        ranges,
        filename,
        validator,
        http_status: status.as_u16(),
        content_type,
        etag,
        last_modified,
        accept_ranges,
        server,
        digest,
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
