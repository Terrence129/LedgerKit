// This is the only production module permitted to contain `unsafe`. The block
// is a narrow WebView2 COM call retained from the measured M1 spike. CI rejects
// `unsafe` anywhere else, and dependency/runtime upgrades require remeasurement.
#![allow(unsafe_code)]

#[cfg(windows)]
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL, ICoreWebView2_19,
};
#[cfg(windows)]
use windows_core::Interface;

#[cfg(windows)]
fn set_memory_level(window: &tauri::WebviewWindow, low: bool) -> tauri::Result<()> {
    window.with_webview(move |platform_webview| {
        // SAFETY: Tauri owns the WebView2 controller for the duration of this
        // callback. QueryInterface validates ICoreWebView2_19 before the call;
        // failure is non-fatal and never dereferences a raw pointer here.
        let result = unsafe {
            platform_webview
                .controller()
                .CoreWebView2()
                .and_then(|webview| webview.cast::<ICoreWebView2_19>())
                .and_then(|webview| {
                    webview.SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL(
                        i32::from(low),
                    ))
                })
        };
        if let Err(error) = result {
            eprintln!("unable to set WebView2 memory target: {error}");
        }
    })
}

#[cfg(not(windows))]
fn set_memory_level(_: &tauri::WebviewWindow, _: bool) -> tauri::Result<()> {
    Ok(())
}

pub fn handle_focus_change(window: &tauri::WebviewWindow, focused: bool) {
    let low = !focused
        || window.is_visible().is_ok_and(|visible| !visible)
        || window.is_minimized().is_ok_and(|minimized| minimized);
    let _ = set_memory_level(window, low);
}

pub fn refresh_memory_level(window: &tauri::WebviewWindow) {
    let focused = window.is_focused().unwrap_or(true);
    handle_focus_change(window, focused);
}
