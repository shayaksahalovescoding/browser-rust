use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    InvalidUrl(String),
    Io(String),
    Tls(String),
    Http(String),
    Decompress(String),
    TooManyRedirects,
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl(s) => write!(f, "invalid URL: {s}"),
            Self::Io(s) => write!(f, "network I/O error: {s}"),
            Self::Tls(s) => write!(f, "TLS error: {s}"),
            Self::Http(s) => write!(f, "HTTP error: {s}"),
            Self::Decompress(s) => write!(f, "decompression error: {s}"),
            Self::TooManyRedirects => write!(f, "too many redirects"),
        }
    }
}

impl std::error::Error for NetError {}

impl From<std::io::Error> for NetError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path_and_query: String,
    pub fragment: Option<String>,
}

impl Url {
    pub fn parse(input: &str) -> Result<Self, NetError> {
        let trimmed = input.trim();
        let (scheme, rest) = trimmed
            .split_once("://")
            .ok_or_else(|| NetError::InvalidUrl(trimmed.to_string()))?;
        let scheme = scheme.to_ascii_lowercase();
        let default_port = match scheme.as_str() {
            "http" => 80,
            "https" => 443,
            _ => return Err(NetError::InvalidUrl(format!("unsupported scheme: {scheme}"))),
        };

        let (without_frag, fragment) = match rest.split_once('#') {
            Some((before, frag)) => (before, Some(frag.to_string())),
            None => (rest, None),
        };

        let (authority, path_and_query) = match without_frag.find(['/', '?']) {
            Some(idx) => {
                let auth = &without_frag[..idx];
                let pq = &without_frag[idx..];
                let pq = if pq.starts_with('?') {
                    format!("/{pq}")
                } else {
                    pq.to_string()
                };
                (auth, pq)
            }
            None => (without_frag, "/".to_string()),
        };

        if authority.is_empty() {
            return Err(NetError::InvalidUrl("missing host".into()));
        }

        // Strip optional userinfo
        let host_port = match authority.rsplit_once('@') {
            Some((_, hp)) => hp,
            None => authority,
        };

        let (host, port) = if host_port.starts_with('[') {
            // IPv6 literal
            if let Some(end) = host_port.find(']') {
                let h = &host_port[..=end];
                let rem = &host_port[end + 1..];
                if let Some(p_str) = rem.strip_prefix(':') {
                    let p: u16 = p_str
                        .parse()
                        .map_err(|_| NetError::InvalidUrl(format!("bad port: {p_str}")))?;
                    (h.to_string(), p)
                } else {
                    (h.to_string(), default_port)
                }
            } else {
                return Err(NetError::InvalidUrl("unclosed IPv6 host".into()));
            }
        } else if let Some((h, p_str)) = host_port.rsplit_once(':') {
            let p: u16 = p_str
                .parse()
                .map_err(|_| NetError::InvalidUrl(format!("bad port: {p_str}")))?;
            (h.to_ascii_lowercase(), p)
        } else {
            (host_port.to_ascii_lowercase(), default_port)
        };

        if host.is_empty() {
            return Err(NetError::InvalidUrl("empty host".into()));
        }

        Ok(Self {
            scheme,
            host,
            port,
            path_and_query,
            fragment,
        })
    }

    pub fn resolve(&self, reference: &str) -> Result<Self, NetError> {
        let r = reference.trim();
        if r.is_empty() {
            return Ok(self.clone());
        }
        if r.contains("://") {
            return Self::parse(r);
        }
        if let Some(after_slashes) = r.strip_prefix("//") {
            return Self::parse(&format!("{}://{after_slashes}", self.scheme));
        }
        if let Some(frag) = r.strip_prefix('#') {
            let mut next = self.clone();
            next.fragment = Some(frag.to_string());
            return Ok(next);
        }

        let (pq_part, fragment) = match r.split_once('#') {
            Some((a, b)) => (a, Some(b.to_string())),
            None => (r, None),
        };

        let mut next = self.clone();
        next.fragment = fragment;

        if pq_part.starts_with('/') {
            next.path_and_query = pq_part.to_string();
            return Ok(next);
        }

        let base_path = self
            .path_and_query
            .split('?')
            .next()
            .unwrap_or("/");
        if pq_part.starts_with('?') {
            next.path_and_query = format!("{base_path}{pq_part}");
            return Ok(next);
        }

        let dir = match base_path.rfind('/') {
            Some(idx) => &base_path[..=idx],
            None => "/",
        };
        let combined = format!("{dir}{pq_part}");
        next.path_and_query = normalize_dot_segments(&combined);
        Ok(next)
    }

    pub fn host_header(&self) -> String {
        let is_default = (self.scheme == "http" && self.port == 80)
            || (self.scheme == "https" && self.port == 443);
        if is_default {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    pub fn as_str(&self) -> String {
        let mut out = format!("{}://{}{}", self.scheme, self.host_header(), self.path_and_query);
        if let Some(frag) = &self.fragment {
            out.push('#');
            out.push_str(frag);
        }
        out
    }
}

fn normalize_dot_segments(path_and_query: &str) -> String {
    let (path, query) = match path_and_query.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (path_and_query, None),
    };
    let mut stack: Vec<&str> = Vec::new();
    let trailing_slash = path.ends_with('/');
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            s => stack.push(s),
        }
    }
    let mut out = String::from("/");
    out.push_str(&stack.join("/"));
    if trailing_slash && !out.ends_with('/') {
        out.push('/');
    }
    if let Some(q) = query {
        out.push('?');
        out.push_str(q);
    }
    out
}
