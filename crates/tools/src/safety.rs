//! Path safety checks — mirrors Claude Code's `isDangerousFilePathToAutoEdit` +
//! `hasSuspiciousWindowsPathPattern` from `src/utils/permissions/filesystem.ts`.
//!
//! These checks are **bypass-immune**: even in `bypassPermissions` mode, writing
//! to sensitive paths triggers an `Ask` decision. This prevents prompt injection
//! attacks that trick the model into overwriting shell configs, git hooks, or
//! credential stores.

/// Result of a path safety check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafetyCheckResult {
    /// `true` if the path is safe for auto-editing.
    pub safe: bool,
    /// Human-readable explanation (shown in approval dialog).
    pub message: String,
    /// When `true`, an auto-mode classifier is allowed to approve this.
    /// When `false`, always force manual user approval.
    /// Currently: sensitive files → `true`, Windows bypass patterns → `false`.
    pub classifier_approvable: bool,
}

/// Dangerous directories that should be protected from auto-editing.
/// These directories contain sensitive configuration or executable files.
pub const DANGEROUS_DIRECTORIES: &[&str] = &[
    ".git",
    ".vscode",
    ".idea",
    ".claude",
    ".crown",
    ".deepseek-agent",
    ".ssh",
    ".gnupg",
    ".aws",
];

/// Dangerous files that should be protected from auto-editing.
/// These files can be used for code execution or data exfiltration.
pub const DANGEROUS_FILES: &[&str] = &[
    ".gitconfig",
    ".gitmodules",
    ".bashrc",
    ".bash_profile",
    ".zshrc",
    ".zprofile",
    ".profile",
    ".ripgreprc",
    ".mcp.json",
    ".claude.json",
    ".env",
    ".env.local",
    ".env.production",
    "id_rsa",
    "id_ed25519",
    "id_ecdsa",
    "id_dsa",
    "credentials",
    "authorized_keys",
    "known_hosts",
    ".git-credentials",
    ".npmrc",
    ".netrc",
    ".pypirc",
    ".dockercfg",
    "config.toml", // deepseek-agent's own config
];

/// Suffixes that mark private keys / certificates — matched case-insensitively
/// against the filename. Writing these almost always means key material.
pub const DANGEROUS_FILE_SUFFIXES: &[&str] = &[".pem", ".key", ".p12", ".pfx"];

/// Absolute system directories that must never be auto-edited. Matched as a
/// path prefix (after separator normalization + lowercasing). Cross-platform:
/// POSIX (`/etc`, `/usr`, `/boot`, `/sys`, `/proc`), macOS (`/system`,
/// `/library`), Windows (`c:/windows`, `c:/program files`).
pub const DANGEROUS_SYSTEM_PREFIXES: &[&str] = &[
    "/etc/",
    "/usr/",
    "/bin/",
    "/sbin/",
    "/boot/",
    "/sys/",
    "/proc/",
    "/system/",
    "/library/",
    "c:/windows/",
    "c:/program files/",
    "c:/program files (x86)/",
];

/// Check whether a file path is safe for automatic editing.
///
/// Returns `SafetyCheckResult { safe: false, .. }` when the path points to a
/// sensitive location (git internals, shell configs, SSH keys, etc.) or uses
/// Windows bypass patterns (ADS, 8.3 names, long path prefix, etc.).
///
/// This function operates on the path string only — it does NOT resolve
/// symlinks or access the filesystem.
pub fn check_path_safety(path: &str) -> SafetyCheckResult {
    // Normalize separators for cross-platform matching
    let normalized = path.replace('\\', "/");

    // 1. Windows bypass patterns (highest priority, NOT classifier-approvable)
    if let Some(msg) = check_suspicious_windows_pattern(&normalized, path) {
        return SafetyCheckResult {
            safe: false,
            message: msg,
            classifier_approvable: false,
        };
    }

    // 2. Dangerous directory check (classifier-approvable)
    if let Some(msg) = check_dangerous_directory(&normalized) {
        return SafetyCheckResult {
            safe: false,
            message: msg,
            classifier_approvable: true,
        };
    }

    // 2b. System directory prefix check (classifier-approvable)
    if let Some(msg) = check_system_directory(&normalized) {
        return SafetyCheckResult {
            safe: false,
            message: msg,
            classifier_approvable: true,
        };
    }

    // 3. Dangerous file check (classifier-approvable)
    if let Some(msg) = check_dangerous_file(&normalized) {
        return SafetyCheckResult {
            safe: false,
            message: msg,
            classifier_approvable: true,
        };
    }

    SafetyCheckResult {
        safe: true,
        message: String::new(),
        classifier_approvable: true,
    }
}

/// Check for suspicious Windows path patterns that could bypass security.
fn check_suspicious_windows_pattern(normalized: &str, original: &str) -> Option<String> {
    // NTFS Alternate Data Streams (e.g., file.txt::$DATA, file.txt:stream)
    // Skip drive letters (first 2 chars like C:)
    if cfg!(windows) || cfg!(target_os = "linux") {
        // On Windows and WSL, a colon after the drive-letter prefix indicates
        // an ADS. Skip the first 2 *characters* (a `C:`-style drive prefix),
        // then look for any remaining colon. Must skip by char boundary, not
        // byte index: a byte slice like `original[2..]` panics when byte 2
        // falls inside a multibyte UTF-8 char (e.g. a Chinese path segment).
        let after_drive = match original.char_indices().nth(2) {
            Some((byte_idx, _)) => &original[byte_idx..],
            None => "", // fewer than 3 chars → no room for an ADS colon
        };
        if after_drive.contains(':') {
            return Some("NTFS Alternate Data Stream detected — requires manual approval".into());
        }
    }

    // 8.3 short names (e.g., GIT~1, CLAUDE~1)
    if normalized.contains('~') {
        // Check for tilde followed by a digit
        let bytes = normalized.as_bytes();
        for i in 0..bytes.len().saturating_sub(1) {
            if bytes[i] == b'~' && bytes[i + 1].is_ascii_digit() {
                return Some("8.3 short filename detected — requires manual approval".into());
            }
        }
    }

    // Long path prefixes (\\?\, \\.\, //?/, //./
    if original.starts_with("\\\\?\\")
        || original.starts_with("\\\\.\\")
        || normalized.starts_with("//?/")
        || normalized.starts_with("//./")
    {
        return Some("Long path prefix detected — requires manual approval".into());
    }

    // Trailing dots and spaces (Windows strips these during resolution)
    if original.ends_with('.') || original.ends_with(' ') {
        // But ignore paths that are just "." or ".."
        let trimmed = original.trim_end_matches(['.', ' ']);
        if !trimmed.is_empty() && trimmed != "." && trimmed != ".." {
            return Some("Trailing dots/spaces in path — requires manual approval".into());
        }
    }

    // DOS device names (CON, PRN, AUX, NUL, COM1-9, LPT1-9) at end
    let last_segment = normalized.rsplit('/').next().unwrap_or("");
    if let Some(dot_pos) = last_segment.rfind('.') {
        let after_dot = &last_segment[dot_pos + 1..];
        let upper = after_dot.to_uppercase();
        if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (upper.len() == 4
                && (upper.starts_with("COM") || upper.starts_with("LPT"))
                && upper.as_bytes().get(3).is_some_and(|b| b.is_ascii_digit()))
        {
            return Some("DOS device name suffix detected — requires manual approval".into());
        }
    }

    // Three or more consecutive dots as a path component
    // Only when dots are a standalone segment (preceded/followed by separator)
    for segment in normalized.split('/') {
        if segment.len() >= 3 && segment.chars().all(|c| c == '.') {
            return Some("Triple-dot path component detected — requires manual approval".into());
        }
    }

    // UNC paths
    if original.starts_with("\\\\") || normalized.starts_with("//") {
        // Skip //?/ and //./ which we already caught above
        if !normalized.starts_with("//?/") && !normalized.starts_with("//./") {
            return Some("UNC network path detected — requires manual approval".into());
        }
    }

    None
}

/// Check if the path traverses a dangerous directory.
fn check_dangerous_directory(normalized: &str) -> Option<String> {
    let segments: Vec<&str> = normalized.split('/').collect();
    for segment in &segments {
        let lower = segment.to_lowercase();
        for &dir in DANGEROUS_DIRECTORIES {
            if lower == dir {
                return Some(format!(
                    "Path contains sensitive directory '{dir}' — requires approval"
                ));
            }
        }
    }
    None
}

/// Check if the path falls under a protected system directory.
///
/// Matches an absolute path prefix (POSIX `/etc/...`, Windows `c:/windows/...`).
/// Relative paths like `etc/config.rs` are NOT matched — only genuine system
/// locations. Drive letters are lowercased by the caller's normalization step
/// is not applied here, so we lowercase locally.
fn check_system_directory(normalized: &str) -> Option<String> {
    let lower = normalized.to_lowercase();
    for &prefix in DANGEROUS_SYSTEM_PREFIXES {
        if lower.starts_with(prefix) {
            let dir = prefix.trim_end_matches('/');
            return Some(format!(
                "Path is under protected system directory '{dir}' — requires approval"
            ));
        }
    }
    None
}

/// Check if the filename is a dangerous file.
fn check_dangerous_file(normalized: &str) -> Option<String> {
    let filename = normalized.rsplit('/').next().unwrap_or(normalized);
    let lower_filename = filename.to_lowercase();
    // Exact filename match.
    for &dangerous in DANGEROUS_FILES {
        if lower_filename == dangerous.to_lowercase() {
            return Some(format!(
                "'{filename}' is a sensitive file — requires approval"
            ));
        }
    }
    // Suffix match for key/cert material.
    for &suffix in DANGEROUS_FILE_SUFFIXES {
        if lower_filename.ends_with(suffix) {
            return Some(format!(
                "'{filename}' looks like key/certificate material — requires approval"
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_normal_paths() {
        assert!(check_path_safety("src/main.rs").safe);
        assert!(check_path_safety("Cargo.toml").safe);
        assert!(check_path_safety("tests/integration.rs").safe);
        assert!(check_path_safety("frontend/src/App.tsx").safe);
        assert!(check_path_safety("/home/user/project/lib.rs").safe);
    }

    #[test]
    fn dangerous_git_directory() {
        assert!(!check_path_safety(".git/config").safe);
        assert!(!check_path_safety(".git/hooks/pre-commit").safe);
        assert!(!check_path_safety("repo/.git/HEAD").safe);
        assert!(check_path_safety(".git/config").classifier_approvable);
    }

    #[test]
    fn dangerous_ssh_directory() {
        assert!(!check_path_safety(".ssh/id_rsa").safe);
        assert!(!check_path_safety("/home/user/.ssh/authorized_keys").safe);
    }

    #[test]
    fn dangerous_dotfiles() {
        assert!(!check_path_safety(".bashrc").safe);
        assert!(!check_path_safety(".zshrc").safe);
        assert!(!check_path_safety(".profile").safe);
        assert!(!check_path_safety(".gitconfig").safe);
        assert!(!check_path_safety("/home/user/.bash_profile").safe);
    }

    #[test]
    fn dangerous_env_files() {
        assert!(!check_path_safety(".env").safe);
        assert!(!check_path_safety(".env.local").safe);
        assert!(!check_path_safety(".env.production").safe);
    }

    #[test]
    fn dangerous_credential_files() {
        assert!(!check_path_safety("id_rsa").safe);
        assert!(!check_path_safety("credentials").safe);
    }

    #[test]
    fn windows_ads_detection() {
        // NTFS Alternate Data Streams
        let r = check_path_safety("file.txt::$DATA");
        if cfg!(windows) || cfg!(target_os = "linux") {
            assert!(!r.safe);
            assert!(!r.classifier_approvable);
        }
    }

    #[test]
    fn windows_8dot3_short_names() {
        let r = check_path_safety("GIT~1/config");
        assert!(!r.safe);
        assert!(!r.classifier_approvable);
    }

    #[test]
    fn windows_long_path_prefix() {
        let r = check_path_safety("\\\\?\\C:\\Users\\test");
        assert!(!r.safe);
        assert!(!r.classifier_approvable);
    }

    #[test]
    fn trailing_dot_bypass() {
        let r = check_path_safety(".git.");
        assert!(!r.safe);
        assert!(!r.classifier_approvable);
    }

    #[test]
    fn dos_device_name() {
        let r = check_path_safety("settings.json.CON");
        assert!(!r.safe);
        assert!(!r.classifier_approvable);
    }

    #[test]
    fn triple_dot_component() {
        let r = check_path_safety("path/.../file.txt");
        assert!(!r.safe);
        assert!(!r.classifier_approvable);
    }

    #[test]
    fn unc_path_detection() {
        let r = check_path_safety("\\\\server\\share\\file.txt");
        assert!(!r.safe);
        assert!(!r.classifier_approvable);
    }

    #[test]
    fn case_insensitive_directory_check() {
        assert!(!check_path_safety(".Git/config").safe);
        assert!(!check_path_safety(".GIT/hooks/pre-commit").safe);
        assert!(!check_path_safety(".SSH/id_rsa").safe);
    }

    #[test]
    fn vscode_directory_protected() {
        assert!(!check_path_safety(".vscode/settings.json").safe);
        assert!(!check_path_safety(".vscode/extensions.json").safe);
    }

    #[test]
    fn deepseek_agent_config_protected() {
        assert!(!check_path_safety(".deepseek-agent/settings.json").safe);
    }

    #[test]
    fn crown_config_protected() {
        assert!(!check_path_safety(".crown/settings.json").safe);
    }


    /// P1-6: system directories must require approval (model writing
    /// /etc/hosts, /etc/sudoers, Windows system dirs, etc.).
    #[test]
    fn system_directories_protected() {
        assert!(!check_path_safety("/etc/hosts").safe);
        assert!(!check_path_safety("/etc/sudoers").safe);
        assert!(!check_path_safety("/usr/bin/foo").safe);
        assert!(!check_path_safety("/boot/grub/grub.cfg").safe);
        assert!(!check_path_safety("/System/Library/x").safe);
        assert!(!check_path_safety("C:/Windows/System32/drivers/etc/hosts").safe);
        assert!(!check_path_safety("C:\\Windows\\System32\\config").safe);
        assert!(!check_path_safety("C:/Program Files/app/x.dll").safe);
    }

    /// P1-6: additional credential files + suffix matching for keys/certs.
    #[test]
    fn extended_credential_files_protected() {
        assert!(!check_path_safety(".npmrc").safe);
        assert!(!check_path_safety(".netrc").safe);
        assert!(!check_path_safety(".pypirc").safe);
        assert!(!check_path_safety(".git-credentials").safe);
        assert!(!check_path_safety("home/user/.aws/credentials").safe);
        assert!(!check_path_safety("authorized_keys").safe);
        assert!(!check_path_safety("known_hosts").safe);
        assert!(!check_path_safety("id_ecdsa").safe);
        assert!(!check_path_safety("id_dsa").safe);
        // Suffix matching for private keys / certs.
        assert!(!check_path_safety("certs/server.pem").safe);
        assert!(!check_path_safety("secrets/tls.key").safe);
        assert!(!check_path_safety("foo/private.p12").safe);
    }

    /// P1-6: normal source files with similar-looking names stay safe.
    #[test]
    fn p1_6_no_false_positives() {
        assert!(check_path_safety("src/key.rs").safe); // not *.key
        assert!(check_path_safety("docs/system.md").safe); // not a system dir
        assert!(check_path_safety("etc/config.rs").safe); // relative "etc", not /etc
        assert!(check_path_safety("usr_manager.ts").safe);
    }

    /// Regression: multibyte (non-ASCII) paths must not panic. The ADS check
    /// used to do `original[2..]` (a BYTE slice), which panics when byte index
    /// 2 falls inside a multibyte UTF-8 character — e.g. a Chinese directory
    /// name (each CJK char is 3 bytes). The project's own working directory
    /// contains Chinese, so this was guaranteed to crash on any write there.
    #[test]
    fn multibyte_path_does_not_panic() {
        // Must not panic, and a normal Chinese-named path is safe.
        assert!(check_path_safety("测试区域/main.rs").safe);
        assert!(check_path_safety("项目/源代码/文件.txt").safe);
        // A single leading multibyte char (byte index 2 lands mid-char).
        let _ = check_path_safety("中");
        let _ = check_path_safety("中a:b");
        // Windows-style absolute path with Chinese segments.
        let _ = check_path_safety("C:\\workspace\\crown\\file.rs");
    }

    /// ADS detection must still work after the multibyte fix: a real
    /// `file.txt:stream` (colon after the first path segment, not a drive
    /// letter) is still flagged on Windows/Linux.
    #[test]
    fn ads_still_detected_after_multibyte_fix() {
        if cfg!(windows) || cfg!(target_os = "linux") {
            assert!(!check_path_safety("file.txt:hidden").safe);
            assert!(!check_path_safety("notes.md::$DATA").safe);
        }
        // A bare Windows drive path (colon at index 1) must NOT be flagged as ADS.
        // `C:/Users/x/file.rs` — the only colon is the drive letter.
        let drive = check_path_safety("C:/Users/test/file.rs");
        assert!(
            drive.safe,
            "drive-letter colon must not be mistaken for ADS: {}",
            drive.message
        );
    }
}
