use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;

use browser_ipc::{BrowserToRenderer, RendererToBrowser, SharedFrameBuffer};

#[derive(Debug)]
pub struct RendererEvent {
    pub tab_id: u64,
    pub kind: RendererEventKind,
}

#[derive(Debug)]
pub enum RendererEventKind {
    Ready {
        pid: u32,
    },
    FrameCommitted {
        nav_id: u64,
        shm_name: String,
        width: u32,
        height: u32,
        title: String,
    },
    LoadFailed {
        nav_id: u64,
        error: String,
    },
    RequestNavigate {
        url: String,
    },
    DevToolsInfo {
        nav_id: u64,
        network_summary: String,
        dom_dump: String,
        console_logs: String,
    },
    CursorChanged {
        cursor: u8,
        hover_url: Option<String>,
    },
    Crashed,
}

pub struct TabRendererHandle {
    child: Child,
    stdin: Option<ChildStdin>,
    shm: SharedFrameBuffer,
    alive: bool,
    closing: Arc<AtomicBool>,
}

impl TabRendererHandle {
    pub fn pixels(&self) -> &[u8] {
        self.shm.pixels()
    }

    pub fn width(&self) -> u32 {
        self.shm.width()
    }

    pub fn height(&self) -> u32 {
        self.shm.height()
    }

    pub fn shm_name(&self) -> &str {
        self.shm.name()
    }

    pub fn is_alive(&self) -> bool {
        self.alive
    }

    pub fn mark_crashed(&mut self) -> Option<i32> {
        self.alive = false;
        self.stdin.take();
        self.child.try_wait().ok().flatten().and_then(|s| s.code())
    }

    pub fn send(&mut self, msg: &BrowserToRenderer) -> io::Result<()> {
        if !self.alive {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "renderer is not alive",
            ));
        }
        if let Some(stdin) = self.stdin.as_mut() {
            msg.write_to(stdin)
        } else {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "stdin closed"))
        }
    }
}

impl Drop for TabRendererHandle {
    fn drop(&mut self) {
        self.closing.store(true, Ordering::Release);
        if self.alive {
            if let Some(mut stdin) = self.stdin.take() {
                let _ = BrowserToRenderer::Shutdown.write_to(&mut stdin);
            }
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

pub struct ProcessManager {
    renderers: HashMap<u64, TabRendererHandle>,
    tx: Sender<RendererEvent>,
    wakeup: Arc<dyn Fn() + Send + Sync>,
    next_shm_seq: u64,
}

impl ProcessManager {
    pub fn new(tx: Sender<RendererEvent>, wakeup: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            renderers: HashMap::new(),
            tx,
            wakeup,
            next_shm_seq: 1,
        }
    }

    fn alloc_shm_name(&mut self, tab_id: u64) -> String {
        let seq = self.next_shm_seq;
        self.next_shm_seq += 1;
        format!(
            "Local\\ChromeScratch_{}_{}_{}",
            std::process::id(),
            tab_id,
            seq
        )
    }

    pub fn spawn_renderer(&mut self, tab_id: u64, width: u32, height: u32) -> io::Result<()> {
        let exe = std::env::current_exe()?;
        self.spawn_renderer_with_exe(&exe, tab_id, width, height)
    }

    pub fn spawn_renderer_with_exe(
        &mut self,
        exe: &Path,
        tab_id: u64,
        width: u32,
        height: u32,
    ) -> io::Result<()> {
        self.renderers.remove(&tab_id);
        let w = width.max(1);
        let h = height.max(1);
        let shm_name = self.alloc_shm_name(tab_id);
        let shm = SharedFrameBuffer::create(&shm_name, w, h)?;

        let mut child = Command::new(exe)
            .arg("--type=renderer")
            .arg(format!("--tab-id={tab_id}"))
            .arg(format!("--shm-name={shm_name}"))
            .arg(format!("--width={w}"))
            .arg(format!("--height={h}"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let stdin = child.stdin.take();
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("failed to capture renderer stdout"))?;

        let closing = Arc::new(AtomicBool::new(false));
        let closing_clone = Arc::clone(&closing);
        let tx = self.tx.clone();
        let wakeup = Arc::clone(&self.wakeup);

        thread::spawn(move || {
            loop {
                match RendererToBrowser::read_from(&mut stdout) {
                    Ok(Some(msg)) => {
                        let kind = match msg {
                            RendererToBrowser::Ready { pid } => RendererEventKind::Ready { pid },
                            RendererToBrowser::FrameCommitted {
                                nav_id,
                                shm_name,
                                width,
                                height,
                                title,
                            } => RendererEventKind::FrameCommitted {
                                nav_id,
                                shm_name,
                                width,
                                height,
                                title,
                            },
                            RendererToBrowser::LoadFailed { nav_id, error } => {
                                RendererEventKind::LoadFailed { nav_id, error }
                            }
                            RendererToBrowser::RequestNavigate { url } => {
                                RendererEventKind::RequestNavigate { url }
                            }
                            RendererToBrowser::DevToolsInfo {
                                nav_id,
                                network_summary,
                                dom_dump,
                                console_logs,
                            } => RendererEventKind::DevToolsInfo {
                                nav_id,
                                network_summary,
                                dom_dump,
                                console_logs,
                            },
                            RendererToBrowser::CursorChanged { cursor, hover_url } => {
                                RendererEventKind::CursorChanged { cursor, hover_url }
                            }
                        };
                        let _ = tx.send(RendererEvent { tab_id, kind });
                        wakeup();
                    }
                    Ok(None) | Err(_) => {
                        if !closing_clone.load(Ordering::Acquire) {
                            let _ = tx.send(RendererEvent {
                                tab_id,
                                kind: RendererEventKind::Crashed,
                            });
                            wakeup();
                        }
                        break;
                    }
                }
            }
        });

        self.renderers.insert(
            tab_id,
            TabRendererHandle {
                child,
                stdin,
                shm,
                alive: true,
                closing,
            },
        );
        Ok(())
    }

    pub fn resize_tab(&mut self, tab_id: u64, width: u32, height: u32) -> io::Result<()> {
        let w = width.max(1);
        let h = height.max(1);
        if let Some(handle) = self.renderers.get(&tab_id) {
            if handle.width() == w && handle.height() == h {
                return Ok(());
            }
            if !handle.is_alive() {
                return Ok(());
            }
        } else {
            return Ok(());
        }

        let new_name = self.alloc_shm_name(tab_id);
        let new_shm = SharedFrameBuffer::create(&new_name, w, h)?;
        if let Some(handle) = self.renderers.get_mut(&tab_id) {
            handle.shm = new_shm;
            handle.send(&BrowserToRenderer::Resize {
                width: w,
                height: h,
                shm_name: new_name,
            })?;
        }
        Ok(())
    }

    pub fn get(&self, tab_id: u64) -> Option<&TabRendererHandle> {
        self.renderers.get(&tab_id)
    }

    pub fn get_mut(&mut self, tab_id: u64) -> Option<&mut TabRendererHandle> {
        self.renderers.get_mut(&tab_id)
    }

    pub fn remove(&mut self, tab_id: u64) {
        self.renderers.remove(&tab_id);
    }
}
