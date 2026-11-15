use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::ptr::{null, null_mut};

use crate::url::NetError;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SecHandle {
    dw_lower: usize,
    dw_upper: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SecBuffer {
    cb_buffer: u32,
    buffer_type: u32,
    pv_buffer: *mut u8,
}

#[repr(C)]
struct SecBufferDesc {
    ul_version: u32,
    c_buffers: u32,
    p_buffers: *mut SecBuffer,
}

#[repr(C)]
struct SchannelCred {
    dw_version: u32,
    c_creds: u32,
    pa_cred: *const u8,
    h_root_store: isize,
    c_mappers: u32,
    aph_mappers: *const u8,
    c_supported_algs: u32,
    palg_supported_algs: *const u32,
    grbit_enabled_protocols: u32,
    dw_minimum_cipher_strength: u32,
    dw_maximum_cipher_strength: u32,
    dw_session_lifespan: u32,
    dw_flags: u32,
    dw_cred_format: u32,
}

#[repr(C)]
#[derive(Default)]
struct StreamSizes {
    cb_header: u32,
    cb_trailer: u32,
    cb_maximum_message: u32,
    c_buffers: u32,
    cb_block_size: u32,
}

const SCHANNEL_CRED_VERSION: u32 = 4;
const SCH_CRED_NO_DEFAULT_CREDS: u32 = 0x0000_0010;
const SCH_CRED_AUTO_CRED_VALIDATION: u32 = 0x0000_0020;
const SCH_USE_STRONG_CRYPTO: u32 = 0x0040_0000;

const SECPKG_CRED_OUTBOUND: u32 = 2;
const SECURITY_NATIVE_DREP: u32 = 0x10;
const SECPKG_ATTR_STREAM_SIZES: u32 = 4;

const ISC_REQ_REPLAY_DETECT: u32 = 0x0000_0004;
const ISC_REQ_SEQUENCE_DETECT: u32 = 0x0000_0008;
const ISC_REQ_CONFIDENTIALITY: u32 = 0x0000_0010;
const ISC_REQ_ALLOCATE_MEMORY: u32 = 0x0000_0100;
const ISC_REQ_EXTENDED_ERROR: u32 = 0x0000_4000;
const ISC_REQ_STREAM: u32 = 0x0000_8000;

const SECBUFFER_VERSION: u32 = 0;
const SECBUFFER_EMPTY: u32 = 0;
const SECBUFFER_DATA: u32 = 1;
const SECBUFFER_TOKEN: u32 = 2;
const SECBUFFER_EXTRA: u32 = 5;
const SECBUFFER_STREAM_TRAILER: u32 = 6;
const SECBUFFER_STREAM_HEADER: u32 = 7;

const SEC_E_OK: i32 = 0;
const SEC_I_CONTINUE_NEEDED: i32 = 0x0009_0312;
const SEC_I_CONTEXT_EXPIRED: i32 = 0x0009_0317;
const SEC_I_RENEGOTIATE: i32 = 0x0009_0321;
const SEC_E_INCOMPLETE_MESSAGE: i32 = -2146893032; // 0x80090318

#[link(name = "secur32")]
unsafe extern "system" {
    fn AcquireCredentialsHandleW(
        psz_principal: *const u16,
        psz_package: *const u16,
        f_credential_use: u32,
        pv_logon_id: *const u8,
        p_auth_data: *const SchannelCred,
        p_get_key_fn: *const u8,
        pv_get_key_argument: *const u8,
        ph_credential: *mut SecHandle,
        pts_expiry: *mut i64,
    ) -> i32;
    fn InitializeSecurityContextW(
        ph_credential: *const SecHandle,
        ph_context: *const SecHandle,
        psz_target_name: *const u16,
        f_context_req: u32,
        reserved1: u32,
        target_data_rep: u32,
        p_input: *mut SecBufferDesc,
        reserved2: u32,
        ph_new_context: *mut SecHandle,
        p_output: *mut SecBufferDesc,
        pf_context_attr: *mut u32,
        pts_expiry: *mut i64,
    ) -> i32;
    fn QueryContextAttributesW(
        ph_context: *const SecHandle,
        ul_attribute: u32,
        p_buffer: *mut u8,
    ) -> i32;
    fn EncryptMessage(
        ph_context: *const SecHandle,
        f_qop: u32,
        p_message: *mut SecBufferDesc,
        message_seq_no: u32,
    ) -> i32;
    fn DecryptMessage(
        ph_context: *const SecHandle,
        p_message: *mut SecBufferDesc,
        message_seq_no: u32,
        pf_qop: *mut u32,
    ) -> i32;
    fn FreeContextBuffer(pv_context_buffer: *mut u8) -> i32;
    fn DeleteSecurityContext(ph_context: *const SecHandle) -> i32;
    fn FreeCredentialsHandle(ph_credential: *const SecHandle) -> i32;
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// TLS stream over `TcpStream` powered by Windows SSPI Schannel.
pub struct TlsStream {
    tcp: TcpStream,
    cred: SecHandle,
    ctx: SecHandle,
    has_ctx: bool,
    sizes: StreamSizes,
    enc_buf: Vec<u8>,
    dec_buf: Vec<u8>,
    dec_pos: usize,
    eof: bool,
}

impl TlsStream {
    pub fn connect(tcp: TcpStream, server_name: &str) -> Result<Self, NetError> {
        let pkg = to_wide_null("Microsoft Unified Security Protocol Provider");
        let target = to_wide_null(server_name);

        let cred_cfg = SchannelCred {
            dw_version: SCHANNEL_CRED_VERSION,
            c_creds: 0,
            pa_cred: null(),
            h_root_store: 0,
            c_mappers: 0,
            aph_mappers: null(),
            c_supported_algs: 0,
            palg_supported_algs: null(),
            grbit_enabled_protocols: 0,
            dw_minimum_cipher_strength: 0,
            dw_maximum_cipher_strength: 0,
            dw_session_lifespan: 0,
            dw_flags: SCH_CRED_NO_DEFAULT_CREDS
                | SCH_CRED_AUTO_CRED_VALIDATION
                | SCH_USE_STRONG_CRYPTO,
            dw_cred_format: 0,
        };

        let mut cred = SecHandle::default();
        let mut expiry = 0i64;
        let status = unsafe {
            AcquireCredentialsHandleW(
                null(),
                pkg.as_ptr(),
                SECPKG_CRED_OUTBOUND,
                null(),
                &cred_cfg,
                null(),
                null(),
                &mut cred,
                &mut expiry,
            )
        };
        if status != SEC_E_OK {
            return Err(NetError::Tls(format!(
                "AcquireCredentialsHandleW failed: 0x{status:08x}"
            )));
        }

        let mut stream = Self {
            tcp,
            cred,
            ctx: SecHandle::default(),
            has_ctx: false,
            sizes: StreamSizes::default(),
            enc_buf: Vec::with_capacity(16384),
            dec_buf: Vec::new(),
            dec_pos: 0,
            eof: false,
        };
        stream.handshake(&target)?;
        Ok(stream)
    }

    fn handshake(&mut self, target: &[u16]) -> Result<(), NetError> {
        let req_flags = ISC_REQ_SEQUENCE_DETECT
            | ISC_REQ_REPLAY_DETECT
            | ISC_REQ_CONFIDENTIALITY
            | ISC_REQ_EXTENDED_ERROR
            | ISC_REQ_ALLOCATE_MEMORY
            | ISC_REQ_STREAM;

        let mut attrs = 0u32;
        let mut expiry = 0i64;

        // Initial ClientHello
        let mut out_buf = SecBuffer {
            cb_buffer: 0,
            buffer_type: SECBUFFER_TOKEN,
            pv_buffer: null_mut(),
        };
        let mut out_desc = SecBufferDesc {
            ul_version: SECBUFFER_VERSION,
            c_buffers: 1,
            p_buffers: &mut out_buf,
        };

        let mut status = unsafe {
            InitializeSecurityContextW(
                &self.cred,
                null(),
                target.as_ptr(),
                req_flags,
                0,
                SECURITY_NATIVE_DREP,
                null_mut(),
                0,
                &mut self.ctx,
                &mut out_desc,
                &mut attrs,
                &mut expiry,
            )
        };
        self.has_ctx = true;

        if status != SEC_I_CONTINUE_NEEDED && status != SEC_E_OK {
            return Err(NetError::Tls(format!(
                "initial TLS handshake failed: 0x{status:08x}"
            )));
        }

        if out_buf.cb_buffer > 0 && !out_buf.pv_buffer.is_null() {
            let slice =
                unsafe { std::slice::from_raw_parts(out_buf.pv_buffer, out_buf.cb_buffer as usize) };
            let write_res = self.tcp.write_all(slice);
            unsafe { FreeContextBuffer(out_buf.pv_buffer) };
            write_res?;
            self.tcp.flush()?;
        }

        let mut tmp = [0u8; 16384];
        while status == SEC_I_CONTINUE_NEEDED || status == SEC_E_INCOMPLETE_MESSAGE {
            if self.enc_buf.is_empty() || status == SEC_E_INCOMPLETE_MESSAGE {
                let n = self.tcp.read(&mut tmp)?;
                if n == 0 {
                    return Err(NetError::Tls("EOF during TLS handshake".into()));
                }
                self.enc_buf.extend_from_slice(&tmp[..n]);
            }

            let mut in_bufs = [
                SecBuffer {
                    cb_buffer: self.enc_buf.len() as u32,
                    buffer_type: SECBUFFER_TOKEN,
                    pv_buffer: self.enc_buf.as_mut_ptr(),
                },
                SecBuffer {
                    cb_buffer: 0,
                    buffer_type: SECBUFFER_EMPTY,
                    pv_buffer: null_mut(),
                },
            ];
            let mut in_desc = SecBufferDesc {
                ul_version: SECBUFFER_VERSION,
                c_buffers: 2,
                p_buffers: in_bufs.as_mut_ptr(),
            };

            out_buf = SecBuffer {
                cb_buffer: 0,
                buffer_type: SECBUFFER_TOKEN,
                pv_buffer: null_mut(),
            };
            out_desc = SecBufferDesc {
                ul_version: SECBUFFER_VERSION,
                c_buffers: 1,
                p_buffers: &mut out_buf,
            };

            status = unsafe {
                InitializeSecurityContextW(
                    &self.cred,
                    &self.ctx,
                    target.as_ptr(),
                    req_flags,
                    0,
                    SECURITY_NATIVE_DREP,
                    &mut in_desc,
                    0,
                    &mut self.ctx,
                    &mut out_desc,
                    &mut attrs,
                    &mut expiry,
                )
            };

            if status == SEC_E_INCOMPLETE_MESSAGE {
                continue;
            }

            if out_buf.cb_buffer > 0 && !out_buf.pv_buffer.is_null() {
                let slice = unsafe {
                    std::slice::from_raw_parts(out_buf.pv_buffer, out_buf.cb_buffer as usize)
                };
                let write_res = self.tcp.write_all(slice);
                unsafe { FreeContextBuffer(out_buf.pv_buffer) };
                write_res?;
                self.tcp.flush()?;
            }

            if in_bufs[1].buffer_type == SECBUFFER_EXTRA {
                let extra = in_bufs[1].cb_buffer as usize;
                let consumed = self.enc_buf.len() - extra;
                self.enc_buf.drain(..consumed);
            } else {
                self.enc_buf.clear();
            }

            if status != SEC_E_OK && status != SEC_I_CONTINUE_NEEDED {
                return Err(NetError::Tls(format!(
                    "TLS handshake failed: 0x{status:08x}"
                )));
            }
        }

        let q_status = unsafe {
            QueryContextAttributesW(
                &self.ctx,
                SECPKG_ATTR_STREAM_SIZES,
                (&mut self.sizes as *mut StreamSizes).cast(),
            )
        };
        if q_status != SEC_E_OK {
            return Err(NetError::Tls(format!(
                "QueryContextAttributesW failed: 0x{q_status:08x}"
            )));
        }
        Ok(())
    }

    fn read_more_decrypted(&mut self) -> io::Result<usize> {
        if self.eof {
            return Ok(0);
        }
        self.dec_buf.clear();
        self.dec_pos = 0;

        let mut tmp = [0u8; 16384];
        loop {
            if !self.enc_buf.is_empty() {
                let mut bufs = [
                    SecBuffer {
                        cb_buffer: self.enc_buf.len() as u32,
                        buffer_type: SECBUFFER_DATA,
                        pv_buffer: self.enc_buf.as_mut_ptr(),
                    },
                    SecBuffer {
                        cb_buffer: 0,
                        buffer_type: SECBUFFER_EMPTY,
                        pv_buffer: null_mut(),
                    },
                    SecBuffer {
                        cb_buffer: 0,
                        buffer_type: SECBUFFER_EMPTY,
                        pv_buffer: null_mut(),
                    },
                    SecBuffer {
                        cb_buffer: 0,
                        buffer_type: SECBUFFER_EMPTY,
                        pv_buffer: null_mut(),
                    },
                ];
                let mut desc = SecBufferDesc {
                    ul_version: SECBUFFER_VERSION,
                    c_buffers: 4,
                    p_buffers: bufs.as_mut_ptr(),
                };
                let mut qop = 0u32;
                let status = unsafe { DecryptMessage(&self.ctx, &mut desc, 0, &mut qop) };

                if status == SEC_E_OK || status == SEC_I_RENEGOTIATE {
                    let mut extra_len = 0usize;
                    for b in &bufs {
                        if b.buffer_type == SECBUFFER_DATA && b.cb_buffer > 0 {
                            let slice = unsafe {
                                std::slice::from_raw_parts(b.pv_buffer, b.cb_buffer as usize)
                            };
                            self.dec_buf.extend_from_slice(slice);
                        } else if b.buffer_type == SECBUFFER_EXTRA {
                            extra_len = b.cb_buffer as usize;
                        }
                    }
                    if extra_len > 0 {
                        let consumed = self.enc_buf.len() - extra_len;
                        self.enc_buf.drain(..consumed);
                    } else {
                        self.enc_buf.clear();
                    }
                    if !self.dec_buf.is_empty() {
                        return Ok(self.dec_buf.len());
                    }
                    // Empty TLS record (e.g., NewSessionTicket in TLS 1.3), continue reading.
                    if self.enc_buf.is_empty() {
                        let n = self.tcp.read(&mut tmp)?;
                        if n == 0 {
                            self.eof = true;
                            return Ok(0);
                        }
                        self.enc_buf.extend_from_slice(&tmp[..n]);
                    }
                    continue;
                } else if status == SEC_I_CONTEXT_EXPIRED {
                    self.eof = true;
                    return Ok(0);
                } else if status == SEC_E_INCOMPLETE_MESSAGE {
                    // Fall through to read more encrypted bytes
                } else {
                    return Err(io::Error::other(format!(
                        "DecryptMessage failed: 0x{status:08x}"
                    )));
                }
            }

            let n = self.tcp.read(&mut tmp)?;
            if n == 0 {
                self.eof = true;
                return Ok(0);
            }
            self.enc_buf.extend_from_slice(&tmp[..n]);
        }
    }
}

impl Read for TlsStream {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        if self.dec_pos >= self.dec_buf.len() && self.read_more_decrypted()? == 0 {
            return Ok(0);
        }
        let avail = &self.dec_buf[self.dec_pos..];
        let n = avail.len().min(out.len());
        out[..n].copy_from_slice(&avail[..n]);
        self.dec_pos += n;
        Ok(n)
    }
}

impl Write for TlsStream {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if data.is_empty() {
            return Ok(0);
        }
        let max_msg = self.sizes.cb_maximum_message.max(1) as usize;
        let chunk_len = data.len().min(max_msg);
        let hdr_len = self.sizes.cb_header as usize;
        let trl_len = self.sizes.cb_trailer as usize;

        let mut msg_buf = vec![0u8; hdr_len + chunk_len + trl_len];
        msg_buf[hdr_len..hdr_len + chunk_len].copy_from_slice(&data[..chunk_len]);

        let mut bufs = [
            SecBuffer {
                cb_buffer: self.sizes.cb_header,
                buffer_type: SECBUFFER_STREAM_HEADER,
                pv_buffer: msg_buf.as_mut_ptr(),
            },
            SecBuffer {
                cb_buffer: chunk_len as u32,
                buffer_type: SECBUFFER_DATA,
                pv_buffer: unsafe { msg_buf.as_mut_ptr().add(hdr_len) },
            },
            SecBuffer {
                cb_buffer: self.sizes.cb_trailer,
                buffer_type: SECBUFFER_STREAM_TRAILER,
                pv_buffer: unsafe { msg_buf.as_mut_ptr().add(hdr_len + chunk_len) },
            },
            SecBuffer {
                cb_buffer: 0,
                buffer_type: SECBUFFER_EMPTY,
                pv_buffer: null_mut(),
            },
        ];
        let mut desc = SecBufferDesc {
            ul_version: SECBUFFER_VERSION,
            c_buffers: 4,
            p_buffers: bufs.as_mut_ptr(),
        };

        let status = unsafe { EncryptMessage(&self.ctx, 0, &mut desc, 0) };
        if status != SEC_E_OK {
            return Err(io::Error::other(format!(
                "EncryptMessage failed: 0x{status:08x}"
            )));
        }

        let total = bufs[0].cb_buffer as usize
            + bufs[1].cb_buffer as usize
            + bufs[2].cb_buffer as usize;
        self.tcp.write_all(&msg_buf[..total])?;
        Ok(chunk_len)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.tcp.flush()
    }
}

impl Drop for TlsStream {
    fn drop(&mut self) {
        unsafe {
            if self.has_ctx {
                DeleteSecurityContext(&self.ctx);
            }
            FreeCredentialsHandle(&self.cred);
        }
    }
}
