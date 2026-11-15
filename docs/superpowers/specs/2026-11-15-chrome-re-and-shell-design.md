# Sub-Project 1 Design: Chrome 157 Reverse-Engineering & Multi-Process Browser Shell

**Date:** 2026-11-15  
**Target Reference Binary:** Google Chrome `157.0.8103.51` (`C:\Program Files\Google\Chrome\Application\chrome.exe`)  
**Workspace:** `d:\Browser`  
**Language & Toolchain:** Rust (`1.99.1`, `x86_64-pc-windows-msvc`), zero third-party browser/UI/network dependencies (pure Rust `std` + Win32 system FFI).

---

## 1. Overall Phased Roadmap

Building a Chrome-class browser from scratch is decomposed into 5 sequential sub-projects:
1. **Sub-Project 1 (This Spec):** Reverse-engineer Chrome `157.0.8103.51` binaries/resources/processes, then build the Rust Cargo workspace, Chrome-style tabbed UI shell, multi-process Browser ↔ Renderer IPC, shared-memory framebuffer, and built-in `chrome://` pages.
2. **Sub-Project 2:** Custom Networking Stack (URL parser, DNS, TCP, TLS/HTTPS via Win32 Schannel or pure Rust, HTTP/1.1, chunked encoding, gzip/deflate, redirects, cookies, memory cache).
3. **Sub-Project 3:** Custom HTML5 Tokenizer & DOM Tree Builder, CSS3 Parser & Specificity Cascade, Block/Inline/Flexbox Layout Engine, and 2D Software Painter + Font Rasterizer.
4. **Sub-Project 4:** Custom JavaScript Engine (Lexer, Parser, Bytecode VM, Closures, Objects/Arrays, Event Loop) and DOM/Window/Fetch Bindings.
5. **Sub-Project 5:** Persistence (Bookmarks, History), Find-in-Page, and Mini-DevTools (DOM & Console Inspector).

---

## 2. Part A: Chrome 157 Reverse-Engineering Plan

Following clean-room reverse-engineering discipline (`reverse-engineer-anything`), every architectural observation must cite concrete evidence (tool output, file offsets, PE headers, `.pak` indices, or observed process arguments) and strictly separate **Observed Behavior**, **Inferences**, and **Clean-Room Design Decisions**.

### 2.1 Inspection Targets & Methods
1. **PE & Binary Split Analysis:**
   - Run `rea-agents` CLI (`inspect-pe-resources`, `inspect-rust-binary`, `inspect-with-objdump`) and custom read-only Python PE/export inspectors on `chrome.exe`, `chrome_elf.dll`, and `chrome.dll` (`157.0.8103.51`).
   - Document: entry-point delegation from `chrome.exe` to `chrome.dll`, PE resources (manifests, icons, version info, embedded HTML/data), imported Win32 DLLs, and whether Rust crates (`.dep-v0`) are embedded.
2. **Chrome DataPack (`.pak`) Reverse-Engineering:**
   - Write a read-only Python/Rust inspector (`scripts/re/inspect_pak.py`) to parse Chrome's `.pak` v4/v5 binary format (`version`, `encoding`, `resource_count`, `alias_count`, resource ID table + file offsets) against `resources.pak`, `chrome_100_percent.pak`, and `chrome_200_percent.pak`.
   - Catalog resource groups (WebUI HTML/JS/CSS bundles, Chrome UI assets, strings) without inferring behavior from filenames alone.
3. **Runtime Process Tree & IPC Flag Observation:**
   - Launch a temporary isolated Chrome instance (`--user-data-dir=<temp> --no-first-run --headless=new` or bounded scenario) or inspect live command lines via Windows CIM/WMI (`Win32_Process`) to record exact child process types (`--type=renderer`, `--type=gpu-process`, `--type=utility`, `--utility-sub-type=network.mojom.NetworkService`, etc.) and IPC handle flags.
4. **Deliverable:** `docs/reverse-engineering/chrome-157-architecture.md` with an Evidence Ledger linking each finding to its command/tool/artifact digest and explicitly listing remaining unknowns.

---

## 3. Part B: Clean-Room Multi-Process Browser Shell Architecture

### 3.1 Cargo Workspace Structure

```text
d:\Browser\
├── Cargo.toml                  # Workspace manifest
├── crates/
│   ├── browser-ipc/            # Cross-process message framing + Windows shared-memory framebuffer
│   ├── browser-gfx/            # 2D RGBA pixel canvas, primitives, vector icons, bitmap/scalable UI font
│   ├── browser-renderer/       # Renderer process runtime (--type=renderer), internal chrome:// handlers
│   └── browser-shell/          # Main executable ("chrome-scratch"): Browser UI, Tab/Process manager, Win32 host
├── scripts/re/                 # Read-only Chrome binary/pak/process inspection scripts
└── docs/
    ├── reverse-engineering/    # Evidence ledger & Chrome 157 architectural findings
    └── superpowers/
        ├── specs/              # Design specifications
        └── plans/              # Implementation plans
```

### 3.2 Process Model (Chrome-Faithful Single-Binary Dispatch)
Mirroring Chrome's `chrome.exe` bootstrapper design:
- Running `browser-shell.exe` with normal arguments starts the **Browser Process** (owns the Win32 window, UI thread, tab strip, navigation history, and child process supervision).
- For each tab, the Browser Process spawns `std::env::current_exe()` with `--type=renderer --tab-id=<u64> --shm-name=<id> --width=<u32> --height=<u32>`.
- When `main()` sees `--type=renderer`, it transfers control immediately to `browser_renderer::run_renderer_main()`, never creating a Win32 window.

### 3.3 IPC Protocol & Shared-Memory Framebuffer (`browser-ipc`)
- **Control Channel:** Length-prefixed (`u32` little-endian length + UTF-8 structured JSON/tagged payload) messages over child `stdin` (Browser → Renderer) and `stdout` (Renderer → Browser), keeping `stderr` dedicated to diagnostics.
- **Shared Pixel Memory:** Windows `CreateFileMappingW` + `MapViewOfFile` named `Local\ChromeScratch_Tab_<pid>_<tab_id>_<gen>` containing a header (`width: u32`, `height: u32`, `stride: u32`, `generation: u64`) followed by `BGRA8` pixels (native Win32 DIB byte order for zero-copy `StretchDIBits` blitting).
- **Messages (`BrowserToRenderer`):**
  - `Navigate { nav_id: u64, url: String }`
  - `Resize { width: u32, height: u32, shm_name: String }`
  - `MouseMove { x: i32, y: i32 }`
  - `MouseDown { x: i32, y: i32, button: MouseButton }`
  - `KeyPress { ch: char }`
  - `Shutdown`
- **Messages (`RendererToBrowser`):**
  - `Ready { pid: u32 }`
  - `FrameCommitted { nav_id: u64, shm_name: String, width: u32, height: u32, title: String }`
  - `LoadFailed { nav_id: u64, error: String }`
  - `RequestNavigate { url: String }` (e.g., clicking a link on `chrome://newtab`)

### 3.4 Software Graphics & Chrome-Style UI (`browser-gfx` + `browser-shell`)
- **Win32 Host:** Pure Win32 FFI (`RegisterClassExW`, `CreateWindowExW`, `PeekMessageW`/`GetMessageW`, `StretchDIBits`) with `SetProcessDpiAwarenessContext` (DPI-aware crisp rendering).
- **UI Chrome Layout (inspired by Chrome 157's top toolbar):**
  - **Tab Strip (`38px`):** Rounded active tab surface, inactive tab dividers, tab title truncation, close (`×`) button per tab, and `+` New Tab button.
  - **Navigation Toolbar (`42px`):** Back (`←`), Forward (`→`), Reload/Stop (`↻`), centered Omnibox (shows current URL, supports click-to-focus, text input, backspace, cursor/select-all `Ctrl+A`, `Enter` to navigate, `Ctrl+L` focus shortcut, `Ctrl+T` new tab, `Ctrl+W` close tab).
  - **Viewport (`client_height - 80px`):** Blits the active tab's shared-memory `BGRA8` framebuffer directly from the Renderer process.
- **Built-In Renderer Pages (Sub-Project 1 Test Surfaces):**
  - `chrome://newtab` — Clean New Tab page with quick-link cards (`chrome://version`, `chrome://architecture`, `chrome://crash`, `about:blank`).
  - `chrome://version` — Displays browser build info, multi-process status, active Renderer PID, and IPC shared-memory stats.
  - `chrome://architecture` — Summarizes our reverse-engineered Chrome 157 architecture vs. clean-room Rust subsystems.
  - `chrome://crash` — Intentionally aborts the Renderer child process (`std::process::abort()`) to verify multi-process crash isolation.
  - `about:blank` — Blank white document.
  - Any `http://` / `https://` URL before Sub-Project 2 renders a clean placeholder page indicating the URL parsed and ready for the Sub-Project 2 network stack.

### 3.5 Error Handling & Crash Isolation
- **Renderer Crash Detection:** Browser polls/reads the Renderer stdout IPC pipe on a dedicated per-tab reader thread (forwarding events to the Win32 message loop via `PostMessageW` wakeups). On unexpected EOF or non-zero child exit, the tab transitions to `TabStatus::Crashed { exit_code }`, keeps the Browser shell and all other tabs 100% responsive, and draws an **"Aw, Snap!"** crash recovery screen in the viewport. Clicking **Reload** (`↻`) or pressing `Enter` spawns a fresh `--type=renderer` child process.
- **Malformed IPC / Oversized Frames:** IPC framing rejects payloads larger than `16 MiB` and invalid UTF-8 without panicking; invalid shared-memory dimensions are clamped or rejected cleanly.
- **URL / Omnibox Normalization:** Bare strings without a scheme (e.g., `chrome://version` vs `example.com` vs `hello world`) are cleanly classified into `chrome://` / `about:` internal routes, `https://` host routes, or search queries (`chrome://search?q=...` placeholder until Sub-Project 2).

### 3.6 Testing Strategy
- **Unit Tests:**
  - `browser-ipc`: frame encode/decode roundtrips, oversized frame rejection, malformed JSON error handling, Windows shared-memory create/open/write/read roundtrip.
  - `browser-gfx`: `Canvas` clipping, alpha blending, rect/rounded-rect bounds, text measurement & glyph blitting.
  - `browser-shell`: tab manager (open/close/switch/active index bounds), per-tab back/forward history stack, omnibox input & URL classification.
- **Integration Tests:**
  - Spawn the real compiled `browser-shell` binary with `--type=renderer`, send `Navigate("chrome://version")`, assert `FrameCommitted` arrives with non-empty shared-memory pixels and expected title.
  - Send `Navigate("chrome://crash")`, assert the child process terminates abnormally without hanging the parent IPC reader and returns clean crash state.
