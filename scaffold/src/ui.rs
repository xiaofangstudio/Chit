//! ui.rs —— 配色、DPI、线性图标绘制与自绘控件（工具栏/标签栏/状态栏/查找条）

#![allow(non_snake_case)]

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, ClientToScreen, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint,
    FillRect, InvalidateRect, LineTo, MoveToEx, RoundRect, ScreenToClient, SelectObject,
    SetBkMode, SetTextColor, HBRUSH, HDC, PAINTSTRUCT, PS_SOLID, TRANSPARENT, DT_CALCRECT,
    DT_END_ELLIPSIS, DT_LEFT, DT_SINGLELINE, DT_VCENTER,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging as wam;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, LoadCursorW, RegisterClassExW, SendMessageW,
    SetWindowPos, ShowWindow, CS_HREDRAW, CS_VREDRAW, SWP_NOACTIVATE, SWP_NOZORDER, SW_HIDE,
    SW_SHOW, WINDOW_EX_STYLE, WM_COMMAND, WM_MBUTTONDOWN, WM_PAINT, WS_CHILD, WS_VISIBLE,
};

use crate::app::{
    tab_width, tabbar_hit, tabbar_hit_tab, toolbar_button_at, toolbar_button_enabled,
    toolbar_button_rect, toolbar_button_toggled, with_app_mut, with_app_try, App, TOOLBAR_BTNS,
};
use crate::consts::*;
use crate::fileio::to_wide;

// ---------------- 配色（Word 风浅色主题，点缀蓝 #2B579A） ----------------

pub const CLR_CHROME:    u32 = 0x00ECEAE8;
pub const CLR_EDITOR:    u32 = 0x00FFFFFF;
pub const CLR_TEXT:      u32 = 0x001B1B1B;
pub const CLR_TEXT_SUB:  u32 = 0x00616161;
pub const CLR_HOVER:     u32 = 0x00E1DFDD;
pub const CLR_PRESSED:   u32 = 0x00D2D0CE;
pub const CLR_BORDER:    u32 = 0x00E1DFDD;
pub const CLR_ACCENT:    u32 = 0x009A572B; // #2B579A（BGR）
pub const CLR_ACCENT_BG: u32 = 0x00F7EFEA;
pub const CLR_DISABLED:  u32 = 0x00B6B6B6;
pub const CLR_TAB_ACTIVE:u32 = 0x00FFFFFF;

#[inline]
fn cr(v: u32) -> COLORREF {
    COLORREF(v)
}

pub fn brush(v: u32) -> HBRUSH {
    unsafe { CreateSolidBrush(cr(v)) }
}

/// DPI 换算：设计像素（96dpi 基准）→ 实际像素
pub fn dp(app: &App, v: i32) -> i32 {
    (v * app.dpi as i32 + 48) / 96
}

// ---------------- 子控件类型标记（存 GWLP_USERDATA） ----------------

pub const CHILD_TOOLBAR: isize = 1;
pub const CHILD_TABBAR: isize = 2;
pub const CHILD_STATUS: isize = 3;
pub const CHILD_FINDBAR: isize = 4;

fn set_child_kind(hwnd: HWND, kind: isize) {
    unsafe {
        wam::SetWindowLongPtrW(hwnd, wam::GWLP_USERDATA, kind);
    }
}

fn get_child_kind(hwnd: HWND) -> isize {
    unsafe { wam::GetWindowLongPtrW(hwnd, wam::GWLP_USERDATA) }
}

/// 创建自绘子控件
pub fn create_ui_child(app: &mut App, kind: isize, class: PCWSTR, text: &str) -> HWND {
    let text_w = to_wide(text);
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            PCWSTR(text_w.as_ptr()),
            WS_CHILD | WS_VISIBLE,
            0, 0, 0, 0,
            app.hwnd,
            wam::HMENU::default(),
            app.hinst,
            None,
        )
        .expect("创建子控件失败")
    };
    set_child_kind(hwnd, kind);
    hwnd
}

// ---------------- 控件注册 ----------------

pub fn register_ui_classes() {
    unsafe {
    let hinst = GetModuleHandleW(None).expect("获取模块句柄失败");
    for name in [w!("Chit.Toolbar"), w!("Chit.Tabbar"), w!("Chit.Status"), w!("Chit.FindBar")] {
        let wc = wam::WNDCLASSEXW {
            cbSize: std::mem::size_of::<wam::WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(ui_child_wndproc),
            hInstance: hinst.into(),
            hCursor: LoadCursorW(windows::Win32::Foundation::HINSTANCE::default(), wam::IDC_ARROW).expect("加载光标失败"),
            lpszClassName: name,
            ..Default::default()
        };
        RegisterClassExW(&wc);
    }
    }
}

/// 统一的子控件窗口过程（按 USERDATA 分发）
unsafe extern "system" fn ui_child_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match get_child_kind(hwnd) {
        CHILD_TOOLBAR => toolbar_proc(hwnd, msg, wp, lp),
        CHILD_TABBAR => tabbar_proc(hwnd, msg, wp, lp),
        CHILD_STATUS => status_proc(hwnd, msg, wp, lp),
        CHILD_FINDBAR => findbar_proc(hwnd, msg, wp, lp),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

// ---------------- 线性图标绘制 ----------------

#[derive(Clone, Copy, PartialEq)]
pub enum IconKind {
    New, Open, Save, Undo, Redo, Find, Wrap, FontMinus, FontPlus, Menu,
}

/// 在 (cx, cy) 中心画 d 像素线性图标
pub fn draw_icon(hdc: HDC, kind: IconKind, cx: i32, cy: i32, d: i32, color: u32) {
    unsafe {
        let pen = CreatePen(PS_SOLID, std::cmp::max(1, (d + 5) / 8), cr(color));
        let old = SelectObject(hdc, pen);
        let r = d / 2;
        let (l, t, rt, b) = (cx - r, cy - r, cx + r, cy + r);
        match kind {
            IconKind::New => {
                MoveToEx(hdc, l + d * 3 / 10, t + d / 5, None);
                LineTo(hdc, l + d * 3 / 10, b - d / 5);
                LineTo(hdc, rt - d * 3 / 10, b - d / 5);
                LineTo(hdc, rt - d * 3 / 10, t + d / 5);
                LineTo(hdc, l + d * 3 / 10, t + d / 5);
                MoveToEx(hdc, l + d * 45 / 100, t + d * 4 / 10, None);
                LineTo(hdc, rt - d * 45 / 100, t + d * 4 / 10);
                MoveToEx(hdc, l + d * 45 / 100, t + d * 55 / 100, None);
                LineTo(hdc, rt - d * 55 / 100, t + d * 55 / 100);
            }
            IconKind::Open => {
                MoveToEx(hdc, l + d / 8, t + d * 3 / 10, None);
                LineTo(hdc, l + d / 8, b - d / 6);
                LineTo(hdc, rt - d / 8, b - d / 6);
                LineTo(hdc, rt - d / 8, t + d * 45 / 100);
                LineTo(hdc, cx, t + d * 45 / 100);
                LineTo(hdc, cx - d / 8, t + d * 3 / 10);
                LineTo(hdc, l + d / 8, t + d * 3 / 10);
            }
            IconKind::Save => {
                MoveToEx(hdc, l + d / 6, t + d / 6, None);
                LineTo(hdc, rt - d * 4 / 10, t + d / 6);
                LineTo(hdc, rt - d / 6, t + d * 4 / 10);
                LineTo(hdc, rt - d / 6, b - d / 6);
                LineTo(hdc, l + d / 6, b - d / 6);
                LineTo(hdc, l + d / 6, t + d / 6);
                MoveToEx(hdc, cx - d / 5, t + d / 6, None);
                LineTo(hdc, cx - d / 5, t + d * 4 / 10);
                LineTo(hdc, cx + d / 5, t + d * 4 / 10);
                LineTo(hdc, cx + d / 5, t + d / 6);
                MoveToEx(hdc, l + d * 3 / 10, b - d / 6, None);
                LineTo(hdc, l + d * 3 / 10, cy + d / 10);
                LineTo(hdc, rt - d * 3 / 10, cy + d / 10);
                LineTo(hdc, rt - d * 3 / 10, b - d / 6);
            }
            IconKind::Undo | IconKind::Redo => {
                let flip = if kind == IconKind::Undo { 1 } else { -1 };
                let y0 = t + d * 3 / 10;
                MoveToEx(hdc, cx + flip * d * 3 / 10, b - d * 25 / 100, None);
                LineTo(hdc, cx + flip * d * 3 / 10, cy);
                LineTo(hdc, cx, cy);
                LineTo(hdc, cx - flip * d * 35 / 100, cy);
                LineTo(hdc, cx - flip * d * 3 / 10 - flip * d * 12 / 100, y0);
                MoveToEx(hdc, cx - flip * d * 35 / 100, cy, None);
                LineTo(hdc, cx - flip * d * 25 / 100, b - d * 3 / 10);
            }
            IconKind::Find => {
                let rad = d * 3 / 10;
                let ccx = cx - d / 8;
                let ccy = cy - d / 8;
                for i in 0..=12 {
                    let a = std::f64::consts::PI * 2.0 * i as f64 / 12.0;
                    let x = ccx + (rad as f64 * a.cos()) as i32;
                    let y = ccy + (rad as f64 * a.sin()) as i32;
                    if i == 0 { MoveToEx(hdc, x, y, None); } else { LineTo(hdc, x, y); }
                }
                MoveToEx(hdc, ccx + rad * 7 / 10, ccy + rad * 7 / 10, None);
                LineTo(hdc, rt - d / 6, b - d / 6);
            }
            IconKind::Wrap => {
                MoveToEx(hdc, l + d / 6, t + d * 25 / 100, None);
                LineTo(hdc, rt - d * 3 / 10, t + d * 25 / 100);
                LineTo(hdc, rt - d * 3 / 10, cy);
                LineTo(hdc, cx - d * 15 / 100, cy);
                LineTo(hdc, cx - d * 5 / 100, cy - d * 15 / 100);
                MoveToEx(hdc, cx - d * 15 / 100, cy, None);
                LineTo(hdc, cx - d * 5 / 100, cy + d * 15 / 100);
                MoveToEx(hdc, l + d / 6, b - d * 25 / 100, None);
                LineTo(hdc, rt - d * 3 / 10, b - d * 25 / 100);
            }
            IconKind::FontMinus | IconKind::FontPlus => {
                MoveToEx(hdc, cx - d * 2 / 10, b - d * 2 / 10, None);
                LineTo(hdc, cx, t + d * 15 / 100);
                LineTo(hdc, cx + d * 2 / 10, b - d * 2 / 10);
                MoveToEx(hdc, cx - d * 1 / 10, cy, None);
                LineTo(hdc, cx + d * 1 / 10, cy);
                let sx = rt - d * 15 / 100;
                let sy = t + d * 3 / 10;
                MoveToEx(hdc, sx - d * 15 / 100, sy, None);
                LineTo(hdc, sx + d * 15 / 100, sy);
                if kind == IconKind::FontPlus {
                    MoveToEx(hdc, sx, sy - d * 15 / 100, None);
                    LineTo(hdc, sx, sy + d * 15 / 100);
                }
            }
            IconKind::Menu => {
                for i in 0..3 {
                    let y = t + d * (2 + i * 4) / 10;
                    MoveToEx(hdc, l + d / 6, y, None);
                    LineTo(hdc, rt - d / 6, y);
                }
            }
        }
        SelectObject(hdc, old);
        DeleteObject(pen);
    }
}

// ---------------- 公共：鼠标离开跟踪 ----------------

fn track_leave(hwnd: HWND) {
    unsafe {
        let mut tme = TRACKMOUSEEVENT {
            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        TrackMouseEvent(&mut tme);
    }
}

fn lp_point(lp: LPARAM) -> (i32, i32) {
    ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32)
}

// ---------------- 工具栏 ----------------

unsafe fn toolbar_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint_toolbar(hwnd);
            LRESULT(0)
        }
        wam::WM_MOUSEMOVE => {
            let (mx, my) = lp_point(lp);
            with_app_mut(|app| {
                let idx = toolbar_button_at(app, mx, my);
                if app.toolbar_hover != idx {
                    app.toolbar_hover = idx;
                    app.toolbar_pressed = -1;
                    unsafe { let _ = InvalidateRect(hwnd, None, false); }
                }
                track_leave(hwnd);
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with_app_mut(|app| {
                app.toolbar_hover = -1;
                app.toolbar_pressed = -1;
                unsafe { let _ = InvalidateRect(hwnd, None, false); }
            });
            LRESULT(0)
        }
        wam::WM_LBUTTONDOWN => {
            let (mx, my) = lp_point(lp);
            with_app_mut(|app| {
                app.toolbar_pressed = toolbar_button_at(app, mx, my);
                unsafe { let _ = InvalidateRect(hwnd, None, false); }
            });
            LRESULT(0)
        }
        wam::WM_LBUTTONUP => {
            let (mx, my) = lp_point(lp);
            with_app_mut(|app| {
                let idx = toolbar_button_at(app, mx, my);
                let was = app.toolbar_pressed;
                app.toolbar_pressed = -1;
                unsafe { let _ = InvalidateRect(hwnd, None, false); }
                if was == idx && idx >= 0 && (idx as usize) < TOOLBAR_BTNS.len() {
                    let id = TOOLBAR_BTNS[idx as usize].id;
                    if id == CMD_MENU {
                        let r = toolbar_button_rect(app, idx as usize);
                        let mut pt = POINT { x: r.left, y: r.bottom };
                        ClientToScreen(hwnd, &mut pt);
                        app.show_main_menu_at(pt.x, pt.y);
                    } else {
                        app.do_command(id);
                    }
                }
            });
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn paint_toolbar(hwnd: HWND) {
    with_app_try(|app| unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        FillRect(hdc, &rc, brush(CLR_CHROME));
        let sep = RECT { top: rc.bottom - 1, bottom: rc.bottom, ..Default::default() };
        FillRect(hdc, &sep, brush(CLR_BORDER));

        let d = dp(app, 16);
        let pad = dp(app, 8);
        let bw = dp(app, 34);
        let bh = rc.bottom - dp(app, 6);
        let y0 = dp(app, 3);

        for (i, btn) in TOOLBAR_BTNS.iter().enumerate() {
            let x = pad + i as i32 * bw;
            if btn.sep_before {
                let sep_r = RECT { left: x - dp(app, 5), right: x - dp(app, 4), top: y0 + dp(app, 6), bottom: y0 + bh - dp(app, 6) };
                FillRect(hdc, &sep_r, brush(CLR_BORDER));
            }
            let enabled = toolbar_button_enabled(app, btn.id);
            let hovered = app.toolbar_hover == i as i32;
            let pressed = app.toolbar_pressed == i as i32;
            let toggled = toolbar_button_toggled(app, btn.id);

            if enabled && (hovered || pressed || toggled) {
                let bg = if pressed { CLR_PRESSED } else if toggled { CLR_ACCENT_BG } else { CLR_HOVER };
                let hbr = brush(bg);
                let hpen = CreatePen(PS_SOLID, 0, cr(bg));
                let old_br = SelectObject(hdc, hbr);
                let old_pen = SelectObject(hdc, hpen);
                RoundRect(hdc, x, y0, x + bw - dp(app, 2), y0 + bh, dp(app, 6), dp(app, 6));
                SelectObject(hdc, old_br);
                SelectObject(hdc, old_pen);
                DeleteObject(hbr);
                DeleteObject(hpen);
            }

            let color = if !enabled {
                CLR_DISABLED
            } else if pressed || toggled {
                CLR_ACCENT
            } else {
                CLR_TEXT
            };
            draw_icon(hdc, btn.icon, x + (bw - dp(app, 2)) / 2, y0 + bh / 2, d, color);
        }
        let _ = EndPaint(hwnd, &ps);
    });
}

// ---------------- 标签栏 ----------------

unsafe fn tabbar_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint_tabbar(hwnd);
            LRESULT(0)
        }
        wam::WM_MOUSEMOVE => {
            let (mx, my) = lp_point(lp);
            with_app_mut(|app| {
                let hit = tabbar_hit(app, mx, my);
                if app.tabbar_hover != hit {
                    app.tabbar_hover = hit;
                    unsafe { let _ = InvalidateRect(hwnd, None, false); }
                }
                track_leave(hwnd);
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with_app_mut(|app| {
                app.tabbar_hover = -1;
                unsafe { let _ = InvalidateRect(hwnd, None, false); }
            });
            LRESULT(0)
        }
        wam::WM_LBUTTONDOWN => {
            let (mx, my) = lp_point(lp);
            with_app_mut(|app| app.tabbar_click(mx, my));
            LRESULT(0)
        }
        WM_MBUTTONDOWN | wam::WM_LBUTTONDBLCLK => {
            let (mx, my) = lp_point(lp);
            with_app_mut(|app| {
                if let Some(i) = tabbar_hit_tab(app, mx, my) {
                    app.close_tab(i);
                }
            });
            LRESULT(0)
        }
        wam::WM_CONTEXTMENU => {
            let (mx, my) = lp_point(lp);
            let mut pt = POINT { x: mx, y: my };
            ScreenToClient(hwnd, &mut pt);
            with_app_mut(|app| app.show_tab_menu(pt.x, pt.y));
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// 圆角填充矩形（画笔与画刷同色，得到纯实心圆角；radius 为圆角直径）
unsafe fn round_fill_rect(hdc: HDC, r: &RECT, color: u32, radius: i32) {
    let hbr = brush(color);
    let hpen = CreatePen(PS_SOLID, 0, cr(color));
    let ob = SelectObject(hdc, hbr);
    let op = SelectObject(hdc, hpen);
    RoundRect(hdc, r.left, r.top, r.right, r.bottom, radius, radius);
    SelectObject(hdc, ob);
    SelectObject(hdc, op);
    DeleteObject(hbr);
    DeleteObject(hpen);
}

pub fn paint_tabbar(hwnd: HWND) {
    with_app_try(|app| unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        FillRect(hdc, &rc, brush(CLR_CHROME));
        let sep = RECT { top: rc.bottom - 1, bottom: rc.bottom, ..Default::default() };
        FillRect(hdc, &sep, brush(CLR_BORDER));

        let th = rc.bottom;
        let mut x = -app.tab_scroll;
        for (i, tab) in app.tabs.iter().enumerate() {
            let active = i == app.active;
            let hovered = app.tabbar_hover == i as i32;
            let tw = tab_width(app, tab);
            let label = if tab.dirty { format!("● {}", tab.display) } else { tab.display.clone() };

            if active {
                let r = RECT { left: x, top: dp(app, 4), right: x + tw, bottom: rc.bottom - dp(app, 8) };
                round_fill_rect(hdc, &r, CLR_TAB_ACTIVE, dp(app, 14));
                let line = RECT { left: x + dp(app, 6), right: x + tw - dp(app, 6), top: rc.bottom - dp(app, 10), bottom: rc.bottom - dp(app, 8) };
                round_fill_rect(hdc, &line, CLR_ACCENT, dp(app, 3));
            } else if hovered {
                let r = RECT { left: x, top: dp(app, 4), right: x + tw, bottom: rc.bottom - dp(app, 8) };
                round_fill_rect(hdc, &r, CLR_HOVER, dp(app, 14));
            }

            SetTextColor(hdc, cr(if active { CLR_TEXT } else { CLR_TEXT_SUB }));
            SetBkMode(hdc, TRANSPARENT);
            let mut text_rect = RECT {
                left: x + dp(app, 12),
                top: dp(app, 3),
                right: x + tw - dp(app, 22),
                bottom: rc.bottom - dp(app, 3),
            };
            let mut buf: Vec<u16> = label.encode_utf16().collect();
            DrawTextW(hdc, &mut buf, &mut text_rect, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_LEFT);

            if active || hovered {
                let ccx = x + tw - dp(app, 12);
                let ccy = th / 2;
                let s = dp(app, 4);
                let pen = CreatePen(PS_SOLID, std::cmp::max(1, dp(app, 1)), cr(CLR_TEXT_SUB));
                let old = SelectObject(hdc, pen);
                MoveToEx(hdc, ccx - s, ccy - s, None);
                LineTo(hdc, ccx + s, ccy + s);
                MoveToEx(hdc, ccx + s, ccy - s, None);
                LineTo(hdc, ccx - s, ccy + s);
                SelectObject(hdc, old);
                DeleteObject(pen);
            }
            x += tw;
        }

        // 「+」新建按钮
        let plus_cx = x + dp(app, 16);
        let plus_cy = th / 2;
        if app.tabbar_hover == 9999 {
            let r = RECT { left: plus_cx - dp(app, 14), top: dp(app, 6), right: plus_cx + dp(app, 14), bottom: th - dp(app, 6) };
            round_fill_rect(hdc, &r, CLR_HOVER, dp(app, 14));
        }
        draw_icon(hdc, IconKind::FontPlus, plus_cx, plus_cy, dp(app, 10), CLR_TEXT_SUB);

        let _ = EndPaint(hwnd, &ps);
    });
}

// ---------------- 状态栏 ----------------

unsafe fn status_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint_status(hwnd);
            LRESULT(0)
        }
        wam::WM_LBUTTONDOWN => {
            let (mx, _) = lp_point(lp);
            with_app_mut(|app| app.status_click(mx));
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub fn paint_status(hwnd: HWND) {
    with_app_try(|app| unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        FillRect(hdc, &rc, brush(CLR_CHROME));
        let sep = RECT { top: rc.top, bottom: rc.top + 1, ..Default::default() };
        FillRect(hdc, &sep, brush(CLR_BORDER));

        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, cr(CLR_TEXT_SUB));
        let pad = dp(app, 10);
        let mut x = rc.left + pad;
        for (text, clickable) in app.status_segments() {
            let mut wide = text.encode_utf16().collect::<Vec<u16>>();
            let mut calc = RECT::default();
            DrawTextW(hdc, &mut wide, &mut calc, DT_SINGLELINE | DT_CALCRECT);
            let mut tr = RECT { left: x, top: rc.top, right: x + calc.right + dp(app, 4), bottom: rc.bottom };
            DrawTextW(hdc, &mut wide, &mut tr, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_LEFT);
            if clickable {
                let line = RECT { left: x, right: x + calc.right, top: rc.bottom - dp(app, 3), bottom: rc.bottom - dp(app, 2) };
                FillRect(hdc, &line, brush(CLR_ACCENT));
            }
            x += calc.right + dp(app, if clickable { 18 } else { 14 });
        }
        let _ = EndPaint(hwnd, &ps);
    });
}

// ---------------- 查找条（底部嵌入式） ----------------

unsafe fn findbar_proc(_hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            // 子控件通知上浮到主窗口统一处理
            with_app_try(|app| unsafe {
                SendMessageW(app.hwnd, WM_COMMAND, wp, lp);
            });
            LRESULT(0)
        }
        _ => DefWindowProcW(_hwnd, msg, wp, lp),
    }
}

pub fn create_findbar(app: &mut App) {
    let fb = create_ui_child(app, CHILD_FINDBAR, w!("Chit.FindBar"), "");
    app.findbar = fb;
    let ui_font = app.ui_font;
    let hinst = app.hinst;

    let mk_edit = || unsafe {
        CreateWindowExW(
            wam::WS_EX_CLIENTEDGE,
            w!("EDIT"),
            PCWSTR::null(),
            WS_CHILD,
            0, 0, 0, 0,
            fb, wam::HMENU::default(), hinst, None,
        )
        .expect("创建查找输入框失败")
    };
    let mk_static = |text: &str| unsafe {
        let t = to_wide(text);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            PCWSTR(t.as_ptr()),
            WS_CHILD | WS_VISIBLE,
            0, 0, 0, 0,
            fb, wam::HMENU::default(), hinst, None,
        )
        .expect("创建静态文本失败")
    };
    let mk_btn = |text: &str| unsafe {
        let t = to_wide(text);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BUTTON"),
            PCWSTR(t.as_ptr()),
            WS_CHILD | WS_VISIBLE,
            0, 0, 0, 0,
            fb, wam::HMENU::default(), hinst, None,
        )
        .expect("创建按钮失败")
    };

    app.find_edit = mk_edit();
    app.find_count = mk_static("");
    app.btn_prev = mk_btn("上一个");
    app.btn_next = mk_btn("下一个");
    app.btn_case = mk_btn("区分大小写");
    app.replace_edit = mk_edit();
    app.btn_replace = mk_btn("替换");
    app.btn_replace_all = mk_btn("全部替换");
    app.btn_close = mk_btn("×");

    for h in [app.find_edit, app.replace_edit, app.find_count, app.btn_prev, app.btn_next, app.btn_case, app.btn_replace, app.btn_replace_all, app.btn_close] {
        unsafe {
            SendMessageW(h, wam::WM_SETFONT, WPARAM(ui_font.0 as usize), LPARAM(1));
        }
    }
    set_cue_banner(app.find_edit, "查找内容（回车找下一个）");
    set_cue_banner(app.replace_edit, "替换为");
}

fn set_cue_banner(hwnd: HWND, text: &str) {
    let t = to_wide(text);
    unsafe {
        SendMessageW(hwnd, 0x1501 /*EM_SETCUEBANNER*/, WPARAM(1), LPARAM(t.as_ptr() as isize));
    }
}

pub const STATUS_H: i32 = 24;

pub fn layout_findbar(app: &App) {
    unsafe {
        let mut rc = RECT::default();
        let _ = GetClientRect(app.hwnd, &mut rc);
        let dp_ = |v: i32| dp(app, v);
        let fb_h = dp_(34);
        let y = rc.bottom - fb_h - dp_(STATUS_H);
        let _ = SetWindowPos(app.findbar, HWND::default(), 0, y, rc.right, fb_h, SWP_NOZORDER | SWP_NOACTIVATE);

        let m = dp_(6);
        let btn_h = dp_(24);
        let cy = (fb_h - btn_h) / 2;
        let mut x = m;
        let find_w = dp_(180);
        let _ = SetWindowPos(app.find_edit, HWND::default(), x, cy, find_w, btn_h, SWP_NOZORDER | SWP_NOACTIVATE);
        let _ = ShowWindow(app.find_edit, SW_SHOW);
        x += find_w + m;
        let cnt_w = dp_(90);
        let _ = SetWindowPos(app.find_count, HWND::default(), x, cy, cnt_w, btn_h, SWP_NOZORDER | SWP_NOACTIVATE);
        let _ = ShowWindow(app.find_count, SW_SHOW);
        x += cnt_w + m;
        let bw = dp_(60);
        for h in [app.btn_prev, app.btn_next] {
            let _ = SetWindowPos(h, HWND::default(), x, cy, bw, btn_h, SWP_NOZORDER | SWP_NOACTIVATE);
            let _ = ShowWindow(h, SW_SHOW);
            x += bw + m;
        }
        let case_w = dp_(86);
        let _ = SetWindowPos(app.btn_case, HWND::default(), x, cy, case_w, btn_h, SWP_NOZORDER | SWP_NOACTIVATE);
        let _ = ShowWindow(app.btn_case, SW_SHOW);
        x += case_w + m;
        if app.find_replace_visible {
            let rw = dp_(170);
            let _ = SetWindowPos(app.replace_edit, HWND::default(), x, cy, rw, btn_h, SWP_NOZORDER | SWP_NOACTIVATE);
            let _ = ShowWindow(app.replace_edit, SW_SHOW);
            x += rw + m;
            for h in [app.btn_replace, app.btn_replace_all] {
                let _ = SetWindowPos(h, HWND::default(), x, cy, bw + dp_(14), btn_h, SWP_NOZORDER | SWP_NOACTIVATE);
                let _ = ShowWindow(h, SW_SHOW);
                x += bw + dp_(14) + m;
            }
        } else {
            for h in [app.replace_edit, app.btn_replace, app.btn_replace_all] {
                let _ = ShowWindow(h, SW_HIDE);
            }
        }
        let cx = dp_(26);
        let _ = SetWindowPos(app.btn_close, HWND::default(), rc.right - cx - m, cy, cx, btn_h, SWP_NOZORDER | SWP_NOACTIVATE);
        let _ = ShowWindow(app.btn_close, SW_SHOW);
    }
    let _ = tab_width; // 保留引用避免未用告警
}
