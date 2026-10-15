//! Bounded newline-delimited frame reader.
use crate::{ErrorCode, ProtocolError, MAX_FRAME_BYTES, MAX_FRAME_PAYLOAD_BYTES};
use std::fmt;
use std::io::{self, BufRead, ErrorKind};

#[derive(Debug)]
pub enum FrameError {
    Io(io::Error),
    /// The frame exceeded `MAX_FRAME_BYTES`; it was drained to its newline (or EOF).
    TooLarge,
    Nul,
    InvalidUtf8,
    /// Empty or whitespace-only frame.
    Blank,
    /// EOF after a partial frame with no trailing newline; the partial bytes are dropped.
    MissingNewline,
}
impl FrameError {
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Io(_) => ErrorCode::Internal,
            _ => ErrorCode::InvalidRequest,
        }
    }
}
impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "frame read failed: {e}"),
            Self::TooLarge => write!(f, "frame exceeds {MAX_FRAME_BYTES} bytes"),
            Self::Nul => f.write_str("frame contains a NUL byte"),
            Self::InvalidUtf8 => f.write_str("frame is not valid UTF-8"),
            Self::Blank => f.write_str("frame is empty"),
            Self::MissingNewline => f.write_str("frame has no trailing newline"),
        }
    }
}
impl std::error::Error for FrameError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}
impl From<FrameError> for ProtocolError {
    fn from(e: FrameError) -> Self {
        ProtocolError::new(e.code(), e.to_string())
    }
}

/// Reads one frame and returns it without its trailing newline.
///
/// `Ok(None)` means clean EOF at a frame boundary. At most `MAX_FRAME_BYTES` bytes are
/// held for one frame. Every non-IO error leaves the reader at the next frame boundary.
pub fn read_frame<R: BufRead + ?Sized>(reader: &mut R) -> Result<Option<String>, FrameError> {
    let mut buf: Vec<u8> = Vec::new();
    let mut oversize = false;
    let mut seen_any = false;
    loop {
        let chunk = match reader.fill_buf() {
            Ok(c) => c,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(FrameError::Io(e)),
        };
        if chunk.is_empty() {
            return match (seen_any, oversize) {
                (false, _) => Ok(None),
                (true, true) => Err(FrameError::TooLarge),
                (true, false) => Err(FrameError::MissingNewline),
            };
        }
        seen_any = true;
        let (take, done) = match chunk.iter().position(|&b| b == b'\n') {
            Some(i) => (i, true),
            None => (chunk.len(), false),
        };
        if !oversize {
            if buf.len() + take > MAX_FRAME_PAYLOAD_BYTES {
                oversize = true;
                buf = Vec::new();
            } else {
                buf.reserve_exact(take);
                buf.extend_from_slice(&chunk[..take]);
            }
        }
        reader.consume(if done { take + 1 } else { take });
        if done {
            break;
        }
    }
    if oversize {
        return Err(FrameError::TooLarge);
    }
    if buf.contains(&0) {
        return Err(FrameError::Nul);
    }
    let text = String::from_utf8(buf).map_err(|_| FrameError::InvalidUtf8)?;
    if text.trim().is_empty() {
        return Err(FrameError::Blank);
    }
    Ok(Some(text))
}

/// Iterator-free convenience wrapper around `read_frame`.
#[derive(Debug)]
pub struct FrameReader<R> {
    inner: R,
}
impl<R: BufRead> FrameReader<R> {
    pub fn new(inner: R) -> Self {
        Self { inner }
    }
    pub fn read_frame(&mut self) -> Result<Option<String>, FrameError> {
        read_frame(&mut self.inner)
    }
    pub fn into_inner(self) -> R {
        self.inner
    }
}
