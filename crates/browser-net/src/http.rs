use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use crate::deflate::{decompress_gzip, decompress_zlib};
use crate::tls::TlsStream;
use crate::url::{NetError, Url};

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub final_url: String,
    pub from_cache: bool,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn body_text(&self) -> String {
        if let Some(ct) = self.header("content-type") {
            let lower = ct.to_ascii_lowercase();
            if lower.contains("iso-8859-1")
                || lower.contains("latin1")
                || lower.contains("windows-1252")
                || lower.contains("cp1252")
            {
                return decode_windows_1252(&self.body);
            }
        }
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

fn decode_windows_1252(bytes: &[u8]) -> String {
    const C1_MAP: [char; 32] = [
        '\u{20AC}', '\u{FFFD}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{FFFD}',
        '\u{017D}', '\u{FFFD}', '\u{FFFD}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}',
        '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}',
        '\u{0153}', '\u{FFFD}', '\u{017E}', '\u{0178}',
    ];
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        match b {
            0x00..=0x7F => out.push(b as char),
            0x80..=0x9F => out.push(C1_MAP[(b - 0x80) as usize]),
            0xA0..=0xFF => out.push(char::from(b)),
        }
    }
    out
}

#[derive(Default)]
pub struct CookieJar {
    cookies: HashMap<String, Vec<(String, String)>>,
}

impl CookieJar {
    pub fn store_from_headers(&mut self, host: &str, headers: &[(String, String)]) {
        for (k, v) in headers {
            if k.eq_ignore_ascii_case("set-cookie") {
                if let Some((nv, _)) = v.split_once(';').or(Some((v.as_str(), ""))) {
                    if let Some((name, val)) = nv.split_once('=') {
                        let entry = self.cookies.entry(host.to_ascii_lowercase()).or_default();
                        let name = name.trim().to_string();
                        let val = val.trim().to_string();
                        if let Some(existing) = entry.iter_mut().find(|(n, _)| *n == name) {
                            existing.1 = val;
                        } else {
                            entry.push((name, val));
                        }
                    }
                }
            }
        }
    }

    pub fn cookie_header(&self, host: &str) -> Option<String> {
        let list = self.cookies.get(&host.to_ascii_lowercase())?;
        if list.is_empty() {
            return None;
        }
        Some(
            list.iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }
}

#[derive(Default)]
pub struct ResponseCache {
    entries: HashMap<String, HttpResponse>,
}

impl ResponseCache {
    pub fn get(&self, url: &str) -> Option<HttpResponse> {
        self.entries.get(url).cloned().map(|mut r| {
            r.from_cache = true;
            r
        })
    }

    pub fn insert(&mut self, url: String, resp: HttpResponse) {
        if resp.status == 200 && self.entries.len() < 128 {
            self.entries.insert(url, resp);
        }
    }
}

pub struct HttpClient {
    pub cookies: CookieJar,
    pub cache: ResponseCache,
    pub timeout: Duration,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    pub fn new() -> Self {
        Self {
            cookies: CookieJar::default(),
            cache: ResponseCache::default(),
            timeout: Duration::from_secs(12),
        }
    }

    pub fn get(&mut self, url_str: &str) -> Result<HttpResponse, NetError> {
        self.get_with_options(url_str, false)
    }

    pub fn get_with_options(
        &mut self,
        url_str: &str,
        bypass_cache: bool,
    ) -> Result<HttpResponse, NetError> {
        let mut current = Url::parse(url_str)?;
        for _ in 0..8 {
            let key = current.as_str();
            if !bypass_cache {
                if let Some(cached) = self.cache.get(&key) {
                    return Ok(cached);
                }
            }

            let resp = self.fetch_single(&current)?;
            self.cookies.store_from_headers(&current.host, &resp.headers);

            if matches!(resp.status, 301 | 302 | 303 | 307 | 308) {
                if let Some(loc) = resp.header("location") {
                    current = current.resolve(loc)?;
                    continue;
                }
            }

            self.cache.insert(key, resp.clone());
            return Ok(resp);
        }
        Err(NetError::TooManyRedirects)
    }

    fn fetch_single(&self, url: &Url) -> Result<HttpResponse, NetError> {
        let addr = format!("{}:{}", url.host, url.port);
        let tcp = TcpStream::connect(&addr)?;
        let _ = tcp.set_read_timeout(Some(self.timeout));
        let _ = tcp.set_write_timeout(Some(self.timeout));

        let mut req = format!(
            "GET {} HTTP/1.1\r\n\
             Host: {}\r\n\
             User-Agent: ChromeScratch/0.1 (Windows NT 10.0; Win64; x64)\r\n\
             Accept: text/html,application/xhtml+xml,text/css,*/*;q=0.8\r\n\
             Accept-Encoding: gzip, deflate\r\n\
             Connection: close\r\n",
            url.path_and_query,
            url.host_header()
        );
        if let Some(cookie) = self.cookies.cookie_header(&url.host) {
            req.push_str(&format!("Cookie: {cookie}\r\n"));
        }
        req.push_str("\r\n");

        let raw = if url.scheme == "https" {
            let mut tls = TlsStream::connect(tcp, &url.host)?;
            tls.write_all(req.as_bytes())?;
            tls.flush()?;
            read_all_until_eof(&mut tls)?
        } else {
            let mut tcp = tcp;
            tcp.write_all(req.as_bytes())?;
            tcp.flush()?;
            read_all_until_eof(&mut tcp)?
        };

        parse_http_response(&raw, url.as_str())
    }
}

fn read_all_until_eof<R: Read>(r: &mut R) -> Result<Vec<u8>, NetError> {
    let mut out = Vec::new();
    let mut buf = [0u8; 16384];
    loop {
        match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => out.extend_from_slice(&buf[..n]),
            Err(e)
                if e.kind() == std::io::ErrorKind::ConnectionReset
                    || e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                if !out.is_empty() {
                    break;
                }
                return Err(e.into());
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(out)
}

pub fn parse_http_response(raw: &[u8], final_url: String) -> Result<HttpResponse, NetError> {
    let header_end = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| NetError::Http("missing HTTP header terminator".into()))?;

    let head_str = String::from_utf8_lossy(&raw[..header_end]);
    let mut lines = head_str.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| NetError::Http("empty HTTP response".into()))?;
    let mut parts = status_line.splitn(3, ' ');
    let _proto = parts
        .next()
        .ok_or_else(|| NetError::Http("missing HTTP version".into()))?;
    let status: u16 = parts
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| NetError::Http(format!("bad status line: {status_line}")))?;
    let reason = parts.next().unwrap_or("").to_string();

    let mut headers = Vec::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }

    let raw_body = &raw[header_end + 4..];
    let is_chunked = headers.iter().any(|(k, v)| {
        k.eq_ignore_ascii_case("transfer-encoding")
            && v.to_ascii_lowercase().contains("chunked")
    });
    let mut body = if is_chunked {
        decode_chunked(raw_body)?
    } else if let Some(len) = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse::<usize>().ok())
    {
        raw_body[..len.min(raw_body.len())].to_vec()
    } else {
        raw_body.to_vec()
    };

    if let Some((_, enc)) = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-encoding"))
    {
        let enc_l = enc.to_ascii_lowercase();
        if enc_l.contains("gzip") {
            body = decompress_gzip(&body)?;
        } else if enc_l.contains("deflate") {
            body = decompress_zlib(&body)?;
        }
    }

    Ok(HttpResponse {
        status,
        reason,
        headers,
        body,
        final_url,
        from_cache: false,
    })
}

pub fn decode_chunked(mut data: &[u8]) -> Result<Vec<u8>, NetError> {
    let mut out = Vec::new();
    loop {
        let crlf = data
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or_else(|| NetError::Http("malformed chunked size".into()))?;
        let size_line = std::str::from_utf8(&data[..crlf])
            .map_err(|_| NetError::Http("non-UTF8 chunk size".into()))?;
        let hex_part = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(hex_part, 16)
            .map_err(|_| NetError::Http(format!("invalid chunk hex: {hex_part}")))?;
        data = &data[crlf + 2..];
        if size == 0 {
            break;
        }
        if data.len() < size + 2 {
            return Err(NetError::Http("truncated chunk body".into()));
        }
        out.extend_from_slice(&data[..size]);
        data = &data[size + 2..];
    }
    Ok(out)
}
