//! 文件面板专用的只读文件系统命令。
//!
//! 这里和 agent 的 `read_file` / `list_directory` 工具分开：工具面向模型，返回
//! 带行号和裁剪规则的文本；文件面板面向 UI，需要结构化的目录项和文件预览数据。
//! 目录只做单层读取，前端展开到哪一层才加载哪一层，避免大仓库递归扫描。

use std::path::{Path, PathBuf};

use serde::Serialize;

/// 文件树里的单个目录项。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FsEntryDto {
    /// 文件或目录名。
    pub name: String,
    /// 绝对路径，前端展开目录或读取文件时直接回传。
    pub path: String,
    /// 是否目录。
    pub is_dir: bool,
    /// 文件大小；目录固定为 0。
    pub size: u64,
    /// 修改时间，Unix 毫秒；不可用时为 0。
    pub modified_ms: i64,
}

/// 文件预览内容。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FsFileDto {
    /// UTF-8 文本内容；二进制文件为空。
    pub content: String,
    /// 是否因为超过上限被截断。
    pub truncated: bool,
    /// 文件完整大小。
    pub size: u64,
    /// 是否疑似二进制文件。
    pub is_binary: bool,
}

/// 默认预览上限：256 KiB，足够覆盖常见源码文件，同时限制内存占用。
const DEFAULT_MAX_BYTES: u64 = 256 * 1024;

/// 默认隐藏的构建产物和依赖目录。
const NOISE_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    "__pycache__",
    ".venv",
    "venv",
];

/// 返回应用当前工作目录，作为还没有项目根时的文件面板默认根目录。
#[tauri::command]
pub async fn fs_get_workspace_root() -> Result<String, String> {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| format!("failed to resolve current directory: {e}"))
}

fn modified_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 读取单层目录，返回文件面板可直接渲染的结构化条目。
#[tauri::command]
pub async fn fs_list_directory(
    path: String,
    show_hidden: Option<bool>,
) -> Result<Vec<FsEntryDto>, String> {
    let show_hidden = show_hidden.unwrap_or(false);
    let dir = PathBuf::from(&path);

    tokio::task::spawn_blocking(move || list_blocking(&dir, show_hidden))
        .await
        .map_err(|e| format!("list task failed: {e}"))?
}

fn list_blocking(dir: &Path, show_hidden: bool) -> Result<Vec<FsEntryDto>, String> {
    let read = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut out: Vec<FsEntryDto> = Vec::new();

    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let is_dir = meta.is_dir();

        if !show_hidden {
            if name.starts_with('.') {
                continue;
            }
            if is_dir && NOISE_DIRS.contains(&name.as_str()) {
                continue;
            }
        }

        out.push(FsEntryDto {
            name,
            path: entry.path().to_string_lossy().into_owned(),
            is_dir,
            size: if is_dir { 0 } else { meta.len() },
            modified_ms: modified_ms(&meta),
        });
    }

    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

/// 读取文本文件内容给 UI 预览，超出上限时截断。
#[tauri::command]
pub async fn fs_read_file(path: String, max_bytes: Option<u64>) -> Result<FsFileDto, String> {
    let cap = max_bytes.unwrap_or(DEFAULT_MAX_BYTES);
    let p = PathBuf::from(&path);

    tokio::task::spawn_blocking(move || read_blocking(&p, cap))
        .await
        .map_err(|e| format!("read task failed: {e}"))?
}

fn read_blocking(path: &Path, cap: u64) -> Result<FsFileDto, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.is_dir() {
        return Err(format!("{} is a directory", path.display()));
    }
    let size = meta.len();
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;

    let probe = &bytes[..bytes.len().min(8192)];
    if probe.contains(&0u8) {
        return Ok(FsFileDto {
            content: String::new(),
            truncated: false,
            size,
            is_binary: true,
        });
    }

    let truncated = bytes.len() as u64 > cap;
    let slice = &bytes[..bytes.len().min(cap as usize)];
    let content = String::from_utf8_lossy(slice).into_owned();

    Ok(FsFileDto {
        content,
        truncated,
        size,
        is_binary: false,
    })
}
