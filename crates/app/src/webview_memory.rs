//! WebView2 runtime memory control (PERF-1).
//!
//! On Windows, lowers the WebView2 memory usage target when the window loses
//! focus (`MemoryUsageTargetLevel = LOW`) and restores it on focus
//! (`NORMAL`), per Microsoft's `ICoreWebView2_19` guidance. On other
//! platforms every function is a no-op so the call sites stay
//! platform-agnostic (mirrors the provider-neutrality discipline: the
//! platform-specific path is gated, the default is a safe no-op).

use tauri::WebviewWindow;

/// Desired WebView2 memory level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryLevel {
    /// Active / focused — full performance.
    Normal,
    /// Inactive / unfocused — release caches.
    Low,
}

/// Apply the desired memory level to the window's WebView2 controller.
///
/// Returns `Ok(())` on success or when running on a non-Windows platform
/// (no-op). Errors only surface real Windows COM failures, which callers
/// log-and-ignore (best-effort — memory hinting must never break the app).
pub fn set_memory_level(window: &WebviewWindow, level: MemoryLevel) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows_impl::set_memory_level(window, level)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (window, level);
        Ok(())
    }
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use std::sync::{Arc, Mutex};

    use tauri::WebviewWindow;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    };
    use windows::core::Interface;

    use super::MemoryLevel;

    /// Set the WebView2 memory usage target level via `ICoreWebView2_19`.
    ///
    /// `with_webview` hands us the platform webview controller on the UI
    /// thread. We walk controller → `CoreWebView2` → cast to `_19` (the
    /// interface revision that introduced `MemoryUsageTargetLevel`). Older
    /// runtimes without `_19` degrade gracefully (returns an error the caller
    /// logs and ignores).
    pub fn set_memory_level(window: &WebviewWindow, level: MemoryLevel) -> Result<(), String> {
        let target = match level {
            MemoryLevel::Normal => COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
            MemoryLevel::Low => COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
        };

        let outcome: Arc<Mutex<Result<(), String>>> = Arc::new(Mutex::new(Ok(())));
        let outcome_cl = outcome.clone();

        window
            .with_webview(move |webview| {
                // SAFETY: all calls run on the UI thread inside with_webview.
                let controller = webview.controller();
                let core = match unsafe { controller.CoreWebView2() } {
                    Ok(c) => c,
                    Err(e) => {
                        *outcome_cl.lock().unwrap() = Err(format!("CoreWebView2(): {e}"));
                        return;
                    }
                };
                match core.cast::<ICoreWebView2_19>() {
                    Ok(v19) => {
                        if let Err(e) = unsafe { v19.SetMemoryUsageTargetLevel(target) } {
                            *outcome_cl.lock().unwrap() =
                                Err(format!("SetMemoryUsageTargetLevel: {e}"));
                        }
                    }
                    Err(e) => {
                        *outcome_cl.lock().unwrap() =
                            Err(format!("ICoreWebView2_19 unavailable: {e}"));
                    }
                }
            })
            .map_err(|e| format!("with_webview: {e}"))?;

        let guard = outcome.lock().unwrap();
        guard.clone()
    }
}
