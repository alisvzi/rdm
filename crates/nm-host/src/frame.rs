//! Native Messaging framing: every message is a 4-byte little-endian length followed by JSON.

use std::io::{self, Read, Write};

/// Browsers never send more than this to a host in practice; refuse anything bigger.
pub const MAX_IN: usize = 8 << 20;
/// Chrome refuses host answers above 1 MiB.
pub const MAX_OUT: usize = 1 << 20;

/// Reads one message. `Ok(None)` = the browser closed the pipe.
pub fn read_message<R: Read>(r: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let n = u32::from_le_bytes(len) as usize;
    if n > MAX_IN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too large",
        ));
    }
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf)?;
    Ok(Some(buf))
}

pub fn write_message<W: Write>(w: &mut W, data: &[u8]) -> io::Result<()> {
    if data.len() > MAX_OUT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "answer too large",
        ));
    }
    w.write_all(&(data.len() as u32).to_le_bytes())?;
    w.write_all(data)?;
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn roundtrip_two_messages() {
        let mut buf = Vec::new();
        write_message(&mut buf, br#"{"type":"ping"}"#).unwrap();
        write_message(&mut buf, b"{}").unwrap();
        let mut c = Cursor::new(buf);
        assert_eq!(
            read_message(&mut c).unwrap().unwrap(),
            br#"{"type":"ping"}"#
        );
        assert_eq!(read_message(&mut c).unwrap().unwrap(), b"{}");
        assert!(read_message(&mut c).unwrap().is_none());
    }

    #[test]
    fn length_is_little_endian() {
        let mut buf = Vec::new();
        write_message(&mut buf, &[b'x'; 258]).unwrap();
        assert_eq!(&buf[..4], &[2, 1, 0, 0]);
    }

    #[test]
    fn oversized_and_truncated_are_errors() {
        let mut c = Cursor::new(((MAX_IN + 1) as u32).to_le_bytes().to_vec());
        assert!(read_message(&mut c).is_err());
        let mut c = Cursor::new(vec![5, 0, 0, 0, b'a']);
        assert!(read_message(&mut c).is_err());
        assert!(write_message(&mut Vec::new(), &vec![0u8; MAX_OUT + 1]).is_err());
    }
}
