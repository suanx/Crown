//! 文件面板专用的只读文件系统命令。
//!
//! 这里和 agent 的 `read_file` / `list_directory` 工具分开：工具面向模型，返回
//! 带行号和裁剪规则的文本；文件面板面向 UI，需要结构化的目录项和文件预览数据。
//! 目录只做单层读取，前端展开到哪一层才加载哪一层，避免大仓库递归扫描。

use std::path::{Path, PathBuf};
use serde::Serialize;
use grep_searcher::sinks::UTF8;


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

// ── Grep: 在文件中搜索内容 ─────────────────────────────────────────────────

/// Search file contents in the workspace using a regex pattern.
/// Returns matching lines with line numbers and file paths.
#[tauri::command]
pub async fn fs_grep(
    pattern: String,
    path: Option<String>,
    glob: Option<String>,
    max_results: Option<usize>,
) -> Result<Vec<GrepMatchDto>, String> {
    let root = path.map(PathBuf::from).unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let max_res = max_results.unwrap_or(100);
    let results = std::sync::Mutex::new(Vec::new());
    let max_file_size: u64 = 5 * 1024 * 1024;

    let matcher = grep_regex::RegexMatcher::new(&pattern).map_err(|e| format!("invalid regex: {e}"))?;

    let mut searcher = grep_searcher::SearcherBuilder::new()
        .line_number(true)
        .build();

    let walk = ignore::WalkBuilder::new(&root)
        .git_global(true)
        .git_ignore(true)
        .git_exclude(true)
        .add_custom_ignore_filename(".gitignore")
        .filter_entry(move |entry| {
            let name = entry.file_name().to_string_lossy();
            // Skip VCS and dependency dirs
            !matches!(name.as_ref(), ".git" | ".svn" | ".hg" | "node_modules" | "target")
        })
        .build();

    for entry in walk.flatten() {
        if results.lock().unwrap().len() >= max_res {
            break;
        }
        let ft = match entry.file_type() {
            Some(ft) => ft,
            None => continue,
        };
        if ft.is_dir() || ft.is_symlink() {
            continue;
        }
        let file_path = entry.path();
        // Apply file glob filter if provided
        if let Some(ref g) = glob {
            let g = globset::Glob::new(g).map_err(|e| format!("invalid glob: {e}"))?;
            if !g.compile_matcher().is_match(file_path) {
                continue;
            }
        }
        // Skip large files
        if std::fs::metadata(file_path).map(|m| m.len()).unwrap_or(0) > max_file_size {
            continue;
        }
        if std::fs::metadata(file_path).map(|m| m.len()).unwrap_or(0) == 0 {
            continue;
        }

        let path_for_closure = file_path.to_path_buf();
        let _ = searcher.search_path(
            &matcher,
            &file_path,
            UTF8(|lnum, line| {
                let mut res = results.lock().unwrap();
                if res.len() < max_res {
                    res.push(GrepMatchDto {
                        path: path_for_closure.to_string_lossy().into_owned(),
                        line_number: lnum,
                        line: line.to_string(),
                    });
                }
                Ok(true)
            }),
        );
    }

    Ok(results.into_inner().unwrap())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrepMatchDto {
    pub path: String,
    pub line_number: u64,
    pub line: String,
}

// ── Glob: 按名称模式查找文件 ──────────────────────────────────────────────

/// Find files by name pattern (glob).
#[tauri::command]
pub async fn fs_glob(
    pattern: String,
    path: Option<String>,
    max_results: Option<usize>,
) -> Result<Vec<FsEntryDto>, String> {
    let root = path.map(PathBuf::from).unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let max_res = max_results.unwrap_or(200);
    let results = std::sync::Mutex::new(Vec::new());
    let glob = globset::Glob::new(&pattern).map_err(|e| format!("invalid glob: {e}"))?;
    let matcher = glob.compile_matcher();

    let walk = ignore::WalkBuilder::new(&root)
        .git_global(true)
        .git_ignore(true)
        .git_exclude(true)
        .filter_entry(move |entry| {
            let name = entry.file_name().to_string_lossy();
            !matches!(name.as_ref(), ".git" | ".svn" | ".hg" | "node_modules" | "target")
        })
        .build();

    for entry in walk.flatten() {
        if results.lock().unwrap().len() >= max_res {
            break;
        }
        let ft = match entry.file_type() {
            Some(ft) => ft,
            None => continue,
        };
        if ft.is_dir() {
            continue;
        }
        let file_path = entry.path();
        if !matcher.is_match(file_path) {
            continue;
        }
        let meta = std::fs::metadata(file_path).ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified_ms = meta.and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        results.lock().unwrap().push(FsEntryDto {
            name: file_path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            path: file_path.to_string_lossy().into_owned(),
            is_dir: false,
            size,
            modified_ms,
        });
    }

    Ok(results.into_inner().unwrap())
}

// ── Search Messages: 在对话历史中搜索消息内容 ─────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageSearchResultDto {
    pub thread_id: String,
    pub thread_title: String,
    pub message_id: String,
    pub role: String,
    pub content_preview: String,
    pub seq: i64,
    pub created_at: i64,
}

/// Search the content of all messages across threads.
/// Searches the `content` field of the JSON-stored messages.
#[tauri::command]
pub async fn search_messages(
    state: tauri::State<'_, crate::AppState>,
    query: String,
    max_results: Option<usize>,
) -> Result<Vec<MessageSearchResultDto>, String> {
    let max_res = max_results.unwrap_or(50);
    let conn = state.db.conn();

    let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));

    let mut stmt = conn.prepare(
        "SELECT m.thread_id, COALESCE(t.name, ''), m.id, m.role, m.content_json, m.seq, m.created_at
         FROM messages m
         JOIN threads t ON t.id = m.thread_id
         WHERE m.content_json LIKE ?1 ESCAPE '\\'
         ORDER BY m.created_at DESC
         LIMIT ?2"
    ).map_err(|e| e.to_string())?;

    let rows = stmt.query_map(
        rusqlite::params![pattern, max_res as i64],
        |r| {
            Ok(MessageSearchResultDto {
                thread_id: r.get(0)?,
                thread_title: r.get(1)?,
                message_id: r.get(2)?,
                role: r.get(3)?,
                content_preview: r.get::<_, String>(4)?.chars().take(200).collect(),
                seq: r.get(5)?,
                created_at: r.get(6)?,
            })
        },
    ).map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}