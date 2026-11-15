use std::io::{self, Read, Write};

use crate::framing::{read_frame, write_frame};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MouseButton {
    Left = 1,
    Right = 2,
    Middle = 3,
}

impl MouseButton {
    fn from_u8(v: u8) -> io::Result<Self> {
        match v {
            1 => Ok(Self::Left),
            2 => Ok(Self::Right),
            3 => Ok(Self::Middle),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown MouseButton tag: {v}"),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserToRenderer {
    Navigate {
        nav_id: u64,
        url: String,
    },
    Resize {
        width: u32,
        height: u32,
        shm_name: String,
    },
    MouseMove {
        x: i32,
        y: i32,
    },
    MouseDown {
        x: i32,
        y: i32,
        button: MouseButton,
    },
    KeyPress {
        ch: char,
    },
    Scroll {
        delta_y: i32,
    },
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RendererToBrowser {
    Ready {
        pid: u32,
    },
    FrameCommitted {
        nav_id: u64,
        shm_name: String,
        width: u32,
        height: u32,
        title: String,
    },
    LoadFailed {
        nav_id: u64,
        error: String,
    },
    RequestNavigate {
        url: String,
    },
    DevToolsInfo {
        nav_id: u64,
        network_summary: String,
        dom_dump: String,
        console_logs: String,
    },
    CursorChanged {
        cursor: u8,
        hover_url: Option<String>,
    },
}

struct Encoder(Vec<u8>);

impl Encoder {
    fn new(tag: u8) -> Self {
        Self(vec![tag])
    }
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn str(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.0.extend_from_slice(s.as_bytes());
    }
    fn finish(self) -> Vec<u8> {
        self.0
    }
}

struct Decoder<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Decoder<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        if self.pos + n > self.buf.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "truncated IPC message",
            ));
        }
        let slice = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }
    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> io::Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn str(&mut self) -> io::Result<String> {
        let len = self.u32()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
    fn expect_end(&self) -> io::Result<()> {
        if self.pos != self.buf.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "trailing bytes in IPC message",
            ));
        }
        Ok(())
    }
}

impl BrowserToRenderer {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Navigate { nav_id, url } => {
                let mut e = Encoder::new(1);
                e.u64(*nav_id);
                e.str(url);
                e.finish()
            }
            Self::Resize {
                width,
                height,
                shm_name,
            } => {
                let mut e = Encoder::new(2);
                e.u32(*width);
                e.u32(*height);
                e.str(shm_name);
                e.finish()
            }
            Self::MouseMove { x, y } => {
                let mut e = Encoder::new(3);
                e.i32(*x);
                e.i32(*y);
                e.finish()
            }
            Self::MouseDown { x, y, button } => {
                let mut e = Encoder::new(4);
                e.i32(*x);
                e.i32(*y);
                e.u8(*button as u8);
                e.finish()
            }
            Self::KeyPress { ch } => {
                let mut e = Encoder::new(5);
                e.u32(*ch as u32);
                e.finish()
            }
            Self::Scroll { delta_y } => {
                let mut e = Encoder::new(7);
                e.i32(*delta_y);
                e.finish()
            }
            Self::Shutdown => Encoder::new(6).finish(),
        }
    }

    pub fn decode(buf: &[u8]) -> io::Result<Self> {
        let mut d = Decoder::new(buf);
        let msg = match d.u8()? {
            1 => Self::Navigate {
                nav_id: d.u64()?,
                url: d.str()?,
            },
            2 => Self::Resize {
                width: d.u32()?,
                height: d.u32()?,
                shm_name: d.str()?,
            },
            3 => Self::MouseMove {
                x: d.i32()?,
                y: d.i32()?,
            },
            4 => Self::MouseDown {
                x: d.i32()?,
                y: d.i32()?,
                button: MouseButton::from_u8(d.u8()?)?,
            },
            5 => {
                let code = d.u32()?;
                let ch = char::from_u32(code).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid Unicode scalar")
                })?;
                Self::KeyPress { ch }
            }
            6 => Self::Shutdown,
            7 => Self::Scroll { delta_y: d.i32()? },
            tag => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unknown BrowserToRenderer tag: {tag}"),
                ));
            }
        };
        d.expect_end()?;
        Ok(msg)
    }

    pub fn write_to<W: Write>(&self, w: &mut W) -> io::Result<()> {
        write_frame(w, &self.encode())
    }

    pub fn read_from<R: Read>(r: &mut R) -> io::Result<Option<Self>> {
        match read_frame(r)? {
            Some(buf) => Self::decode(&buf).map(Some),
            None => Ok(None),
        }
    }
}

impl RendererToBrowser {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Ready { pid } => {
                let mut e = Encoder::new(1);
                e.u32(*pid);
                e.finish()
            }
            Self::FrameCommitted {
                nav_id,
                shm_name,
                width,
                height,
                title,
            } => {
                let mut e = Encoder::new(2);
                e.u64(*nav_id);
                e.str(shm_name);
                e.u32(*width);
                e.u32(*height);
                e.str(title);
                e.finish()
            }
            Self::LoadFailed { nav_id, error } => {
                let mut e = Encoder::new(3);
                e.u64(*nav_id);
                e.str(error);
                e.finish()
            }
            Self::RequestNavigate { url } => {
                let mut e = Encoder::new(4);
                e.str(url);
                e.finish()
            }
            Self::DevToolsInfo {
                nav_id,
                network_summary,
                dom_dump,
                console_logs,
            } => {
                let mut e = Encoder::new(5);
                e.u64(*nav_id);
                e.str(network_summary);
                e.str(dom_dump);
                e.str(console_logs);
                e.finish()
            }
            Self::CursorChanged { cursor, hover_url } => {
                let mut e = Encoder::new(6);
                e.u8(*cursor);
                match hover_url {
                    Some(u) => {
                        e.u8(1);
                        e.str(u);
                    }
                    None => e.u8(0),
                }
                e.finish()
            }
        }
    }

    pub fn decode(buf: &[u8]) -> io::Result<Self> {
        let mut d = Decoder::new(buf);
        let msg = match d.u8()? {
            1 => Self::Ready { pid: d.u32()? },
            2 => Self::FrameCommitted {
                nav_id: d.u64()?,
                shm_name: d.str()?,
                width: d.u32()?,
                height: d.u32()?,
                title: d.str()?,
            },
            3 => Self::LoadFailed {
                nav_id: d.u64()?,
                error: d.str()?,
            },
            4 => Self::RequestNavigate { url: d.str()? },
            5 => Self::DevToolsInfo {
                nav_id: d.u64()?,
                network_summary: d.str()?,
                dom_dump: d.str()?,
                console_logs: d.str()?,
            },
            6 => {
                let cursor = d.u8()?;
                let hover_url = if d.u8()? != 0 { Some(d.str()?) } else { None };
                Self::CursorChanged { cursor, hover_url }
            }
            tag => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unknown RendererToBrowser tag: {tag}"),
                ));
            }
        };
        d.expect_end()?;
        Ok(msg)
    }

    pub fn write_to<W: Write>(&self, w: &mut W) -> io::Result<()> {
        write_frame(w, &self.encode())
    }

    pub fn read_from<R: Read>(r: &mut R) -> io::Result<Option<Self>> {
        match read_frame(r)? {
            Some(buf) => Self::decode(&buf).map(Some),
            None => Ok(None),
        }
    }
}
