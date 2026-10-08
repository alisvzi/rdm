//! File integrity: checksum parsing (what the server or the user says the file hash is)
//! and SHA-256 / SHA-512 hashing of finished files.
//!
//! Everything above the "hashing" marker is pure string logic with unit tests.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algo {
    Sha256,
    Sha512,
}

impl Algo {
    /// Short name used in stored values, e.g. "sha256".
    pub fn label(self) -> &'static str {
        match self {
            Algo::Sha256 => "sha256",
            Algo::Sha512 => "sha512",
        }
    }

    pub fn hex_len(self) -> usize {
        match self {
            Algo::Sha256 => 64,
            Algo::Sha512 => 128,
        }
    }

    pub fn from_name(s: &str) -> Option<Algo> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sha-256" | "sha256" => Some(Algo::Sha256),
            "sha-512" | "sha512" => Some(Algo::Sha512),
            _ => None,
        }
    }

    fn from_hex_len(n: usize) -> Option<Algo> {
        match n {
            64 => Some(Algo::Sha256),
            128 => Some(Algo::Sha512),
            _ => None,
        }
    }

    fn from_byte_len(n: usize) -> Option<Algo> {
        match n {
            32 => Some(Algo::Sha256),
            64 => Some(Algo::Sha512),
            _ => None,
        }
    }
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Standard base64 (padding optional). Returns None on any invalid character.
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for c in s.trim().trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

/// What the user types or pastes: "sha256:ABC...", "SHA-256=abc...", or only the hex digits
/// (64 digits = SHA-256, 128 digits = SHA-512). Returns the algorithm and lower-case hex.
pub fn parse_expected(text: &str) -> Option<(Algo, String)> {
    let t = text.trim();
    // "name:hex" or "name=hex"
    if let Some(idx) = t.find(|c| c == ':' || c == '=') {
        let (name, rest) = (&t[..idx], t[idx + 1..].trim());
        let algo = Algo::from_name(name)?;
        if is_hex(rest) && rest.len() == algo.hex_len() {
            return Some((algo, rest.to_ascii_lowercase()));
        }
        return None;
    }
    // A checksum file line: "<hex>  filename" (sha256sum output) — take the first word.
    let first = t.split_whitespace().next()?;
    if is_hex(first) {
        let algo = Algo::from_hex_len(first.len())?;
        return Some((algo, first.to_ascii_lowercase()));
    }
    None
}

/// Parses an HTTP digest header value into (algorithm, lower-case hex).
/// Accepts RFC 9530 ("sha-256=:BASE64:, sha-512=:BASE64:") and the older RFC 3230 form
/// ("SHA-256=BASE64"). Unknown algorithms (md5, crc32c, ...) are ignored on purpose.
pub fn parse_digest_header(value: &str) -> Option<(Algo, String)> {
    let mut best: Option<(Algo, String)> = None;
    for part in value.split(',') {
        let Some((name, val)) = part.trim().split_once('=') else {
            continue;
        };
        let Some(algo) = Algo::from_name(name) else {
            continue;
        };
        let val = val.trim().trim_matches(':');
        let Some(bytes) = base64_decode(val) else {
            continue;
        };
        if Algo::from_byte_len(bytes.len()) != Some(algo) {
            continue;
        }
        let hex = to_hex(&bytes);
        // Prefer the stronger hash when a server sends both.
        if best.as_ref().map_or(true, |(a, _)| *a == Algo::Sha256) {
            best = Some((algo, hex));
        }
    }
    best
}

/// Stored form: "sha256:<hex>".
pub fn format_hash(algo: Algo, hex: &str) -> String {
    format!("{}:{}", algo.label(), hex)
}

/// Splits a stored "sha256:<hex>" value.
pub fn split_stored(s: &str) -> Option<(Algo, String)> {
    let (name, hex) = s.split_once(':')?;
    let algo = Algo::from_name(name)?;
    if is_hex(hex) && hex.len() == algo.hex_len() {
        Some((algo, hex.to_ascii_lowercase()))
    } else {
        None
    }
}

// ---- hashing ----

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256, Sha512};
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Hashes a file in 1 MiB steps. `done` receives the number of bytes read so far (for a
/// progress display); setting `stop` cancels the work. Blocking: run it in `spawn_blocking`.
pub fn hash_file(path: &Path, algo: Algo, stop: &AtomicBool, done: &AtomicU64) -> Result<String> {
    let mut f = std::fs::File::open(path)
        .with_context(|| format!("cannot open {} for checking", path.display()))?;
    let mut buf = vec![0u8; 1 << 20];
    let mut h256 = Sha256::new();
    let mut h512 = Sha512::new();
    loop {
        if stop.load(Ordering::Relaxed) {
            bail!("integrity check cancelled");
        }
        let n = f
            .read(&mut buf)
            .context("read error while checking the file")?;
        if n == 0 {
            break;
        }
        match algo {
            Algo::Sha256 => h256.update(&buf[..n]),
            Algo::Sha512 => h512.update(&buf[..n]),
        }
        done.fetch_add(n as u64, Ordering::Relaxed);
    }
    Ok(match algo {
        Algo::Sha256 => to_hex(&h256.finalize()),
        Algo::Sha512 => to_hex(&h512.finalize()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_basic() {
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64_decode("aGVsbG8").unwrap(), b"hello");
        assert!(base64_decode("a$b").is_none());
    }

    #[test]
    fn expected_forms() {
        let h = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(parse_expected(h).unwrap(), (Algo::Sha256, h.to_string()));
        assert_eq!(
            parse_expected(&format!("SHA-256={}", h.to_uppercase())).unwrap(),
            (Algo::Sha256, h.to_string())
        );
        assert_eq!(
            parse_expected(&format!("{h}  file.iso")).unwrap().1,
            h.to_string()
        );
        assert!(parse_expected("sha256:abc").is_none());
        assert!(parse_expected("hello").is_none());
        assert!(parse_expected("md5:d41d8cd98f00b204e9800998ecf8427e").is_none());
    }

    #[test]
    fn digest_header_rfc9530() {
        // sha-256 of the empty string
        let v = "sha-256=:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=:";
        let (a, hex) = parse_digest_header(v).unwrap();
        assert_eq!(a, Algo::Sha256);
        assert_eq!(
            hex,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        // legacy form, no colons
        assert!(
            parse_digest_header("SHA-256=47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=").is_some()
        );
        // md5 ignored, wrong length ignored
        assert!(parse_digest_header("md5=1B2M2Y8AsgTpgAmY7PhCfg==").is_none());
        assert!(parse_digest_header("sha-256=:aGVsbG8=:").is_none());
    }

    #[test]
    fn stored_roundtrip() {
        let h = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let s = format_hash(Algo::Sha256, h);
        assert_eq!(split_stored(&s).unwrap(), (Algo::Sha256, h.to_string()));
    }

    #[test]
    fn sha256_of_file() {
        let p = std::env::temp_dir().join("rdm_integrity_test.bin");
        std::fs::write(&p, b"abc").unwrap();
        let hex = hash_file(
            &p,
            Algo::Sha256,
            &AtomicBool::new(false),
            &AtomicU64::new(0),
        )
        .unwrap();
        let _ = std::fs::remove_file(&p);
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
