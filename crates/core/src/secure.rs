//! Protects secrets (cookies, tokens) before they are written to disk.
//!
//! On Windows the data is encrypted with DPAPI (`CryptProtectData`), so only the same
//! Windows user on the same computer can read it back. On other systems nothing can be
//! protected, so `protect` returns None and the caller must not write the secret to disk.

use crate::integrity::base64_decode;

const PREFIX: &str = "dpapi1:";

fn base64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Encrypts `text` for the current Windows user. None = could not be protected.
pub fn protect(text: &str) -> Option<String> {
    let blob = os::protect(text.as_bytes())?;
    Some(format!("{PREFIX}{}", base64_encode(&blob)))
}

/// Reverses `protect`. None = wrong format, other user/computer, or damaged data.
pub fn unprotect(stored: &str) -> Option<String> {
    let b64 = stored.strip_prefix(PREFIX)?;
    let blob = base64_decode(b64)?;
    String::from_utf8(os::unprotect(&blob)?).ok()
}

#[cfg(windows)]
mod os {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
    };

    const UI_FORBIDDEN: u32 = 0x1;

    fn run(data: &[u8], encrypt: bool) -> Option<Vec<u8>> {
        let input = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: null_mut(),
        };
        // SAFETY: `input` points to live memory for the whole call; Windows allocates
        // `output`, which we copy and free with LocalFree.
        unsafe {
            let ok = if encrypt {
                CryptProtectData(
                    &input,
                    null(),
                    null(),
                    null(),
                    null(),
                    UI_FORBIDDEN,
                    &mut output,
                )
            } else {
                CryptUnprotectData(
                    &input,
                    null_mut(),
                    null(),
                    null(),
                    null(),
                    UI_FORBIDDEN,
                    &mut output,
                )
            };
            if ok == 0 || output.pbData.is_null() {
                return None;
            }
            let v = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
            let _ = LocalFree(output.pbData as _);
            Some(v)
        }
    }

    pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
        run(data, true)
    }
    pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
        run(data, false)
    }
}

#[cfg(not(windows))]
mod os {
    pub fn protect(_: &[u8]) -> Option<Vec<u8>> {
        None
    }
    pub fn unprotect(_: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip() {
        for s in ["", "a", "ab", "abc", "Cookie: a=b; c=d"] {
            assert_eq!(
                base64_decode(&base64_encode(s.as_bytes())).unwrap(),
                s.as_bytes()
            );
        }
    }

    #[test]
    fn bad_input_is_rejected() {
        assert!(unprotect("plain text").is_none());
        assert!(unprotect("dpapi1:!!!").is_none());
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_roundtrip() {
        let p = protect("Cookie: session=secret").unwrap();
        assert!(!p.contains("secret"));
        assert_eq!(unprotect(&p).unwrap(), "Cookie: session=secret");
    }
}
