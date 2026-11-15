# Sub-Project 1 Implementation Plan: Chrome 157 Reverse-Engineering & Multi-Process Shell

**Spec:** `docs/superpowers/specs/2026-11-15-chrome-re-and-shell-design.md`

---

## Task 1: Initialize Repository & Reverse-Engineer Chrome `157.0.8103.51`
1. Initialize git repository in `d:\Browser` with `.gitignore` (excluding `target/`, temporary profile artifacts).
2. Run read-only PE & binary inspections on `chrome.exe`, `chrome_elf.dll`, and `chrome.dll` using:
   - `npx -y rea-agents@6.7.0 inspect-pe-resources`
   - `npx -y rea-agents@6.7.0 inspect-rust-binary`
   - `objdump -p` / Python PE parser (`scripts/re/inspect_pe.py`) for imports, exports, sections, and entry-point delegation.
3. Write `scripts/re/inspect_pak.py` to parse Chrome DataPack v4/v5 headers (`resources.pak`, `chrome_100_percent.pak`, `chrome_200_percent.pak`) and sample embedded WebUI/resource payloads.
4. Observe live Chrome `157.0.8103.51` process tree and command-line flags using a temporary `--user-data-dir` and `Win32_Process` CIM query (`scripts/re/observe_processes.ps1`).
5. Author `docs/reverse-engineering/chrome-157-architecture.md` documenting all **Observed Facts** (with digests/offsets/flags), **Inferences**, **Unknowns**, and **Clean-Room Rust Mapping**.

## Task 2: Scaffold Cargo Workspace & Implement `browser-ipc`
1. Create `Cargo.toml` workspace with `crates/browser-ipc`, `crates/browser-gfx`, `crates/browser-renderer`, `crates/browser-shell`.
2. In `browser-ipc`:
   - Implement length-prefixed (`u32` LE + UTF-8 payload, max 16 MiB) framing reader/writer.
   - Implement zero-dependency JSON/tagged serializer & parser for `BrowserToRenderer` and `RendererToBrowser` messages (`Ready`, `Navigate`, `Resize`, `MouseMove`, `MouseDown`, `KeyPress`, `FrameCommitted`, `LoadFailed`, `RequestNavigate`, `Shutdown`).
   - Implement Windows Shared Memory (`CreateFileMappingW`, `OpenFileMappingW`, `MapViewOfFile`, `UnmapViewOfFile`, `CloseHandle`) wrapper (`SharedFrameBuffer`) with `FrameHeader { width, height, stride, generation }` + `BGRA8` slice access.
   - Write comprehensive unit tests covering framing boundaries, codec roundtrips, and real Windows shared-memory cross-handle read/write.

## Task 3: Implement `browser-gfx` (2D Software Canvas & Font Rasterizer)
1. Implement `Color`, `Rect`, `Canvas` (BGRA8 buffer, alpha-blended `fill_rect`, `fill_rounded_rect`, `stroke_rect`, `draw_line`, `draw_circle`, clipped `blit`).
2. Embed a crisp built-in 8×16 VGA/bitmap font + integer scaling (`1x`, `2x`, `3x`) and clean vector toolbar icons (Back arrow, Forward arrow, Reload arc, Close `×`, Plus `+`, Lock/Info badge).
3. Write unit tests for clipping bounds, alpha blending, rounded rectangle coverage, and text measurement/rendering.

## Task 4: Implement `browser-renderer` (Isolated Child Process)
1. Implement `run_renderer_main()` reading `BrowserToRenderer` frames from `stdin` and writing `RendererToBrowser` frames to `stdout`.
2. Manage the child's mapped `SharedFrameBuffer` (re-opening on `Resize` / new `shm_name`).
3. Implement clean UI-styled internal page renderers + clickable hit-test regions:
   - `chrome://newtab` (search/shortcut cards linking to `chrome://version`, `chrome://architecture`, `chrome://crash`, `about:blank`)
   - `chrome://version` (live renderer PID, Chrome-scratch version, shared-memory dimensions, IPC stats)
   - `chrome://architecture` (visual summary of our reverse-engineered Chrome 157 process & crate architecture)
   - `chrome://crash` (deliberately calls `std::process::abort()` to demonstrate multi-process isolation)
   - `about:blank` and `http(s)://` placeholder page (indicating URL parsed and awaiting Sub-Project 2 network stack).

## Task 5: Implement `browser-shell` (Win32 Host, Tab/Process Manager, Omnibox, Crash Recovery)
1. Implement Win32 window host (`RegisterClassExW`, `CreateWindowExW`, DPI awareness, mouse/keyboard/resize handling, `StretchDIBits` presenting the Browser compositor buffer).
2. Implement `TabManager` & `TabHistory` (Back/Forward stacks, URL normalization, active tab switching, close tab, new tab).
3. Implement `ProcessManager` spawning `current_exe()` with `--type=renderer --tab-id=... --shm-name=... --width=... --height=...`, with per-tab reader threads posting custom Win32 wakeup messages (`WM_APP + 1`).
4. Implement Chrome-style top bar (`TabStrip` + `Toolbar` + `Omnibox` with keyboard editing, `Ctrl+L`, `Ctrl+T`, `Ctrl+W`, `Ctrl+R`/`F5`) and **"Aw, Snap!"** crash screen when a renderer process terminates unexpectedly, with live Reload recovery.

## Task 6: Verification & Integration Tests
1. Add integration tests in `browser-shell` / `browser-renderer` that spawn the actual compiled executable in `--type=renderer` mode, verify `Ready` + `FrameCommitted` + non-zero `BGRA8` shared memory pixels for `chrome://newtab` and `chrome://version`, and verify `chrome://crash` exits abnormally without blocking the parent IPC reader.
2. Run `cargo test --workspace` and verify all tests pass cleanly after final edits.
3. Perform a short bounded GUI launch smoke check (launching `browser-shell.exe` briefly or verifying window creation without panic) if non-blocking, and commit all work.
