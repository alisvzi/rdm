//! Small file-system helpers: free disk space and a rename that survives short locks.

use std::path::Path;
use std::time::Duration;

/// Free bytes available to the current user on the drive that holds `path`
/// (the nearest existing parent folder is used). None = unknown.
pub fn free_space(path: &Path) -> Option<u64> {
    let mut dir = path;
    while !dir.exists() {
        dir = dir.parent()?;
    }
    os_free_space(dir)
}

#[cfg(windows)]
fn os_free_space(dir: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut avail: u64 = 0;
    let mut total: u64 = 0;
    let mut free: u64 = 0;
    // SAFETY: `wide` is NUL-terminated and all out-pointers are valid for the call.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) };
    if ok == 0 {
        None
    } else {
        Some(avail)
    }
}

#[cfg(not(windows))]
fn os_free_space(_: &Path) -> Option<u64> {
    None
}

/// Renames `from` to `to`, retrying for a few seconds when the file is briefly locked
/// (antivirus or indexer scanning it). Never replaces an existing file.
pub fn safe_rename(from: &Path, to: &Path) -> std::io::Result<()> {
    if to.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "a file with this name already exists",
        ));
    }
    let mut last = None;
    for _ in 0..10 {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(500));
            }
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_never_overwrites() {
        let d = std::env::temp_dir().join("rdm_fsutil_test");
        let _ = std::fs::create_dir_all(&d);
        let (a, b) = (d.join("a.part"), d.join("a"));
        std::fs::write(&a, b"x").unwrap();
        std::fs::write(&b, b"old").unwrap();
        assert!(safe_rename(&a, &b).is_err());
        assert_eq!(std::fs::read(&b).unwrap(), b"old");
        let _ = std::fs::remove_file(&b);
        assert!(safe_rename(&a, &b).is_ok());
        let _ = std::fs::remove_dir_all(&d);
    }
}
