# Sub-Projects 2–4 Design: Custom Networking, HTML/CSS/Layout Engine, JS VM & DevTools

**Date:** 2026-11-15  
**Prerequisites:** Sub-Project 1 (`browser-ipc`, `browser-gfx`, `browser-renderer`, `browser-shell`)

---

## 1. Crate Architecture Extensions

```text
crates/
├── browser-net/        # URL parser, DNS/TCP, Win32 Schannel TLS, HTTP/1.1, chunked, RFC 1951/1952 DEFLATE/GZIP, cookies, cache
├── browser-dom/        # HTML5 tokenizer & tree builder, DOM nodes, CSS parser, selector matching, cascade, box/flex layout, painter
├── browser-js/         # Lexer, recursive-descent/Pratt parser, interpreter/VM, closures, built-ins, DOM bindings
├── browser-ipc/        # Extended with Scroll/Wheel, DevTools toggle, Network/Console events
├── browser-renderer/   # Hosts browser-net + browser-dom + browser-js + scrolling + DevTools snapshot
└── browser-shell/      # Compositor + MouseWheel + F12 DevTools drawer + real HTTP/HTTPS navigation
```

---

## 2. Sub-Project 2: `browser-net` (From-Scratch Network Stack)

- **URL Parser (`url.rs`):** Full `Url { scheme, host, port, path_and_query, fragment }` with `Url::parse` and `Url::resolve(base, relative)` handling protocol-relative (`//`), root-relative (`/path`), relative (`../foo`), and query/fragment overrides.
- **RFC 1951 DEFLATE & RFC 1952 GZIP (`deflate.rs`):** Pure-Rust Huffman tree decoder + LZ77 back-reference expander supporting stored (`BTYPE=0`), fixed Huffman (`BTYPE=1`), and dynamic Huffman (`BTYPE=2`) blocks, plus GZIP header/CRC32 validation (`0x1F 0x8B`).
- **TLS over TCP (`tls.rs`):** `std::net::TcpStream` wrapped in Win32 Schannel SSPI (`secur32.dll`: `AcquireCredentialsHandleW`, `InitializeSecurityContextW`, `EncryptMessage`, `DecryptMessage`, `QueryContextAttributesW`) for real TLS 1.2/1.3 HTTPS connections using the OS certificate store.
- **HTTP/1.1 Client (`http.rs`):** Request builder, status-line & header parser, `Content-Length` + `Transfer-Encoding: chunked` body reader, automatic `gzip`/`deflate` decompression, `301`/`302`/`303`/`307`/`308` redirect following (up to 8 hops), per-host `CookieJar`, and in-memory `ResponseCache`.
- **Network Process Boundary Note:** Matching Chrome 157's `network.mojom.NetworkService` boundary, `browser-net` runs cleanly behind a `Fetcher` trait so Browser or utility processes can also host it without changing `browser-dom`.

---

## 3. Sub-Project 3: `browser-dom` (HTML, CSS, Layout & Paint)

- **HTML5 Tokenizer & Tree Builder (`html.rs`):** Character-state tokenizer (TagOpen, TagName, AttrName, AttrValue, Comment, Script/Style rawtext) + tree builder with void elements (`meta`, `link`, `br`, `hr`, `img`, `input`), entity decoding (`&amp;`, `&lt;`, `&gt;`, `&quot;`, `&nbsp;`, numeric `&#...;`), and auto-closing rules.
- **CSS3 Parser & Cascade (`css.rs`):** Stylesheet tokenizer & parser, user-agent default stylesheet (for `h1`–`h6`, `p`, `a`, `ul`/`li`, `pre`, `code`, `button`, `input`, `b`/`strong`, `div`, `body`), selector specificity `(id, class, tag)` matching (`div.card > a`, `#main .title`, etc.), inline `style=""`, and color/length resolution (`px`, `em`, `rem`, `%`, `#hex`, `rgb()`, `rgba()`).
- **Layout Engine (`layout.rs`):**
  - Box generation (`Display::Block`, `Inline`, `InlineBlock`, `Flex`, `None`)
  - Block vertical stacking + margin/padding/border box model
  - Word-wrapped inline text runs & inline boxes
  - Flexbox row/column layout (`flex-direction`, `justify-content`, `align-items`, `gap`, `flex-grow`)
  - Hit-test tree mapping `(x, y)` viewport coordinates (accounting for `scroll_y`) to `NodeId` for `<a href>` navigation, `<button>` clicks, and JS event dispatch.
- **Software Painter (`paint.rs`):** Renders backgrounds, borders, rounded corners, text runs, bullet markers, inline links, and `<img>` alt-boxes into `browser_gfx::CanvasView` with vertical scroll offset and clipping.

---

## 4. Sub-Project 4: `browser-js` & Interactive DevTools

- **JavaScript Engine (`browser-js`):**
  - Lexer + Pratt/Recursive-Descent Parser supporting `var`/`let`/`const`, `function`, arrow functions, closures, objects (`{}`), arrays (`[]`), template/string literals, numbers, booleans, `null`/`undefined`, `if/else`, `while`, `for`, `return`, `try/catch`, `typeof`, binary/unary/logical/ternary operators.
  - Environment-scoped evaluator with garbage-free `Rc<RefCell<...>>` heap objects, built-in `console.log`, `Math`, `JSON.parse`/`stringify`, `Array`/`String` methods, and `setTimeout` task queue.
- **DOM Bindings:** `window`, `document.getElementById`, `document.querySelector`, `document.querySelectorAll`, `document.createElement`, `node.appendChild`, `node.textContent`, `node.innerText`, `node.innerHTML`, `node.setAttribute`/`getAttribute`, `node.style`, `node.addEventListener("click", fn)` — mutations automatically mark the document dirty and trigger re-layout + repaint.
- **Mini-DevTools (`F12` / `Ctrl+Shift+I`):** Renderer exposes a compact DOM summary + `console.log` buffer + last fetch status over IPC so toggling DevTools in `browser-shell` shows live page inspection.
