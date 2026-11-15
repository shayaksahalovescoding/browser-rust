use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use browser_net::deflate::{crc32, decompress_deflate, decompress_gzip};
use browser_net::{HttpClient, Url};

#[test]
fn url_parsing_and_relative_resolution() {
    let base = Url::parse("https://example.com:8443/docs/intro/index.html?v=1#top").unwrap();
    assert_eq!(base.scheme, "https");
    assert_eq!(base.host, "example.com");
    assert_eq!(base.port, 8443);
    assert_eq!(base.path_and_query, "/docs/intro/index.html?v=1");
    assert_eq!(base.fragment.as_deref(), Some("top"));

    assert_eq!(
        base.resolve("../style.css").unwrap().as_str(),
        "https://example.com:8443/docs/style.css"
    );
    assert_eq!(
        base.resolve("/root.html").unwrap().as_str(),
        "https://example.com:8443/root.html"
    );
    assert_eq!(
        base.resolve("//cdn.example.com/a.js").unwrap().as_str(),
        "https://cdn.example.com/a.js"
    );
}

#[test]
fn deflate_stored_and_gzip_roundtrip() {
    // Stored DEFLATE block for "Hello" (BFINAL=1, BTYPE=00, LEN=5, NLEN=0xFFFA)
    let stored = [0x01, 0x05, 0x00, 0xFA, 0xFF, b'H', b'e', b'l', b'l', b'o'];
    assert_eq!(decompress_deflate(&stored).unwrap(), b"Hello");

    // GZIP wrapper around stored DEFLATE block for "Hello"
    let mut gz = vec![0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 0, 3];
    gz.extend_from_slice(&stored);
    gz.extend_from_slice(&crc32(b"Hello").to_le_bytes());
    gz.extend_from_slice(&5u32.to_le_bytes());
    assert_eq!(decompress_gzip(&gz).unwrap(), b"Hello");
}

#[test]
fn http_client_handles_redirects_cookies_chunked_and_gzip() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let server = thread::spawn(move || {
        // First request -> 302 redirect + Set-Cookie
        {
            let (mut s1, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2048];
            let _ = s1.read(&mut buf);
            s1.write_all(
                b"HTTP/1.1 302 Found\r\n\
                  Location: /final\r\n\
                  Set-Cookie: session=abc123; Path=/\r\n\
                  Content-Length: 0\r\n\r\n",
            )
            .unwrap();
        }

        // Second request -> check Cookie header, reply chunked + gzip
        {
            let (mut s2, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2048];
            let n = s2.read(&mut buf).unwrap();
            let req_txt = String::from_utf8_lossy(&buf[..n]);
            assert!(req_txt.contains("Cookie: session=abc123"));

            let body_plain = b"<h1>Hello Browser</h1>";
            let stored_len = body_plain.len() as u16;
            let mut gz = vec![0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 0, 3, 0x01];
            gz.extend_from_slice(&stored_len.to_le_bytes());
            gz.extend_from_slice(&(!stored_len).to_le_bytes());
            gz.extend_from_slice(body_plain);
            gz.extend_from_slice(&crc32(body_plain).to_le_bytes());
            gz.extend_from_slice(&(body_plain.len() as u32).to_le_bytes());

            let head = format!(
                "HTTP/1.1 200 OK\r\n\
                 Transfer-Encoding: chunked\r\n\
                 Content-Encoding: gzip\r\n\r\n{:x}\r\n",
                gz.len()
            );
            s2.write_all(head.as_bytes()).unwrap();
            s2.write_all(&gz).unwrap();
            s2.write_all(b"\r\n0\r\n\r\n").unwrap();
        }
    });

    let mut client = HttpClient::new();
    let resp = client.get(&format!("http://{addr}/start")).unwrap();
    assert_eq!(resp.status, 200);
    assert!(!resp.from_cache);
    assert_eq!(resp.body_text(), "<h1>Hello Browser</h1>");
    assert_eq!(resp.final_url, format!("http://{addr}/final"));

    // Second fetch of final_url hits in-memory ResponseCache
    let cached = client.get(&format!("http://{addr}/final")).unwrap();
    assert!(cached.from_cache);
    assert_eq!(cached.body_text(), "<h1>Hello Browser</h1>");

    server.join().unwrap();
}

#[test]
fn body_text_decodes_iso_8859_1_charset() {
    let resp = browser_net::HttpResponse {
        status: 200,
        reason: "OK".to_string(),
        headers: vec![("Content-Type".to_string(), "text/html; charset=ISO-8859-1".to_string())],
        body: vec![0xA9, b' ', b'2', b'0', b'2', b'6', b' ', 0x97, b' ', b'\xE9'],
        final_url: "http://example.com/".to_string(),
        from_cache: false,
    };
    assert_eq!(resp.body_text(), "\u{00A9} 2026 \u{2014} \u{00E9}");
}

#[test]
#[ignore]
fn live_https_example_com() {
    let mut client = HttpClient::new();
    let resp = client.get("https://example.com/").unwrap();
    assert_eq!(resp.status, 200);
    let body = resp.body_text();
    assert!(body.contains("Example Domain"), "body: {}", &body[..body.len().min(200)]);
}
