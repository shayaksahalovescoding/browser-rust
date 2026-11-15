use std::io;
use std::ptr::null_mut;

/// Magic signature `"CHRF"` stored at the start of every shared-memory framebuffer.
pub const FRAME_MAGIC: u32 = 0x4652_4843;
pub const FRAME_HEADER_BYTES: usize = std::mem::size_of::<FrameHeader>();
pub const MAX_SURFACE_DIM: u32 = 8192;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    pub magic: u32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub generation: u64,
}

const INVALID_HANDLE_VALUE: isize = -1;
const PAGE_READWRITE: u32 = 0x04;
const FILE_MAP_ALL_ACCESS: u32 = 0x000F_001F;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateFileMappingW(
        h_file: isize,
        lp_file_mapping_attributes: *const u8,
        fl_protect: u32,
        dw_maximum_size_high: u32,
        dw_maximum_size_low: u32,
        lp_name: *const u16,
    ) -> isize;
    fn OpenFileMappingW(
        dw_desired_access: u32,
        b_inherit_handle: i32,
        lp_name: *const u16,
    ) -> isize;
    fn MapViewOfFile(
        h_file_mapping_object: isize,
        dw_desired_access: u32,
        dw_file_offset_high: u32,
        dw_file_offset_low: u32,
        dw_number_of_bytes_to_map: usize,
    ) -> *mut u8;
    fn UnmapViewOfFile(lp_base_address: *const u8) -> i32;
    fn CloseHandle(h_object: isize) -> i32;
    fn GetLastError() -> u32;
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn calc_total_bytes(width: u32, height: u32) -> io::Result<usize> {
    if width == 0 || height == 0 || width > MAX_SURFACE_DIM || height > MAX_SURFACE_DIM {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid shared framebuffer dimensions: {width}x{height}"),
        ));
    }
    let pixel_bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|px| px.checked_mul(4))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "pixel buffer overflow"))?;
    FRAME_HEADER_BYTES
        .checked_add(pixel_bytes)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "total buffer overflow"))
}

/// Cross-process Windows file-mapping backed BGRA8 pixel surface.
pub struct SharedFrameBuffer {
    name: String,
    handle: isize,
    view: *mut u8,
    total_bytes: usize,
    width: u32,
    height: u32,
}

unsafe impl Send for SharedFrameBuffer {}

impl SharedFrameBuffer {
    /// Creates a named shared-memory framebuffer (owned by the Browser process).
    pub fn create(name: &str, width: u32, height: u32) -> io::Result<Self> {
        let total_bytes = calc_total_bytes(width, height)?;
        let wide_name = to_wide_null(name);
        let size_high = ((total_bytes as u64) >> 32) as u32;
        let size_low = (total_bytes as u64 & 0xFFFF_FFFF) as u32;

        let handle = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                null_mut(),
                PAGE_READWRITE,
                size_high,
                size_low,
                wide_name.as_ptr(),
            )
        };
        if handle == 0 {
            return Err(io::Error::from_raw_os_error(unsafe { GetLastError() } as i32));
        }

        let view = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, total_bytes) };
        if view.is_null() {
            let err = unsafe { GetLastError() } as i32;
            unsafe { CloseHandle(handle) };
            return Err(io::Error::from_raw_os_error(err));
        }

        let mut shm = Self {
            name: name.to_string(),
            handle,
            view,
            total_bytes,
            width,
            height,
        };
        shm.write_header(FrameHeader {
            magic: FRAME_MAGIC,
            width,
            height,
            stride: width * 4,
            generation: 0,
        });
        Ok(shm)
    }

    /// Opens an existing named shared-memory framebuffer (used by Renderer child processes).
    pub fn open(name: &str, width: u32, height: u32) -> io::Result<Self> {
        let total_bytes = calc_total_bytes(width, height)?;
        let wide_name = to_wide_null(name);
        let handle = unsafe { OpenFileMappingW(FILE_MAP_ALL_ACCESS, 0, wide_name.as_ptr()) };
        if handle == 0 {
            return Err(io::Error::from_raw_os_error(unsafe { GetLastError() } as i32));
        }
        let view = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, total_bytes) };
        if view.is_null() {
            let err = unsafe { GetLastError() } as i32;
            unsafe { CloseHandle(handle) };
            return Err(io::Error::from_raw_os_error(err));
        }
        let shm = Self {
            name: name.to_string(),
            handle,
            view,
            total_bytes,
            width,
            height,
        };
        let hdr = shm.header();
        if hdr.magic != FRAME_MAGIC || hdr.width != width || hdr.height != height {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "shared framebuffer header mismatch",
            ));
        }
        Ok(shm)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn header(&self) -> FrameHeader {
        unsafe { std::ptr::read_volatile(self.view as *const FrameHeader) }
    }

    pub fn write_header(&mut self, hdr: FrameHeader) {
        unsafe { std::ptr::write_volatile(self.view as *mut FrameHeader, hdr) }
    }

    pub fn pixels(&self) -> &[u8] {
        let len = self.total_bytes - FRAME_HEADER_BYTES;
        unsafe { std::slice::from_raw_parts(self.view.add(FRAME_HEADER_BYTES), len) }
    }

    pub fn pixels_mut(&mut self) -> &mut [u8] {
        let len = self.total_bytes - FRAME_HEADER_BYTES;
        unsafe { std::slice::from_raw_parts_mut(self.view.add(FRAME_HEADER_BYTES), len) }
    }

    /// Bumps the generation counter after writing pixels.
    pub fn commit_generation(&mut self) -> u64 {
        let mut hdr = self.header();
        hdr.generation = hdr.generation.wrapping_add(1);
        std::sync::atomic::fence(std::sync::atomic::Ordering::Release);
        self.write_header(hdr);
        hdr.generation
    }
}

impl Drop for SharedFrameBuffer {
    fn drop(&mut self) {
        unsafe {
            if !self.view.is_null() {
                UnmapViewOfFile(self.view);
            }
            if self.handle != 0 {
                CloseHandle(self.handle);
            }
        }
    }
}
