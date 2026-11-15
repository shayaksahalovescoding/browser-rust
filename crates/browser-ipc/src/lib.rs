//! Cross-process IPC framing, typed protocol messages, and Windows shared-memory framebuffers.

pub mod framing;
pub mod protocol;
pub mod shm;

pub use framing::{MAX_FRAME_BYTES, read_frame, write_frame};
pub use protocol::{BrowserToRenderer, MouseButton, RendererToBrowser};
pub use shm::{FRAME_HEADER_BYTES, FRAME_MAGIC, FrameHeader, SharedFrameBuffer};
