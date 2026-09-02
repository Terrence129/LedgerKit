#[cfg(windows)]
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL, ICoreWebView2_19,
};
#[cfg(windows)]
use windows_core::Interface;

#[cfg(windows)]
fn set_memory_level(window: &tauri::WebviewWindow, low: bool) -> tauri::Result<()> {
    window.with_webview(move |platform_webview| {
        let result = unsafe {
            platform_webview
                .controller()
                .CoreWebView2()
                .and_then(|webview| webview.cast::<ICoreWebView2_19>())
                .and_then(|webview| {
                    webview.SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL(
                        if low { 1 } else { 0 },
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

pub fn synchronize_memory_level(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let low = !window.is_focused()? || !window.is_visible()? || window.is_minimized()?;
    set_memory_level(window, low)
}

pub fn handle_focus_change(window: &tauri::WebviewWindow, focused: bool) {
    let low = !focused
        || window.is_visible().is_ok_and(|visible| !visible)
        || window.is_minimized().is_ok_and(|minimized| minimized);
    let _ = set_memory_level(window, low);
}
