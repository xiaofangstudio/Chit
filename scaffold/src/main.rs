//! main.rs —— Chit 入口：DPI 感知、单实例、命令行打开、消息循环
//!
//! 启动路径极短（docs/03 §三.3）：DPI → 单实例检查 → 建窗口 → 打开命令行文件 → 消息循环。

#![windows_subsystem = "windows"]

mod app;
mod consts;
mod edit;
mod encoding;
mod fileio;
mod ui;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_COPYDATA};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow, ShowWindow, SW_SHOW};

fn main() {
    // Per-Monitor V2 DPI 感知（多显示器不模糊，docs/05 §十）
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let args: Vec<String> = std::env::args().skip(1).collect();

    // 单实例（docs/05 §四：重复启动把已有窗口带到前台）
    match fileio::create_singleton_mutex() {
        Ok(false) => {
            forward_to_existing(&args);
            return;
        }
        _ => {}
    }

    let app = app::create_app();
    app::with_app_mut(|a| {
        for f in &args {
            a.open_path(f);
        }
        if a.tabs.is_empty() {
            a.new_tab();
        }
        a.layout();
        a.update_ui();
    });
    unsafe {
        if let Some(h) = app.try_borrow().ok().map(|a| a.hwnd) {
            let _ = ShowWindow(h, SW_SHOW);
        }
    }
    let code = app::run_message_loop(&app);
    std::process::exit(code);
}

/// 把命令行里的文件路径转发给已在运行的实例
fn forward_to_existing(args: &[String]) {
    unsafe {
        let hwnd = FindWindowW(PCWSTR(w!("Chit.Main").as_ptr()), None).unwrap_or_default();
        if hwnd.is_invalid() {
            return;
        }
        for a in args {
            let p = fileio::absolutize(a);
            let wide: Vec<u16> = p.encode_utf16().collect();
            let cds = COPYDATASTRUCT {
                dwData: 1,
                cbData: (wide.len() * 2) as u32,
                lpData: wide.as_ptr() as *mut core::ffi::c_void,
            };
            SendMessageW(
                hwnd,
                WM_COPYDATA,
                WPARAM(0),
                LPARAM(&cds as *const _ as isize),
            );
        }
        let _ = SetForegroundWindow(hwnd);
    }
}
