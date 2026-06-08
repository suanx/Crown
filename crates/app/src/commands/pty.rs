use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

use dashmap::DashMap;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

const MAX_OUTPUT_BUFFER: usize = 2 * 1024 * 1024;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PtyDataEvent {
    pub pty_id: String,
    pub data: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PtyExitEvent {
    pub pty_id: String,
    pub code: Option<i32>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PtySessionDto {
    pub pty_id: String,
    pub cwd: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PtySnapshotDto {
    pub pty_id: String,
    pub cwd: Option<String>,
    pub output: String,
}

pub struct PtyManager {
    sessions: DashMap<String, Arc<PtySession>>,
}

struct PtySession {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    cwd: Option<String>,
    output: Mutex<String>,
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }

    fn spawn(
        &self,
        app: AppHandle,
        cwd: Option<PathBuf>,
        cols: u16,
        rows: u16,
    ) -> Result<String, String> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("创建 PTY 失败: {e}"))?;

        let mut cmd = CommandBuilder::new(default_shell());
        let cwd_text = cwd.as_ref().map(|p| p.to_string_lossy().into_owned());
        if let Some(cwd) = cwd.as_ref() {
            cmd.cwd(cwd);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("启动 shell 失败: {e}"))?;
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("克隆 PTY reader 失败: {e}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("获取 PTY writer 失败: {e}"))?;

        let pty_id = ulid::Ulid::new().to_string();
        let session = Arc::new(PtySession {
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            child: Mutex::new(child),
            cwd: cwd_text,
            output: Mutex::new(String::new()),
        });
        self.sessions.insert(pty_id.clone(), session.clone());

        spawn_reader(app, pty_id.clone(), session, reader);

        Ok(pty_id)
    }

    fn list(&self) -> Vec<PtySessionDto> {
        let mut sessions: Vec<PtySessionDto> = self
            .sessions
            .iter()
            .map(|entry| PtySessionDto {
                pty_id: entry.key().clone(),
                cwd: entry.value().cwd.clone(),
            })
            .collect();
        sessions.sort_by(|a, b| a.pty_id.cmp(&b.pty_id));
        sessions
    }

    fn snapshot(&self, pty_id: &str) -> Result<PtySnapshotDto, String> {
        let session = self
            .sessions
            .get(pty_id)
            .ok_or_else(|| format!("终端不存在: {pty_id}"))?;
        let output = session
            .output
            .lock()
            .map_err(|_| "终端输出缓存锁已损坏".to_string())?
            .clone();
        Ok(PtySnapshotDto {
            pty_id: pty_id.to_string(),
            cwd: session.cwd.clone(),
            output,
        })
    }

    fn write(&self, pty_id: &str, data: &str) -> Result<(), String> {
        let session = self
            .sessions
            .get(pty_id)
            .ok_or_else(|| format!("终端不存在: {pty_id}"))?;
        let mut writer = session
            .writer
            .lock()
            .map_err(|_| "终端 writer 锁已损坏".to_string())?;
        writer
            .write_all(data.as_bytes())
            .map_err(|e| format!("写入终端失败: {e}"))?;
        writer.flush().map_err(|e| format!("刷新终端失败: {e}"))
    }

    fn resize(&self, pty_id: &str, cols: u16, rows: u16) -> Result<(), String> {
        let session = self
            .sessions
            .get(pty_id)
            .ok_or_else(|| format!("终端不存在: {pty_id}"))?;
        let master = session
            .master
            .lock()
            .map_err(|_| "终端 master 锁已损坏".to_string())?;
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("调整终端尺寸失败: {e}"))
    }

    fn kill(&self, pty_id: &str) -> Result<(), String> {
        let Some((_, session)) = self.sessions.remove(pty_id) else {
            return Ok(());
        };
        let mut child = session
            .child
            .lock()
            .map_err(|_| "终端进程锁已损坏".to_string())?;
        child.kill().map_err(|e| format!("结束终端失败: {e}"))
    }

    fn remove(&self, pty_id: &str) {
        self.sessions.remove(pty_id);
    }
}

#[tauri::command]
pub async fn pty_list(state: tauri::State<'_, AppState>) -> Result<Vec<PtySessionDto>, String> {
    Ok(state.pty.list())
}

#[tauri::command]
pub async fn pty_snapshot(
    state: tauri::State<'_, AppState>,
    pty_id: String,
) -> Result<PtySnapshotDto, String> {
    state.pty.snapshot(&pty_id)
}

#[tauri::command]
pub async fn pty_spawn(
    state: tauri::State<'_, AppState>,
    app: AppHandle,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
) -> Result<String, String> {
    let cwd = cwd.map(PathBuf::from);
    state.pty.spawn(app, cwd, cols.max(1), rows.max(1))
}

#[tauri::command]
pub async fn pty_write(
    state: tauri::State<'_, AppState>,
    pty_id: String,
    data: String,
) -> Result<(), String> {
    state.pty.write(&pty_id, &data)
}

#[tauri::command]
pub async fn pty_resize(
    state: tauri::State<'_, AppState>,
    pty_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    state.pty.resize(&pty_id, cols.max(1), rows.max(1))
}

#[tauri::command]
pub async fn pty_kill(state: tauri::State<'_, AppState>, pty_id: String) -> Result<(), String> {
    state.pty.kill(&pty_id)
}

fn spawn_reader(
    app: AppHandle,
    pty_id: String,
    session: Arc<PtySession>,
    mut reader: Box<dyn Read + Send>,
) {
    thread::spawn(move || {
        let mut buf = [0_u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    append_output(&session, &data);
                    let _ = app.emit(
                        "pty:data",
                        PtyDataEvent {
                            pty_id: pty_id.clone(),
                            data,
                        },
                    );
                }
                Err(e) => {
                    tracing::debug!(pty_id = %pty_id, error = %e, "PTY reader 已结束");
                    break;
                }
            }
        }
        let _ = app.emit(
            "pty:exit",
            PtyExitEvent {
                pty_id: pty_id.clone(),
                code: None,
            },
        );
        if let Some(state) = app.try_state::<AppState>() {
            state.pty.remove(&pty_id);
        }
    });
}

fn append_output(session: &PtySession, data: &str) {
    let Ok(mut output) = session.output.lock() else {
        return;
    };
    output.push_str(data);
    if output.len() <= MAX_OUTPUT_BUFFER {
        return;
    }
    let overflow = output.len() - MAX_OUTPUT_BUFFER;
    let drain_to = output
        .char_indices()
        .find(|(idx, _)| *idx >= overflow)
        .map(|(idx, _)| idx)
        .unwrap_or(overflow);
    output.drain(..drain_to);
}

fn default_shell() -> String {
    #[cfg(windows)]
    {
        if shell_exists("pwsh.exe") {
            return "pwsh.exe".to_string();
        }
        if shell_exists("powershell.exe") {
            return "powershell.exe".to_string();
        }
        "cmd.exe".to_string()
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

#[cfg(windows)]
fn shell_exists(name: &str) -> bool {
    std::process::Command::new("where.exe")
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
