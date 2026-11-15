# Google Chrome 157 (`157.0.8103.51`) Reverse-Engineering Report

**Target:** `C:\Program Files\Google\Chrome\Application\chrome.exe` (`157.0.8103.51`)  
**Methodology:** Static PE/resource inspection (`rea-agents@6.7.0`, `scripts/re/inspect_pe.py`), Chrome DataPack v5 parsing (`scripts/re/inspect_pak.py`), and bounded isolated runtime process-tree observation (`scripts/re/observe_processes.ps1`).  
**Clean-Room Rule:** All findings strictly separate **Observed Facts**, **Inferences**, and **Unknowns**, and inform our independent Rust implementation without copying proprietary code or assets.

---

## 1. Evidence Ledger

| Evidence ID / Source | Artifact / Target | SHA-256 (first 16 hex) | Size | Key Observed Facts |
|---|---|---|---|---|
| `ev_a15c5b46...` (`rea inspect-pe-resources`) | `chrome.exe` | `5b835c1083604511...` | 4,191,896 B | PE32+ (`0x8664`), 92 PE resources (`RT_GROUP_ICON` etc.) |
| `ev_44849919...` (`rea inspect-rust-binary`) | `chrome.exe` | `5b835c1083604511...` | 4,191,896 B | No `.dep-v0` section, no `/rustc/<40-hex>/` path strings |
| `scripts/re/inspect_pe.py` | `chrome.exe` | `5b835c1083604511...` | 4,191,896 B | 11 PE sections; direct imports: `chrome_elf.dll`, `KERNEL32.dll`, `ntdll.dll`, `VERSION.dll`; 2 exports (`GetHandleVerifier`, `IsSandboxedProcess`) |
| `scripts/re/inspect_pe.py` | `157.0.8103.51\chrome_elf.dll` | `c52bdba010d52f22...` | 2,846,872 B | Direct imports: `KERNEL32.dll`, `VERSION.dll`, `ntdll.dll`; 27 exports including `IsBrowserProcess`, `GetCrashpadDatabasePath_ExportThunk`, `DisableHook` |
| `scripts/re/inspect_pe.py` | `157.0.8103.51\chrome.dll` | `32a0c2f1b144c2ad...` | 311,393,432 B | 15 PE sections (`.text` ~265.4 MB); direct imports include `WS2_32.dll`, `CRYPT32.dll`, `DWrite.dll`, `WINMM.dll`, `chrome_elf.dll`; delay-imports `USER32.dll`, `GDI32.dll`, `d3d11.dll`, `d3d12.dll`, `dcomp.dll`, `Secur32.dll`, `dwmapi.dll`; 5 exports (`ChromeMain`, `CrashForExceptionInNonABICompliantCodeRange`, `GetHandleVerifier`, `IsSandboxedProcess`, `sqlite3_dbdata_init`) |
| `scripts/re/inspect_pak.py` | `resources.pak` | `fcfee8a1eed1929e...` | 26,279,968 B | DataPack v5 (`encoding=1`), 5,967 resources, 129 aliases (3,494 gzipped payloads, 108 PNGs, 283 text, 2,081 binary) |
| `scripts/re/inspect_pak.py` | `chrome_100_percent.pak` | `0143f5e746981aaf...` | 1,032,891 B | DataPack v5 (`encoding=1`), 425 resources, 21 aliases (402 PNGs, 23 gzipped) |
| `scripts/re/inspect_pak.py` | `chrome_200_percent.pak` | `57054f262964816c...` | 1,799,772 B | DataPack v5 (`encoding=1`), 425 resources, 21 aliases (402 PNGs, 23 gzipped) |
| `scripts/re/observe_processes.ps1` | Live temporary Chrome 157 instance | N/A (runtime) | 12 processes | Single-executable multi-process tree under Browser PID `5620` |

---

## 2. Observed Architecture

### 2.1 Bootstrapper vs. Core Library Split
- **Observed:** `chrome.exe` is ~4.19 MB with only 4 direct DLL imports and 2 exports (`GetHandleVerifier`, `IsSandboxedProcess`). `chrome_elf.dll` (~2.85 MB) exposes early-load hooks and crashpad/process-type thunks (`IsBrowserProcess`). The actual browser/rendering/networking/JS implementation resides in `chrome.dll` (~311.39 MB), which exports `ChromeMain`.
- **Inferred:** `chrome.exe` acts as a thin bootstrapper and sandbox/handle-verifier host that loads `chrome_elf.dll` early (before `USER32`/`GDI32` initialization) and dynamically loads `chrome.dll`, invoking `ChromeMain` for every process role.
- **Unknowns:** Exact internal `ChromeMain` startup call sequence inside `chrome.dll` (not decompiled in this pass).

### 2.2 Multi-Process Model & IPC Switches
From `observe_processes.ps1` (Browser PID `5620` spawning child processes from the same `chrome.exe` binary):
- **Browser Process** (no `--type=` switch): coordinates profile, windows, tabs, and child lifecycles.
- **Crashpad Handler** (`--type=crashpad-handler --database=... --annotation=ver=157.0.8103.51`): out-of-process crash capture.
- **GPU Process** (`--type=gpu-process --mojo-platform-channel-handle=...`): graphics/compositing process.
- **Network Service Utility** (`--type=utility --utility-sub-type=network.mojom.NetworkService --service-sandbox-type=none`).
- **Storage Service Utility** (`--type=utility --utility-sub-type=storage.mojom.StorageService --service-sandbox-type=service`).
- **Renderer Processes** (`--type=renderer --renderer-client-id=<id> --num-raster-threads=4 --mojo-platform-channel-handle=<h>`), observed in three distinct variants:
  - Standard page/tab renderer (`PID 11696`, `10216`? no, `11696` and `8692`).
  - Top-Chrome WebUI renderer (`PID 14200` with `--top-chrome-webui`).
  - Extension renderers (`PID 4024`, `6156`, `7888`, `17040`, `21120` with `--extension-process`).
- **Observed:** Every child process receives an inherited IPC channel handle (`--mojo-platform-channel-handle=<id>`) and shared-memory/field-trial handles (`--field-trial-handle=...`).

### 2.3 Resource Packaging (`.pak` Format)
- **Observed:** Chrome bundles UI templates, scripts, stylesheets, and scaled PNG assets (`100%` / `200%`) into DataPack v5 archives (`resources.pak`, `chrome_100_percent.pak`, `chrome_200_percent.pak`), where HTML/JS/CSS WebUI resources are individually gzip-compressed (`\x1f\x8b`) and localized via `$i18n{...}` placeholders.

---

## 3. Clean-Room Rust Mapping (Sub-Project 1 → 5)

| Chrome 157 Component (Observed) | Our Clean-Room Rust Equivalent |
|---|---|
| `chrome.exe` single-binary multi-mode dispatch (`--type=...`) | `browser-shell` executable dispatches `--type=renderer` to `browser_renderer::run_renderer_main()` |
| `--mojo-platform-channel-handle` + shared handles | `browser-ipc`: length-prefixed pipe messages (`stdin`/`stdout`) + named Windows shared-memory framebuffer (`CreateFileMappingW` / `MapViewOfFile`) |
| `chrome.dll` UI + `USER32`/`GDI32`/`DWrite` | `browser-shell` + `browser-gfx`: pure Win32 window host (`StretchDIBits`, DPI-aware) + Chrome-style tab strip, omnibox, navigation bar |
| `--type=renderer` isolation & Crashpad recovery | Per-tab Renderer process isolation; unexpected child exit surfaces an "Aw, Snap!" crash screen without bringing down the Browser process |
| `network.mojom.NetworkService` (Sub-Project 2) | `browser-net` crate (URL, DNS, TCP, TLS/HTTPS, HTTP/1.1) |
| Blink DOM/CSS/Layout/Skia (Sub-Project 3) | `browser-html`, `browser-css`, `browser-layout`, `browser-gfx` |
| V8 (Sub-Project 4) | `browser-js` bytecode VM & DOM bindings |
