//! From-scratch networking stack: URL parser, RFC 1951/1952 DEFLATE/GZIP, Win32 Schannel TLS, and HTTP/1.1 client.

pub mod deflate;
pub mod http;
pub mod tls;
pub mod url;

pub use http::{CookieJar, HttpClient, HttpResponse, ResponseCache};
pub use url::{NetError, Url};
