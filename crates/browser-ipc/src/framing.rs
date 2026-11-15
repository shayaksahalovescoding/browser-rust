use std::io::{self, Read, Write};

/// Upper bound for a single IPC control frame (16 MiB) to prevent OOM on corrupt lengths.
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Writes a length-prefixed frame (`u32` little-endian byte length followed by payload).
pub fn write_frame<W: Write>(writer: &mut W, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("frame payload {} exceeds MAX_FRAME_BYTES", payload.len()),
        ));
    }
    let len_bytes = (payload.len() as u32).to_le_bytes();
    writer.write_all(&len_bytes)?;
    writer.write_all(payload)?;
    writer.flush()
}

/// Reads a length-prefixed frame. Returns `Ok(None)` on clean EOF at a frame boundary.
pub fn read_frame<R: Read>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid IPC frame length: {len}"),
        ));
    }
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload)?;
    Ok(Some(payload))
}
