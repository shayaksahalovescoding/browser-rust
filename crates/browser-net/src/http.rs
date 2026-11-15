use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
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
        let host_l = host.to_ascii_lowercase();
        let mut pairs = Vec::new();
        for (cookie_host, list) in &self.cookies {
            let domain = cookie_host.trim_start_matches('.');
            if host_l == domain || host_l.ends_with(&format!(".{domain}")) {
                for (k, v) in list {
                    pairs.push(format!("{k}={v}"));
                }
            }
        }
        if pairs.is_empty() {
            None
        } else {
            Some(pairs.join("; "))
        }
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
        let tcp = connect_tcp_with_fallback(&url.host, url.port, Duration::from_secs(6))?;
        let _ = tcp.set_read_timeout(Some(self.timeout));
        let _ = tcp.set_write_timeout(Some(self.timeout));

        let mut req = format!(
            "GET {} HTTP/1.1\r\n\
             Host: {}\r\n\
             User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36\r\n\
             Accept: text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8\r\n\
             Accept-Language: en-US,en;q=0.9\r\n\
             Accept-Encoding: gzip, deflate\r\n\
             Sec-CH-UA: \"Chromium\";v=\"134\", \"Not:A-Brand\";v=\"24\", \"Google Chrome\";v=\"134\"\r\n\
             Sec-CH-UA-Mobile: ?0\r\n\
             Sec-CH-UA-Platform: \"Windows\"\r\n\
             Sec-Fetch-Dest: document\r\n\
             Sec-Fetch-Mode: navigate\r\n\
             Sec-Fetch-Site: none\r\n\
             Sec-Fetch-User: ?1\r\n\
             Upgrade-Insecure-Requests: 1\r\n\
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
            read_http_raw(&mut tls)?
        } else {
            let mut tcp = tcp;
            tcp.write_all(req.as_bytes())?;
            tcp.flush()?;
            read_http_raw(&mut tcp)?
        };

        parse_http_response(&raw, url.as_str())
    }
}

fn connect_tcp_with_fallback(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, NetError> {
    let mut addrs: Vec<_> = (host, port).to_socket_addrs()?.collect();
    if addrs.is_empty() {
        return Err(NetError::Http(format!("DNS resolved zero addresses for {host}")));
    }
    // Prefer IPv4 first to avoid broken IPv6 routes stalling connections on Windows.
    addrs.sort_by_key(|a| if a.is_ipv4() { 0u8 } else { 1u8 });
    let mut last_err = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(s) => return Ok(s),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err
        .map(NetError::from)
        .unwrap_or_else(|| NetError::Http("TCP connect failed".into())))
}

enum HttpFraming {
    Unknown,
    NoBody,
    ContentLength(usize),
    Chunked,
    UntilEof,
}

fn detect_http_framing(head: &[u8]) -> HttpFraming {
    let head_str = String::from_utf8_lossy(head);
    let mut lines = head_str.split("\r\n");
    if let Some(status_line) = lines.next() {
        let code = status_line.split_whitespace().nth(1).unwrap_or("");
        if code == "204" || code == "304" {
            return HttpFraming::NoBody;
        }
    }
    let mut content_len = None;
    let mut chunked = false;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            let kl = k.trim();
            if kl.eq_ignore_ascii_case("transfer-encoding")
                && v.to_ascii_lowercase().contains("chunked")
            {
                chunked = true;
            } else if kl.eq_ignore_ascii_case("content-length") {
                content_len = v.trim().parse::<usize>().ok();
            }
        }
    }
    if chunked {
        HttpFraming::Chunked
    } else if let Some(n) = content_len {
        HttpFraming::ContentLength(n)
    } else {
        HttpFraming::UntilEof
    }
}

fn chunk_stream_complete(mut data: &[u8]) -> bool {
    loop {
        let Some(crlf) = data.windows(2).position(|w| w == b"\r\n") else {
            return false;
        };
        let size_line = match std::str::from_utf8(&data[..crlf]) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let hex = size_line.split(';').next().unwrap_or("").trim();
        let Ok(sz) = usize::from_str_radix(hex, 16) else {
            return false;
        };
        data = &data[crlf + 2..];
        if sz == 0 {
            return data.windows(2).any(|w| w == b"\r\n") || data.is_empty();
        }
        if data.len() < sz + 2 {
            return false;
        }
        data = &data[sz + 2..];
    }
}

fn read_http_raw<R: Read>(r: &mut R) -> Result<Vec<u8>, NetError> {
    let mut out = Vec::new();
    let mut buf = [0u8; 16384];
    let mut header_end: Option<usize> = None;
    let mut framing = HttpFraming::Unknown;

    loop {
        if let Some(he) = header_end {
            let body = &out[he + 4..];
            match framing {
                HttpFraming::NoBody => break,
                HttpFraming::ContentLength(len) if body.len() >= len => break,
                HttpFraming::Chunked if chunk_stream_complete(body) => break,
                _ => {}
            }
        }
        match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                out.extend_from_slice(&buf[..n]);
                if header_end.is_none() {
                    if let Some(he) = out.windows(4).position(|w| w == b"\r\n\r\n") {
                        header_end = Some(he);
                        framing = detect_http_framing(&out[..he]);
                    }
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::WouldBlock
                ) =>
            {
                if header_end.is_some() {
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
