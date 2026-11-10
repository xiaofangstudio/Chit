//! app.rs —— 主窗口、命令表、菜单、标签管理、查找替换
//! 架构约束（docs/03）：所有动作走统一命令表。

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_ROUND, DWMSBT_MAINWINDOW,
};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, GetDC, GetDeviceCaps, GetMonitorInfoW, GetTextExtentPoint32W, InvalidateRect,
    MonitorFromWindow, ReleaseDC, HFONT, LOGFONTW, MONITOR_DEFAULTTONEAREST, MONITORINFO,
};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    EM_CANUNDO, EM_GETSEL, EM_LINEFROMCHAR, EM_LINEINDEX, EM_LINELENGTH, EM_REPLACESEL,
    EM_SCROLLCARET, EM_SETMODIFY, EM_SETREADONLY, EM_SETSEL, EM_UNDO, NMHDR, TTTOOLINFOW,
    TTF_SUBCLASS,
};
const EN_SELCHANGE_U: u32 = 1794; // EN_SELCHANGE（0x0701）
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::Shell::{
    DefSubclassProc, DragAcceptFiles, DragFinish, DragQueryFileW, RemoveWindowSubclass,
    SetWindowSubclass, ShellExecuteW, HDROP,
};
use windows::Win32::UI::WindowsAndMessaging as wam;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateAcceleratorTableW, CreateIconFromResourceEx, CreatePopupMenu,
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnableMenuItem,
    FindWindowW, GetCursorPos, GetDlgItem, GetMessageW, GetWindowPlacement,
    GetWindowLongPtrW, LoadCursorW, MessageBoxW, PostQuitMessage, RegisterClassExW,
    SendMessageW, SetForegroundWindow, SetWindowPlacement, SetWindowPos, SetWindowTextW,
    ShowWindow, TrackPopupMenu, TranslateAcceleratorW, TranslateMessage,
    ACCEL, CW_USEDEFAULT, FALT, FCONTROL, FSHIFT, FVIRTKEY, HACCEL, HICON, HMENU,
    MB_ICONQUESTION, MB_ICONWARNING, MB_OK, MB_YESNOCANCEL, MF_BYCOMMAND, MF_CHECKED,
    MF_DISABLED, MF_ENABLED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MESSAGEBOX_RESULT,
    MESSAGEBOX_STYLE, MSG, SWP_NOACTIVATE, SWP_NOZORDER,
    SW_HIDE, SW_SHOW, SW_SHOWNORMAL, TPM_RIGHTBUTTON, WINDOWPLACEMENT, WM_CLOSE, WM_COMMAND,
    WM_CONTEXTMENU, WM_DESTROY, WM_DPICHANGED, WM_DROPFILES, WM_GETMINMAXINFO, WM_NOTIFY,
    WM_SETFONT, WM_SIZE, WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_OVERLAPPEDWINDOW,
    WS_POPUP, WS_SYSMENU, WS_VISIBLE, IDNO, IDYES,
};

use crate::consts::*;
use crate::edit::{self, Eol, Tab};
use crate::encoding::{self, Encoding, ALL_ENCODINGS};
use crate::fileio;
use crate::ui;

// ================= 应用状态 =================

pub struct App {
    pub hwnd: HWND,
    pub hinst: windows::Win32::Foundation::HINSTANCE,
    pub toolbar: HWND,
    pub tabbar: HWND,
    pub status: HWND,
    pub findbar: HWND,
    pub find_edit: HWND,
    pub find_count: HWND,
    pub replace_edit: HWND,
    pub btn_prev: HWND,
    pub btn_next: HWND,
    pub btn_case: HWND,
    pub btn_replace: HWND,
    pub btn_replace_all: HWND,
    pub btn_close: HWND,

    pub tabs: Vec<Tab>,
    pub active: usize,

    pub dpi: u32,
    pub ui_font: HFONT,
    pub text_font: HFONT,
    pub font_size: i32,
    pub wrap: bool,
    pub readonly: bool,
    pub fullscreen: bool,
    pub saved_placement: Option<WINDOWPLACEMENT>,

    pub find_case: bool,
    pub find_replace_visible: bool,
    pub find_visible: bool,

    pub recents: Vec<String>,
    pub cfg_text: String,

    pub toolbar_hover: i32,
    pub toolbar_pressed: i32,
    pub tabbar_hover: i32,
    pub tab_scroll: i32,
    pub accel: Option<HACCEL>,
    pub tooltip_texts: Vec<Vec<u16>>,
    pub tooltip_wnd: Option<HWND>,
}

pub const TABBAR_PLUS: i32 = 9999;

// ================= 线程局部分发 =================

thread_local! {
    static APP: RefCell<Option<Rc<RefCell<App>>>> = const { RefCell::new(None) };
}

pub fn set_app(app: Rc<RefCell<App>>) {
    APP.with(|slot| *slot.borrow_mut() = Some(app));
}

/// 主访问入口（可变借用）
pub fn with_app_mut<R>(f: impl FnOnce(&mut App) -> R) -> R {
    APP.with(|slot| {
        let rc = slot.borrow().as_ref().expect("App 未初始化").clone();
        let mut b = rc.borrow_mut();
        f(&mut b)
    })
}

/// 绘制/重入场景：拿不到借用就跳过
pub fn with_app_try<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|slot| {
        let rc = slot.borrow().as_ref()?.clone();
        let mut b = rc.try_borrow_mut().ok()?;
        Some(f(&mut b))
    })
}

// ================= 工具栏定义 =================

pub struct ToolButton {
    pub id: u32,
    pub icon: ui::IconKind,
    pub sep_before: bool,
    pub tooltip: &'static str,
}

pub const TOOLBAR_BTNS: [ToolButton; 10] = [
    ToolButton { id: CMD_NEW, icon: ui::IconKind::New, sep_before: false, tooltip: "新建（Ctrl+N）" },
    ToolButton { id: CMD_OPEN, icon: ui::IconKind::Open, sep_before: false, tooltip: "打开（Ctrl+O）" },
    ToolButton { id: CMD_SAVE, icon: ui::IconKind::Save, sep_before: false, tooltip: "保存（Ctrl+S）" },
    ToolButton { id: CMD_UNDO, icon: ui::IconKind::Undo, sep_before: true, tooltip: "撤销（Ctrl+Z）" },
    ToolButton { id: CMD_REDO, icon: ui::IconKind::Redo, sep_before: false, tooltip: "重做（Ctrl+Y）" },
    ToolButton { id: CMD_FIND, icon: ui::IconKind::Find, sep_before: true, tooltip: "查找（Ctrl+F）" },
    ToolButton { id: CMD_WRAP, icon: ui::IconKind::Wrap, sep_before: true, tooltip: "自动换行" },
    ToolButton { id: CMD_ZOOM_OUT, icon: ui::IconKind::FontMinus, sep_before: true, tooltip: "缩小字号（Ctrl+减号）" },
    ToolButton { id: CMD_ZOOM_IN, icon: ui::IconKind::FontPlus, sep_before: false, tooltip: "放大字号（Ctrl+加号）" },
    ToolButton { id: CMD_MENU, icon: ui::IconKind::Menu, sep_before: true, tooltip: "更多" },
];

pub fn toolbar_button_at(app: &App, x: i32, _y: i32) -> i32 {
    let pad = ui::dp(app, 8);
    let bw = ui::dp(app, 34);
    let idx = (x - pad) / bw;
    if idx >= 0 && idx < TOOLBAR_BTNS.len() as i32 {
        idx
    } else {
        -1
    }
}

pub fn toolbar_button_rect(app: &App, index: usize) -> RECT {
    let pad = ui::dp(app, 8);
    let bw = ui::dp(app, 34);
    let h = ui::dp(app, 34);
    RECT {
        left: pad + index as i32 * bw,
        top: ui::dp(app, 3),
        right: pad + (index as i32 + 1) * bw,
        bottom: ui::dp(app, 3) + h,
    }
}

pub fn toolbar_button_enabled(app: &App, id: u32) -> bool {
    match id {
        CMD_UNDO => app.tabs.get(app.active).map(edit::can_undo).unwrap_or(false),
        CMD_REDO => app.tabs.get(app.active).map(edit::can_redo).unwrap_or(false),
        _ => true,
    }
}

pub fn toolbar_button_toggled(app: &App, id: u32) -> bool {
    match id {
        CMD_WRAP => app.wrap,
        _ => false,
    }
}

// ================= 标签栏命中测试 =================

pub fn tab_width(app: &App, tab: &Tab) -> i32 {
    unsafe {
        let hdc = GetDC(app.tabbar);
        let old = windows::Win32::Graphics::Gdi::SelectObject(hdc, app.ui_font);
        let label = if tab.dirty {
            format!("● {}", tab.display)
        } else {
            tab.display.clone()
        };
        let wide: Vec<u16> = label.encode_utf16().collect();
        let mut size = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &wide, &mut size);
        windows::Win32::Graphics::Gdi::SelectObject(hdc, old);
        let _ = ReleaseDC(app.tabbar, hdc);
        size.cx + ui::dp(app, 46)
    }
}

pub fn tabbar_hit_tab(app: &App, x: i32, _y: i32) -> Option<usize> {
    let mut cx = -app.tab_scroll;
    for (i, tab) in app.tabs.iter().enumerate() {
        let tw = tab_width(app, tab);
        if x >= cx && x < cx + tw {
            return Some(i);
        }
        cx += tw;
    }
    None
}

pub fn tabbar_hit(app: &App, x: i32, y: i32) -> i32 {
    if let Some(i) = tabbar_hit_tab(app, x, y) {
        i as i32
    } else {
        let mut cx = -app.tab_scroll;
        for tab in &app.tabs {
            cx += tab_width(app, tab);
        }
        if x >= cx && x <= cx + ui::dp(app, 30) && y > 0 && y < ui::dp(app, 30) {
            TABBAR_PLUS
        } else {
            -1
        }
    }
}

impl App {
    /// 鼠标坐标是否落在第 i 个标签的关闭按钮上
    pub fn tab_close_hit(&self, i: usize, x: i32) -> bool {
        let mut cx = -self.tab_scroll;
        for (j, tab) in self.tabs.iter().enumerate() {
            let tw = tab_width(self, tab);
            if j == i {
                let close_cx = cx + tw - ui::dp(self, 12);
                return (x - close_cx).abs() <= ui::dp(self, 8);
            }
            cx += tw;
        }
        false
    }

    pub fn tabbar_click(&mut self, x: i32, y: i32) {
        if let Some(i) = tabbar_hit_tab(self, x, y) {
            if self.tab_close_hit(i, x) {
                self.close_tab(i);
                return;
            }
            self.activate(i);
            return;
        }
        if tabbar_hit(self, x, y) == TABBAR_PLUS {
            self.new_tab();
        }
    }
}

// ================= 状态栏 =================

impl App {
    pub fn status_segments(&self) -> Vec<(String, bool)> {
        let Some(tab) = self.tabs.get(self.active) else {
            return vec![("就绪".to_string(), false)];
        };
        unsafe {
            let (sel_s, _sel_e) = edit::get_sel(tab.edit);
            let line = SendMessageW(tab.edit, EM_LINEFROMCHAR, WPARAM(usize::MAX), LPARAM(0)).0 as i32;
            let line_start = SendMessageW(tab.edit, EM_LINEINDEX, WPARAM(line as usize), LPARAM(0)).0 as i32;
            let col = sel_s - line_start;
            let chars = wam::GetWindowTextLengthW(tab.edit);
            vec![
                (format!("行 {}, 列 {}", line + 1, col + 1), true),
                (format!("{} 字符", chars), false),
                (tab.encoding.name().to_string(), true),
                (tab.eol.name().to_string(), true),
            ]
        }
    }

    pub fn status_click(&mut self, x: i32) {
        unsafe {
            let mut rc = RECT::default();
            let _ = wam::GetClientRect(self.status, &mut rc);
            let hdc = GetDC(self.status);
            windows::Win32::Graphics::Gdi::SelectObject(hdc, self.ui_font);
            let pad = ui::dp(self, 10);
            let mut sx = rc.left + pad;
            let mut hits: Vec<(RECT, u32)> = Vec::new();
            for (i, (text, clickable)) in self.status_segments().into_iter().enumerate() {
                let wide: Vec<u16> = text.encode_utf16().collect();
                let mut size = SIZE::default();
                let _ = GetTextExtentPoint32W(hdc, &wide, &mut size);
                let seg_w = size.cx + ui::dp(self, if clickable { 18 } else { 14 });
                if clickable {
                    hits.push((RECT { left: sx, right: sx + seg_w, top: 0, bottom: rc.bottom }, i as u32));
                }
                sx += seg_w;
            }
            let _ = ReleaseDC(self.status, hdc);
            for (r, which) in hits {
                if x >= r.left && x < r.right {
                    match which {
                        0 => self.goto_dialog(),
                        2 => {
                            let (px, py) = self.status_screen_pos();
                            self.show_encoding_menu(px, py);
                        }
                        3 => {
                            let (px, py) = self.status_screen_pos();
                            self.show_eol_menu(px, py);
                        }
                        _ => {}
                    }
                    return;
                }
            }
        }
    }

    fn status_screen_pos(&self) -> (i32, i32) {
        unsafe {
            let mut rc = RECT::default();
            let _ = wam::GetClientRect(self.status, &mut rc);
            let mut pt = POINT { x: rc.left + ui::dp(self, 10), y: rc.bottom / 2 };
            let _ = ClientToScreen(self.status, &mut pt);
            (pt.x, pt.y)
        }
    }
}

// ================= 创建 =================

pub fn create_app() -> Rc<RefCell<App>> {
    unsafe {
        let hinst = GetModuleHandleW(None).expect("获取模块句柄失败");
        ui::register_ui_classes();

        let (icon_big, icon_small) = load_app_icon();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(main_wndproc),
            hInstance: hinst.into(),
            hIcon: icon_big,
            hIconSm: icon_small,
            hCursor: LoadCursorW(windows::Win32::Foundation::HINSTANCE::default(), wam::IDC_ARROW).expect("加载光标失败"),
            lpszClassName: w!("Chit.Main"),
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let dpi = initial_dpi();
        let hwnd = CreateWindowExW(
            wam::WINDOW_EX_STYLE(0),
            w!("Chit.Main"),
            w!("Chit"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT,
            HWND::default(),
            HMENU::default(),
            hinst,
            None,
        )
        .expect("创建主窗口失败");

        // Win11：整窗圆角 + 标题栏云母（Mica）磨砂材质（Win10 上静默失败）
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const core::ffi::c_void,
            4,
        );
        let backdrop = DWMSBT_MAINWINDOW;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as *const core::ffi::c_void,
            4,
        );

        DragAcceptFiles(hwnd, true);

        let mut app = App {
            hwnd,
            hinst: hinst.into(),
            toolbar: HWND::default(),
            tabbar: HWND::default(),
            status: HWND::default(),
            findbar: HWND::default(),
            find_edit: HWND::default(),
            find_count: HWND::default(),
            replace_edit: HWND::default(),
            btn_prev: HWND::default(),
            btn_next: HWND::default(),
            btn_case: HWND::default(),
            btn_replace: HWND::default(),
            btn_replace_all: HWND::default(),
            btn_close: HWND::default(),
            tabs: Vec::new(),
            active: 0,
            dpi,
            ui_font: HFONT::default(),
            text_font: HFONT::default(),
            font_size: FONT_DEFAULT,
            wrap: true,
            readonly: false,
            fullscreen: false,
            saved_placement: None,
            find_case: false,
            find_replace_visible: false,
            find_visible: false,
            recents: Vec::new(),
            cfg_text: fileio::load_config(),
            toolbar_hover: -1,
            toolbar_pressed: -1,
            tabbar_hover: -1,
            tab_scroll: 0,
            accel: None,
            tooltip_texts: Vec::new(),
            tooltip_wnd: None,
        };

        // 读取配置
        if let Some(v) = fileio::ini_get(&app.cfg_text, "视图", "字号") {
            if let Ok(n) = v.parse::<i32>() {
                if (FONT_MIN..=FONT_MAX).contains(&n) {
                    app.font_size = n;
                }
            }
        }
        if let Some(v) = fileio::ini_get(&app.cfg_text, "视图", "自动换行") {
            app.wrap = v != "0";
        }
        app.recents = fileio::load_recents(&app.cfg_text);

        app.create_fonts();
        app.toolbar = ui::create_ui_child(&mut app, ui::CHILD_TOOLBAR, w!("Chit.Toolbar"), "");
        app.tabbar = ui::create_ui_child(&mut app, ui::CHILD_TABBAR, w!("Chit.Tabbar"), "");
        app.status = ui::create_ui_child(&mut app, ui::CHILD_STATUS, w!("Chit.Status"), "");
        ui::create_findbar(&mut app);
        let _ = ShowWindow(app.findbar, SW_HIDE);
        app.add_tooltips();
        app.build_accel();

        let rc = Rc::new(RefCell::new(app));
        set_app(rc.clone());
        rc
    }
}

fn initial_dpi() -> u32 {
    unsafe {
        let hdc = GetDC(HWND::default());
        let dpi = GetDeviceCaps(hdc, windows::Win32::Graphics::Gdi::LOGPIXELSX);
        let _ = ReleaseDC(HWND::default(), hdc);
        dpi.max(96) as u32
    }
}

/// 从嵌入的 ico 数据加载图标（不用 .rc 编译）
fn load_app_icon() -> (HICON, HICON) {
    const ICON_BYTES: &[u8] = include_bytes!("../res/chit.ico");
    unsafe {
        let big = CreateIconFromResourceEx(
            ICON_BYTES,
            true,
            0x00030000,
            32, 32,
            wam::LR_DEFAULTCOLOR,
        )
        .unwrap_or_default();
        let small = CreateIconFromResourceEx(
            ICON_BYTES,
            true,
            0x00030000,
            16, 16,
            wam::LR_DEFAULTCOLOR,
        )
        .unwrap_or(big);
        (big, small)
    }
}

fn set_face(lf: &mut LOGFONTW, face: &str) {
    for (i, c) in face.encode_utf16().take(31).enumerate() {
        lf.lfFaceName[i] = c;
    }
}

impl App {
    pub fn create_fonts(&mut self) {
        unsafe {
            let mut lf = LOGFONTW::default();
            lf.lfHeight = -(9 * self.dpi as i32 + 48) / 72;
            lf.lfWeight = 400;
            set_face(&mut lf, "Microsoft YaHei");
            self.ui_font = windows::Win32::Graphics::Gdi::CreateFontIndirectW(&lf);
            let mut lf2 = LOGFONTW::default();
            lf2.lfHeight = -(self.font_size * self.dpi as i32 + 48) / 72;
            set_face(&mut lf2, "Microsoft YaHei");
            self.text_font = windows::Win32::Graphics::Gdi::CreateFontIndirectW(&lf2);
        }
    }

    pub fn set_text_font_all(&mut self) {
        unsafe {
            for tab in &self.tabs {
                SendMessageW(tab.edit, WM_SETFONT, WPARAM(self.text_font.0 as usize), LPARAM(1));
            }
        }
    }

    pub fn layout(&mut self) {
        unsafe {
            let mut rc = RECT::default();
            let _ = wam::GetClientRect(self.hwnd, &mut rc);
            let dp_ = |v: i32| ui::dp(self, v);
            let tb_h = dp_(40);
            let tab_h = dp_(34);
            let st_h = dp_(ui::STATUS_H);
            let fb_h = if self.find_visible { dp_(34) } else { 0 };

            let _ = SetWindowPos(self.toolbar, HWND::default(), 0, 0, rc.right, tb_h, SWP_NOZORDER);
            let _ = SetWindowPos(self.tabbar, HWND::default(), 0, tb_h, rc.right, tab_h, SWP_NOZORDER);
            let _ = SetWindowPos(self.status, HWND::default(), 0, rc.bottom - st_h, rc.right, st_h, SWP_NOZORDER);

            let top = tb_h + tab_h;
            let bottom = rc.bottom - st_h - fb_h;
            for (i, tab) in self.tabs.iter().enumerate() {
                if i == self.active {
                    let _ = SetWindowPos(tab.edit, HWND::default(), 0, top, rc.right, bottom - top, SWP_NOZORDER);
                    let _ = ShowWindow(tab.edit, SW_SHOW);
                } else {
                    let _ = ShowWindow(tab.edit, SW_HIDE);
                }
            }
            if self.find_visible {
                let _ = ShowWindow(self.findbar, SW_SHOW);
                ui::layout_findbar(self);
            } else {
                let _ = ShowWindow(self.findbar, SW_HIDE);
            }
        }
    }

    pub fn set_title(&mut self) {
        let title = match self.tabs.get(self.active) {
            Some(tab) => format!("{} — {}", tab.title(), APP_NAME),
            None => format!("{}", APP_NAME),
        };
        unsafe {
            let t = fileio::to_wide(&title);
            let _ = SetWindowTextW(self.hwnd, PCWSTR(t.as_ptr()));
        }
    }

    pub fn update_ui(&mut self) {
        self.set_title();
        unsafe {
            let _ = InvalidateRect(self.tabbar, None, false);
            let _ = InvalidateRect(self.status, None, false);
            let _ = InvalidateRect(self.toolbar, None, false);
        }
    }

    pub fn refresh_status(&mut self) {
        unsafe {
            let _ = InvalidateRect(self.status, None, false);
        }
    }

    // ---------- 工具提示 ----------
    fn add_tooltips(&mut self) {
        unsafe {
            let tt = CreateWindowExW(
                wam::WINDOW_EX_STYLE(wam::WS_EX_TOPMOST.0),
                windows::Win32::UI::Controls::TOOLTIPS_CLASS,
                PCWSTR::null(),
                wam::WS_POPUP | wam::WS_VISIBLE,
                0, 0, 0, 0,
                self.toolbar,
                HMENU::default(),
                self.hinst,
                None,
            )
            .ok();
            let Some(tt) = tt else { return };
            self.tooltip_wnd = Some(tt);
            for (i, btn) in TOOLBAR_BTNS.iter().enumerate() {
                let mut text: Vec<u16> = btn.tooltip.encode_utf16().collect();
                let rect = toolbar_button_rect(self, i);
                let ti = TTTOOLINFOW {
                    cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
                    uFlags: TTF_SUBCLASS,
                    hwnd: self.toolbar,
                    uId: i as usize,
                    rect,
                    hinst: self.hinst,
                    lpszText: PWSTR(text.as_mut_ptr()),
                    ..Default::default()
                };
                self.tooltip_texts.push(text);
                SendMessageW(tt, 0x0432 /*TTM_ADDTOOLW*/, WPARAM(0), LPARAM(&ti as *const _ as isize));
            }
        }
    }

    // ---------- 加速键表 ----------
    fn build_accel(&mut self) {
        let a = |virt: wam::ACCEL_VIRT_FLAGS, key: u16, cmd: u32| ACCEL { fVirt: virt, key, cmd: cmd as u16 };
        let table = [
            a(FVIRTKEY | FCONTROL, 'N' as u16, CMD_NEW),
            a(FVIRTKEY | FCONTROL, 'O' as u16, CMD_OPEN),
            a(FVIRTKEY | FCONTROL, 'S' as u16, CMD_SAVE),
            a(FVIRTKEY | FCONTROL | FSHIFT, 'S' as u16, CMD_SAVE_AS),
            a(FVIRTKEY | FCONTROL, 'W' as u16, CMD_CLOSE_TAB),
            a(FVIRTKEY | FCONTROL, 'Z' as u16, CMD_UNDO),
            a(FVIRTKEY | FCONTROL, 'Y' as u16, CMD_REDO),
            a(FVIRTKEY | FCONTROL | FSHIFT, 'Z' as u16, CMD_REDO),
            a(FVIRTKEY | FCONTROL, 'F' as u16, CMD_FIND),
            a(FVIRTKEY | FCONTROL, 'H' as u16, CMD_REPLACE),
            a(FVIRTKEY, 0x70, CMD_FIND_NEXT),          // F3
            a(FVIRTKEY | FSHIFT, 0x70, CMD_FIND_PREV), // Shift+F3
            a(FVIRTKEY | FCONTROL, 'G' as u16, CMD_GOTO),
            a(FVIRTKEY | FCONTROL | FSHIFT, 'K' as u16, CMD_LINE_DEL),
            a(FVIRTKEY | FCONTROL, 'D' as u16, CMD_LINE_DUP),
            a(FVIRTKEY | FALT, 0x26, CMD_LINE_UP),     // VK_UP
            a(FVIRTKEY | FALT, 0x28, CMD_LINE_DOWN),   // VK_DOWN
            a(FVIRTKEY | FCONTROL, 0xBB, CMD_ZOOM_IN), // VK_OEM_PLUS
            a(FVIRTKEY | FCONTROL, 0xBD, CMD_ZOOM_OUT),
            a(FVIRTKEY | FCONTROL, '0' as u16, CMD_ZOOM_RESET),
            a(FVIRTKEY, 0x1B, CMD_FIND_CLOSE), // Esc
            a(FVIRTKEY, 0x7A, CMD_FULLSCREEN), // F11
        ];
        unsafe {
            self.accel = CreateAcceleratorTableW(&table).ok();
        }
    }
}

// ================= 标签管理 =================

impl App {
    fn create_edit(&self, tab_index: usize) -> HWND {
        unsafe {
            let id = 1000 + tab_index; // 子控件 ID = 标签索引（EN_CHANGE 通知映射用）
            let style = wam::WS_CHILD
                | wam::WS_VISIBLE
                | wam::WS_VSCROLL
                | wam::WINDOW_STYLE(wam::ES_MULTILINE as u32)
                | wam::WINDOW_STYLE(wam::ES_AUTOVSCROLL as u32)
                | wam::WINDOW_STYLE(wam::ES_NOHIDESEL as u32)
                | if self.wrap {
                    wam::WINDOW_STYLE(0)
                } else {
                    wam::WS_HSCROLL | wam::WINDOW_STYLE(wam::ES_AUTOHSCROLL as u32)
                };
            let hwnd = CreateWindowExW(
                wam::WINDOW_EX_STYLE(0),
                w!("EDIT"),
                PCWSTR::null(),
                style,
                0, 0, 0, 0,
                self.hwnd,
                HMENU(id as *mut core::ffi::c_void),
                self.hinst,
                None,
            )
            .expect("创建编辑控件失败");
            SendMessageW(hwnd, WM_SETFONT, WPARAM(self.text_font.0 as usize), LPARAM(1));
            edit::set_text_limit(hwnd);
            SendMessageW(
                hwnd,
                0x0431, /*EM_SETEVENTMASK*/
                WPARAM(0),
                LPARAM(0x0001 | 0x0002), // ENM_CHANGE | ENM_SELCHANGE
            );
            let _ = SetWindowSubclass(hwnd, Some(edit_subproc), 1, 0);
            hwnd
        }
    }

    pub fn new_tab(&mut self) -> usize {
        let edit_h = self.create_edit(self.tabs.len());
        let tab = Tab::new(edit_h);
        self.tabs.push(tab);
        let idx = self.tabs.len() - 1;
        self.activate(idx);
        idx
    }

    pub fn activate(&mut self, idx: usize) {
        if idx >= self.tabs.len() {
            return;
        }
        self.active = idx;
        self.layout();
        unsafe {
            if let Some(tab) = self.tabs.get(idx) {
                let _ = SetFocus(tab.edit);
            }
        }
        self.update_ui();
        self.refresh_status();
    }

    pub fn close_tab(&mut self, idx: usize) -> bool {
        if idx >= self.tabs.len() {
            return false;
        }
        if !self.prompt_save_if_dirty(idx) {
            return false;
        }
        self.destroy_tab(idx);
        if self.tabs.is_empty() {
            self.new_tab();
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if idx <= self.active {
            self.active = self.active.saturating_sub(1).min(self.tabs.len() - 1);
        }
        self.activate(self.active);
        true
    }

    fn destroy_tab(&mut self, idx: usize) {
        unsafe {
            let edit_h = self.tabs[idx].edit;
            let _ = RemoveWindowSubclass(edit_h, Some(edit_subproc), 1);
            let _ = DestroyWindow(edit_h);
        }
        self.tabs.remove(idx);
        // 修正 edit 控件 ID 与新索引的映射
        for (i, tab) in self.tabs.iter().enumerate() {
            unsafe {
                wam::SetWindowLongPtrW(tab.edit, wam::GWL_ID, (1000 + i) as isize);
            }
        }
    }

    fn prompt_save_if_dirty(&mut self, idx: usize) -> bool {
        if !self.tabs[idx].dirty {
            return true;
        }
        let name = self.tabs[idx].display.clone();
        let text = format!("「{}」有未保存的改动。\n\n是 —— 保存并继续\n否 —— 不保存继续\n取消 —— 什么也不做", name);
        let r = self.msg_box(&text, MB_YESNOCANCEL | MB_ICONQUESTION);
        match r {
            IDYES => self.save_tab(idx, false),
            IDNO => true,
            _ => false,
        }
    }
}

// ================= 文件命令 =================

impl App {
    pub fn open_path(&mut self, path: &str) {
        let path = fileio::absolutize(path);
        for (i, tab) in self.tabs.iter().enumerate() {
            if tab.path.as_deref() == Some(path.as_str()) {
                self.activate(i);
                return;
            }
        }
        if !std::path::Path::new(&path).exists() {
            self.msg_box(&format!("找不到该文件：{}", path), MB_ICONWARNING);
            return;
        }
        if let Some(size) = fileio::file_size(&path) {
            if size > MAX_OPEN_BYTES {
                self.msg_box("文件太大（超过 100 MB）。大文件模式将在后续版本提供。", MB_ICONWARNING);
                return;
            }
        }
        let data = match fileio::read_bytes(&path) {
            Ok(d) => d,
            Err(e) => {
                self.msg_box(&e, MB_ICONWARNING);
                return;
            }
        };
        let det = encoding::detect_and_decode(&data);
        let text = edit::normalize_to_crlf(&det.text);
        let eol = edit::detect_eol(&det.text);

        let edit_h = self.create_edit(self.tabs.len());
        let mut tab = Tab::new(edit_h);
        tab.path = Some(path.clone());
        tab.display = std::path::Path::new(&path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        tab.encoding = det.encoding;
        tab.eol = eol;
        tab.raw = if (data.len() as u64) <= RAW_KEEP_LIMIT { Some(data) } else { None };
        edit::set_text(&mut tab, &text);
        unsafe {
            SendMessageW(tab.edit, EM_SETMODIFY, WPARAM(0), LPARAM(0));
        }
        self.tabs.push(tab);
        self.push_recent(path.clone());
        self.activate(self.tabs.len() - 1);
    }

    pub fn open_dialog(&mut self) {
        // 先收集状态（对话框会泵消息，不能持有 App 借用）
        let (hwnd, filter, title) = (
            self.hwnd,
            wide_filter(&[
                ("文本文件 (*.txt;*.log;*.md;*.ini;*.csv;*.json)", "*.txt;*.log;*.md;*.ini;*.csv;*.json"),
                ("所有文件 (*.*)", "*.*"),
            ]),
            fileio::to_wide("打开文件"),
        );
        unsafe {
            let mut buf = vec![0u16; 32768];
            let mut ofn = windows::Win32::UI::Controls::Dialogs::OPENFILENAMEW {
                lStructSize: std::mem::size_of::<windows::Win32::UI::Controls::Dialogs::OPENFILENAMEW>() as u32,
                hwndOwner: hwnd,
                lpstrFilter: PCWSTR(filter.as_ptr()),
                lpstrFile: PWSTR(buf.as_mut_ptr()),
                nMaxFile: buf.len() as u32,
                lpstrTitle: PCWSTR(title.as_ptr()),
                Flags: windows::Win32::UI::Controls::Dialogs::OFN_ALLOWMULTISELECT
                    | windows::Win32::UI::Controls::Dialogs::OFN_EXPLORER
                    | windows::Win32::UI::Controls::Dialogs::OFN_FILEMUSTEXIST
                    | windows::Win32::UI::Controls::Dialogs::OFN_PATHMUSTEXIST
                    | windows::Win32::UI::Controls::Dialogs::OFN_HIDEREADONLY,
                ..Default::default()
            };
            let ok = windows::Win32::UI::Controls::Dialogs::GetOpenFileNameW(&mut ofn);
            if ok.as_bool() {
                let paths = parse_multiselect(&buf);
                for p in paths {
                    self.open_path(&p);
                }
            }
        }
    }

    pub fn save_tab(&mut self, idx: usize, save_as: bool) -> bool {
        if save_as || self.tabs[idx].path.is_none() {
            let Some(p) = self.save_as_dialog() else { return false };
            let tab = self.tabs.get_mut(idx).unwrap();
            tab.path = Some(p.clone());
            tab.display = std::path::Path::new(&p)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| p.clone());
        }
        let path = self.tabs[idx].path.clone().unwrap();

        let tab = self.tabs.get_mut(idx).unwrap();
        let text = edit::get_text(tab.edit);
        let text = edit::convert_eol(&text, tab.eol);
        let enc = tab.encoding;

        let bytes = match encoding::encode(enc, &text) {
            r if !r.lossy => r.bytes,
            _ => {
                // 有损转换保护（docs/02 E1）：不许静默替换成 ?
                let q = self.msg_box(
                    &format!("部分字符无法用 {} 表示（保存后会变成 ?）。\n\n仍要用这个编码保存吗？", enc.name()),
                    MB_YESNOCANCEL | MB_ICONWARNING,
                );
                if q != IDYES {
                    return false;
                }
                encoding::encode_exact(enc.code_page(), &text, false)
            }
        };
        match fileio::write_bytes_atomic(&path, &bytes) {
            Ok(()) => {
                let tab = self.tabs.get_mut(idx).unwrap();
                tab.dirty = false;
                tab.raw = Some(bytes); // 保存后 raw 与磁盘一致
                unsafe {
                    SendMessageW(tab.edit, EM_SETMODIFY, WPARAM(0), LPARAM(0));
                }
                self.push_recent(path);
                self.update_ui();
                self.refresh_status();
                true
            }
            Err(e) => {
                self.msg_box(&format!("保存失败：{}", e), MB_ICONWARNING);
                false
            }
        }
    }

    fn save_as_dialog(&mut self) -> Option<String> {
        let (hwnd, filter, title) = (
            self.hwnd,
            wide_filter(&[("文本文件 (*.txt)", "*.txt"), ("所有文件 (*.*)", "*.*")]),
            fileio::to_wide("另存为"),
        );
        unsafe {
            let mut buf = vec![0u16; 1024];
            let mut ofn = windows::Win32::UI::Controls::Dialogs::OPENFILENAMEW {
                lStructSize: std::mem::size_of::<windows::Win32::UI::Controls::Dialogs::OPENFILENAMEW>() as u32,
                hwndOwner: hwnd,
                lpstrFilter: PCWSTR(filter.as_ptr()),
                lpstrFile: PWSTR(buf.as_mut_ptr()),
                nMaxFile: buf.len() as u32,
                lpstrTitle: PCWSTR(title.as_ptr()),
                Flags: windows::Win32::UI::Controls::Dialogs::OFN_OVERWRITEPROMPT
                    | windows::Win32::UI::Controls::Dialogs::OFN_PATHMUSTEXIST
                    | windows::Win32::UI::Controls::Dialogs::OFN_HIDEREADONLY,
                ..Default::default()
            };
            let ok = windows::Win32::UI::Controls::Dialogs::GetSaveFileNameW(&mut ofn);
            if ok.as_bool() {
                let p = PWSTR(buf.as_mut_ptr()).to_string().unwrap_or_default();
                if !p.is_empty() {
                    return Some(fileio::absolutize(&p));
                }
            }
            None
        }
    }

    pub fn push_recent(&mut self, path: String) {
        self.recents.retain(|p| p != &path);
        self.recents.insert(0, path);
        self.recents.truncate(10);
        self.cfg_text = fileio::save_recents(&self.recents);
    }
}

/// 筛选器字符串：("显示名","模式") 数组 → NUL 分隔的 UTF-16
fn wide_filter(pairs: &[(&str, &str)]) -> Vec<u16> {
    let mut v = Vec::new();
    for (name, pat) in pairs {
        v.extend(name.encode_utf16());
        v.push(0);
        v.extend(pat.encode_utf16());
        v.push(0);
    }
    v.push(0);
    v
}

/// 解析多选文件对话框的 "目录\0文件1\0文件2\0\0"
fn parse_multiselect(buf: &[u16]) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    for &c in buf {
        if c == 0 {
            if !cur.is_empty() {
                parts.push(std::mem::take(&mut cur));
            } else {
                break;
            }
        } else {
            cur.push(char::from_u32(c as u32).unwrap_or('\u{FFFD}'));
        }
    }
    if parts.len() <= 1 {
        return parts;
    }
    let dir = parts[0].clone();
    parts[1..]
        .iter()
        .map(|f| fileio::absolutize(&format!("{}\\{}", dir, f)))
        .collect()
}

// ================= 查找替换 =================

impl App {
    pub fn show_find(&mut self, replace_mode: bool) {
        self.find_replace_visible = replace_mode;
        self.find_visible = true;
        self.layout();
        unsafe {
            if let Some(tab) = self.tabs.get(self.active) {
                let (s, e) = edit::get_sel(tab.edit);
                if e > s && e - s < 256 {
                    let text = edit::get_text(tab.edit);
                    let sel: Vec<u16> = text[s as usize..e as usize].to_vec();
                    if let Ok(v) = String::from_utf16(&sel) {
                        let h = fileio::to_wide(&v);
                        let _ = SetWindowTextW(self.find_edit, PCWSTR(h.as_ptr()));
                    }
                }
            }
            let _ = SetFocus(self.find_edit);
        }
    }

    pub fn hide_find(&mut self) {
        self.find_visible = false;
        self.layout();
        unsafe {
            if let Some(tab) = self.tabs.get(self.active) {
                let _ = SetFocus(tab.edit);
            }
        }
    }

    fn fold_char(c: u16) -> u16 {
        let ch = char::from_u32(c as u32).unwrap_or('\u{FFFD}');
        let lower: Vec<char> = ch.to_lowercase().collect();
        if lower.len() == 1 { lower[0] as u16 } else { c }
    }

    fn find_positions(text: &[u16], needle: &[u16], case: bool) -> Vec<(usize, usize)> {
        if needle.is_empty() || text.len() < needle.len() {
            return vec![];
        }
        if case {
            text.windows(needle.len())
                .enumerate()
                .filter(|(_, w)| *w == needle)
                .map(|(i, _)| (i, i + needle.len()))
                .collect()
        } else {
            let n: Vec<u16> = needle.iter().map(|&c| Self::fold_char(c)).collect();
            let mut out = Vec::new();
            for i in 0..=text.len() - needle.len() {
                let w = &text[i..i + needle.len()];
                if w.iter().map(|&c| Self::fold_char(c)).eq(n.iter().copied()) {
                    out.push((i, i + needle.len()));
                }
            }
            out
        }
    }

    pub fn find_next(&mut self, backwards: bool) {
        let needle_buf = unsafe { get_window_text(self.find_edit) };
        if needle_buf.is_empty() {
            self.set_find_count("输入要查找的内容");
            return;
        }
        // 先在不可变借用里收集数据，再做可变操作
        let (text, sel, case) = {
            let Some(tab) = self.tabs.get(self.active) else { return };
            let sel = edit::get_sel(tab.edit);
            let text = edit::get_text(tab.edit);
            (text, sel, self.find_case)
        };
        let needle: Vec<u16> = needle_buf.encode_utf16().collect();
        let all = Self::find_positions(&text, &needle, case);
        let count_msg = format!("{} 处匹配", all.len());
        self.set_find_count(&count_msg);
        if all.is_empty() {
            return;
        }
        let (s, e) = sel;
        let pos = if backwards { s as usize } else { e as usize };
        let found = if backwards {
            all.iter().rev().find(|(a, _)| *a < pos).copied().or_else(|| all.last().copied())
        } else {
            all.iter().find(|(a, _)| *a >= pos).copied().or_else(|| all.first().copied())
        };
        if let Some((a, b)) = found {
            if let Some(tab) = self.tabs.get(self.active) {
                edit::set_sel(tab.edit, a as i32, b as i32);
            }
        }
    }

    pub fn do_replace_one(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else { return };
        let replacement = unsafe { get_window_text(self.replace_edit) };
        let needle_buf = unsafe { get_window_text(self.find_edit) };
        let (s, e) = edit::get_sel(tab.edit);
        let text = edit::get_text(tab.edit);
        let needle: Vec<u16> = needle_buf.encode_utf16().collect();
        if e > s && (e - s) as usize == needle.len() {
            let sel: Vec<u16> = text[s as usize..e as usize].to_vec();
            let eq = if self.find_case {
                sel == needle
            } else {
                sel.iter().map(|&c| Self::fold_char(c)).eq(needle.iter().map(|&c| Self::fold_char(c)))
            };
            if eq {
                unsafe {
                    SendMessageW(
                        tab.edit,
                        EM_REPLACESEL,
                        WPARAM(1), // 可撤销
                        LPARAM(replacement.as_ptr() as isize),
                    );
                }
            }
        }
        self.find_next(false);
    }

    pub fn do_replace_all(&mut self) {
        let replacement = unsafe { get_window_text(self.replace_edit) };
        let needle_buf = unsafe { get_window_text(self.find_edit) };
        if needle_buf.is_empty() {
            return;
        }
        let Some(tab) = self.tabs.get_mut(self.active) else { return };
        let text = edit::get_text(tab.edit);
        let needle: Vec<u16> = needle_buf.encode_utf16().collect();
        let repl: Vec<u16> = replacement.encode_utf16().collect();
        let all = Self::find_positions(&text, &needle, self.find_case);
        if all.is_empty() {
            self.set_find_count("0 处匹配");
            return;
        }
        let mut out = Vec::with_capacity(text.len());
        let mut i = 0usize;
        let mut count = 0usize;
        for (a, b) in all {
            if a < i {
                continue;
            }
            out.extend_from_slice(&text[i..a]);
            out.extend_from_slice(&repl);
            i = b;
            count += 1;
        }
        out.extend_from_slice(&text[i..]);
        edit::set_text(tab, &out);
        tab.dirty = true;
        self.set_find_count(&format!("已替换 {} 处", count));
        self.refresh_status();
        self.update_ui();
    }

    pub fn set_find_count(&mut self, s: &str) {
        unsafe {
            let h = fileio::to_wide(s);
            let _ = SetWindowTextW(self.find_count, PCWSTR(h.as_ptr()));
        }
    }

    pub fn set_find_case_visual(&mut self) {
        unsafe {
            SendMessageW(self.btn_case, 0x00F3 /*BM_SETSTATE*/, WPARAM(self.find_case as usize), LPARAM(0));
        }
    }
}

pub unsafe fn get_window_text(hwnd: HWND) -> String {
    let len = wam::GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let n = wam::GetWindowTextW(hwnd, &mut buf);
    buf.truncate(n.max(0) as usize);
    String::from_utf16_lossy(&buf)
}

// ================= 消息循环与主窗口过程 =================

pub fn run_message_loop(app_rc: &Rc<RefCell<App>>) -> i32 {
    unsafe {
        let mut msg = MSG::default();
        let mut exit_code = 0i32;
        loop {
            let r = GetMessageW(&mut msg, HWND::default(), 0, 0);
            if r.0 <= 0 {
                exit_code = msg.wParam.0 as i32;
                break;
            }
            // 加速键
            let (accel, hwnd) = match app_rc.try_borrow() {
                Ok(a) => (a.accel, a.hwnd),
                Err(_) => (None, HWND::default()),
            };
            let mut handled = false;
            if let Some(hac) = accel {
                handled = TranslateAcceleratorW(hwnd, hac, &msg) != 0;
            }
            if !handled {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        exit_code
    }
}

unsafe extern "system" fn main_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_SIZE => {
            with_app_try(|app| app.layout());
            LRESULT(0)
        }
        wam::WM_SETFOCUS => {
            with_app_try(|app| {
                if let Some(tab) = app.tabs.get(app.active) {
                    let _ = SetFocus(tab.edit);
                }
            });
            LRESULT(0)
        }
        wam::WM_ACTIVATE => {
            if (wp.0 & 0xFFFF) != 0 {
                with_app_try(|app| {
                    if let Some(tab) = app.tabs.get(app.active) {
                        let _ = SetFocus(tab.edit);
                    }
                });
            }
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let mmi = lp.0 as *mut wam::MINMAXINFO;
            with_app_try(|app| {
                (*mmi).ptMinTrackSize = POINT { x: ui::dp(app, 480), y: ui::dp(app, 320) };
            });
            LRESULT(0)
        }
        WM_DPICHANGED => {
            with_app_mut(|app| {
                let new_dpi = ((wp.0 >> 16) & 0xFFFF) as u32;
                app.dpi = new_dpi.max(96);
                app.create_fonts();
                app.set_text_font_all();
                for h in [app.find_edit, app.replace_edit, app.find_count, app.btn_prev, app.btn_next, app.btn_case, app.btn_replace, app.btn_replace_all, app.btn_close] {
                    SendMessageW(h, WM_SETFONT, WPARAM(app.ui_font.0 as usize), LPARAM(1));
                }
                let rect = lp.0 as *const RECT;
                if !rect.is_null() {
                    let _ = SetWindowPos(
                        hwnd, HWND::default(),
                        (*rect).left, (*rect).top,
                        (*rect).right - (*rect).left,
                        (*rect).bottom - (*rect).top,
                        SWP_NOZORDER,
                    );
                }
                app.layout();
            });
            LRESULT(0)
        }
        WM_DROPFILES => {
            let hdrop = HDROP(wp.0 as *mut core::ffi::c_void);
            let mut paths = Vec::new();
            unsafe {
                let n = DragQueryFileW(hdrop, u32::MAX, None);
                for i in 0..n {
                    let len = DragQueryFileW(hdrop, i, None) as usize;
                    let mut buf = vec![0u16; len + 1];
                    DragQueryFileW(hdrop, i, Some(&mut buf));
                    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                    paths.push(String::from_utf16_lossy(&buf[..end]));
                }
                DragFinish(hdrop);
            }
            with_app_mut(|app| {
                for p in paths {
                    app.open_path(&p);
                }
            });
            LRESULT(0)
        }
        wam::WM_COPYDATA => {
            let cds = lp.0 as *const COPYDATASTRUCT;
            if !cds.is_null() && unsafe { (*cds).dwData } == 1 {
                let data = unsafe {
                    std::slice::from_raw_parts((*cds).lpData as *const u16, (*cds).cbData as usize / 2)
                };
                let path = String::from_utf16_lossy(data);
                with_app_mut(|app| app.open_path(&path));
            }
            LRESULT(1)
        }
        WM_CONTEXTMENU => {
            let from = HWND(wp.0 as *mut core::ffi::c_void);
            let is_edit = with_app_try(|app| app.tabs.iter().any(|t| t.edit.0 == from.0)).unwrap_or(false);
            if is_edit {
                let mut pt = POINT { x: (lp.0 & 0xFFFF) as i16 as i32, y: ((lp.0 >> 16) & 0xFFFF) as i16 as i32 };
                if pt.x == -1 {
                    let _ = GetCursorPos(&mut pt);
                }
                with_app_mut(|app| app.show_edit_menu(pt.x, pt.y));
                return LRESULT(0);
            }
            LRESULT(0)
        }
        WM_NOTIFY => {
            let nmhdr = lp.0 as *const NMHDR;
            if !nmhdr.is_null() && unsafe { (*nmhdr).code as u32 } == EN_SELCHANGE_U {
                with_app_try(|app| app.refresh_status());
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let code = (wp.0 >> 16) & 0xFFFF;
            let id = (wp.0 & 0xFFFF) as u32;
            match code {
                0x0300 /* EN_CHANGE */ => {
                    if id >= 1000 {
                        with_app_mut(|app| {
                            let idx = (id - 1000) as usize;
                            if let Some(tab) = app.tabs.get_mut(idx) {
                                edit::on_change(tab);
                                tab.dirty = true;
                                app.update_ui();
                                app.refresh_status();
                            }
                        });
                    }
                }
                0 => {
                    // 菜单 / 加速键 / 按钮
                    let is_findbar = with_app_try(|app| {
                        Some(app.findbar_children_contains(HWND(lp.0 as *mut core::ffi::c_void)))
                    })
                    .unwrap_or(Some(false))
                    .unwrap_or(false);
                    if is_findbar {
                        match id {
                            CMD_FIND_NEXT => with_app_mut(|app| app.find_next(false)),
                            CMD_FIND_PREV => with_app_mut(|app| app.find_next(true)),
                            CMD_FIND_CASE => with_app_mut(|app| {
                                app.find_case = !app.find_case;
                                app.set_find_case_visual();
                            }),
                            CMD_DO_REPLACE => with_app_mut(|app| app.do_replace_one()),
                            CMD_DO_REPLACE_ALL => with_app_mut(|app| app.do_replace_all()),
                            CMD_FIND_CLOSE => with_app_mut(|app| app.hide_find()),
                            _ => {}
                        }
                    } else {
                        with_app_mut(|app| app.do_command(id));
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            // 逐个处理脏标签，全部干净才退出
            loop {
                let dirty = with_app_mut(|app| (0..app.tabs.len()).find(|&i| app.tabs[i].dirty));
                match dirty {
                    None => break,
                    Some(i) => {
                        let ok = with_app_mut(|app| app.prompt_save_if_dirty(i));
                        if !ok {
                            return LRESULT(0); // 用户取消 → 不退出
                        }
                        with_app_mut(|app| app.discard_tab(i));
                    }
                }
            }
            with_app_mut(|app| app.save_config_on_exit());
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Ctrl+滚轮缩放的编辑控件子类过程
unsafe extern "system" fn edit_subproc(
    hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize,
) -> LRESULT {
    match msg {
        0x020A /* WM_MOUSEWHEEL */ => {
            let ctrl = (wp.0 & 0x0008) != 0; // MK_CONTROL
            if ctrl {
                let delta = ((wp.0 >> 16) & 0xFFFF) as i16;
                with_app_mut(|app| {
                    if delta > 0 {
                        app.zoom(1);
                    } else if delta < 0 {
                        app.zoom(-1);
                    }
                });
                LRESULT(0)
            } else {
                DefSubclassProc(hwnd, msg, wp, lp)
            }
        }
        _ => DefSubclassProc(hwnd, msg, wp, lp),
    }
}

// ================= 命令分发 =================

impl App {
    pub fn do_command(&mut self, id: u32) {
        match id {
            CMD_NEW => {
                self.new_tab();
            }
            CMD_OPEN => self.open_dialog(),
            CMD_SAVE => {
                self.save_tab(self.active, false);
            }
            CMD_SAVE_AS => {
                self.save_tab(self.active, true);
            }
            CMD_CLOSE_TAB => {
                let idx = self.active;
                self.close_tab(idx);
            }
            CMD_EXIT => {
                unsafe {
                    SendMessageW(self.hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
            CMD_UNDO => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    edit::undo(tab);
                    tab.dirty = true;
                    self.refresh_status();
                    self.update_ui();
                }
            }
            CMD_REDO => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    edit::redo(tab);
                    tab.dirty = true;
                    self.refresh_status();
                    self.update_ui();
                }
            }
            CMD_CUT | CMD_COPY | CMD_PASTE | CMD_DEL | CMD_SEL_ALL => {
                if let Some(tab) = self.tabs.get(self.active) {
                    match id {
                        CMD_CUT => edit::cut(tab.edit),
                        CMD_COPY => edit::copy(tab.edit),
                        CMD_PASTE => edit::paste(tab.edit),
                        CMD_DEL => edit::clear(tab.edit),
                        _ => edit::select_all(tab.edit),
                    }
                    self.refresh_status();
                }
            }
            CMD_LINE_DEL => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    edit::delete_line(tab);
                    tab.dirty = true;
                    self.refresh_status();
                }
            }
            CMD_LINE_DUP => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    edit::duplicate_line(tab);
                    tab.dirty = true;
                    self.refresh_status();
                }
            }
            CMD_LINE_UP => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    edit::move_line(tab, true);
                    tab.dirty = true;
                    self.refresh_status();
                }
            }
            CMD_LINE_DOWN => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    edit::move_line(tab, false);
                    tab.dirty = true;
                    self.refresh_status();
                }
            }
            CMD_FIND => self.show_find(false),
            CMD_REPLACE => self.show_find(true),
            CMD_FIND_NEXT => self.find_next(false),
            CMD_FIND_PREV => self.find_next(true),
            CMD_FIND_CLOSE => self.hide_find(),
            CMD_GOTO => self.goto_dialog(),
            CMD_WRAP => {
                self.wrap = !self.wrap;
                self.apply_wrap_all();
                self.layout();
                self.update_ui();
            }
            CMD_FONT => self.choose_font_dialog(),
            CMD_ZOOM_IN => self.zoom(1),
            CMD_ZOOM_OUT => self.zoom(-1),
            CMD_ZOOM_RESET => {
                self.font_size = FONT_DEFAULT;
                self.create_fonts();
                self.set_text_font_all();
                self.refresh_status();
            }
            CMD_READONLY => {
                self.readonly = !self.readonly;
                if let Some(tab) = self.tabs.get(self.active) {
                    edit::set_readonly(tab.edit, self.readonly);
                }
            }
            CMD_FULLSCREEN => self.toggle_fullscreen(),
            CMD_EOL_CRLF => self.set_eol(Eol::CrLf),
            CMD_EOL_LF => self.set_eol(Eol::Lf),
            CMD_EOL_CR => self.set_eol(Eol::Cr),
            id if id >= CMD_ENC_BASE && id <= CMD_ENC_BASE + 9 => {
                self.convert_encoding((id - CMD_ENC_BASE) as usize);
            }
            id if id >= CMD_ENC_RE_BASE && id <= CMD_ENC_RE_BASE + 9 => {
                self.reinterpret_encoding((id - CMD_ENC_RE_BASE) as usize);
            }
            CMD_ABOUT => {
                self.msg_box(
                    &format!(
                        "{} {}\n小方工作室 出品\n\n本程序不联网、不收集任何数据。\n单文件绿色程序，卸载即净。\n\nMIT 许可证",
                        APP_NAME, VERSION
                    ),
                    MB_OK,
                );
            }
            CMD_CLEAR_RECENTS => {
                self.recents.clear();
                self.cfg_text = fileio::save_recents(&self.recents);
            }
            id if id >= CMD_RECENT_BASE && id <= CMD_RECENT_BASE + 9 => {
                let idx = (id - CMD_RECENT_BASE) as usize;
                if let Some(p) = self.recents.get(idx).cloned() {
                    self.open_path(&p);
                }
            }
            CMD_TAB_CLOSE_OTHER => {
                let active = self.active;
                let mut i = 0;
                while i < self.tabs.len() {
                    if i != active {
                        if !self.close_tab(i) {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            CMD_TAB_CLOSE_RIGHT => {
                let active = self.active;
                while self.tabs.len() > active + 1 {
                    if !self.close_tab(self.tabs.len() - 1) {
                        break;
                    }
                }
            }
            CMD_TAB_COPY_PATH => {
                if let Some(tab) = self.tabs.get(self.active) {
                    if let Some(p) = &tab.path {
                        copy_to_clipboard(self.hwnd, p);
                    }
                }
            }
            CMD_TAB_SHOW_IN_EXPLORER => {
                if let Some(tab) = self.tabs.get(self.active) {
                    if let Some(p) = &tab.path {
                        unsafe {
                                                        let params = fileio::to_wide(&format!("/select,\"{}\"", p));
                            let _ = ShellExecuteW(
                                self.hwnd,
                                PCWSTR::null(),
                                PCWSTR(w!("explorer").as_ptr()),
                                PCWSTR(params.as_ptr()),
                                PCWSTR::null(),
                                SW_SHOWNORMAL,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn set_eol(&mut self, eol: Eol) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            edit::change_eol(tab, eol);
            self.refresh_status();
            self.update_ui();
        }
    }

    fn convert_encoding(&mut self, enc_idx: usize) {
        if let Some(enc) = Encoding::from_index(enc_idx) {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                if tab.encoding != enc {
                    tab.encoding = enc;
                    tab.dirty = true;
                    self.refresh_status();
                    self.update_ui();
                }
            }
        }
    }

    fn reinterpret_encoding(&mut self, enc_idx: usize) {
        let Some(enc) = Encoding::from_index(enc_idx) else { return };
        let Some(tab) = self.tabs.get_mut(self.active) else { return };
        let Some(raw) = tab.raw.clone() else {
            self.msg_box("文件过大，原始字节未保留，无法重新解释。", MB_ICONWARNING);
            return;
        };
        let data = encoding::trim_bom(enc, &raw);
        let text = encoding::decode(enc, data);
        let text = edit::normalize_to_crlf(&text);
        edit::set_text(tab, &text);
        tab.encoding = enc;
        tab.dirty = true;
        self.refresh_status();
        self.update_ui();
    }

    pub fn zoom(&mut self, delta: i32) {
        let ns = (self.font_size + delta).clamp(FONT_MIN, FONT_MAX);
        if ns != self.font_size {
            self.font_size = ns;
            self.create_fonts();
            self.set_text_font_all();
            self.refresh_status();
        }
    }

    fn apply_wrap_all(&mut self) {
        // 重建所有 EDIT 控件以改变自动换行样式（EDIT 不支持运行时改样式）
        let old: Vec<Tab> = std::mem::take(&mut self.tabs);
        for mut tab in old {
            unsafe {
                let text = edit::get_text(tab.edit);
                let (s, e) = edit::get_sel(tab.edit);
                let _ = RemoveWindowSubclass(tab.edit, Some(edit_subproc), 1);
                let _ = DestroyWindow(tab.edit);
                let new_edit = self.create_edit(self.tabs.len());
                tab.edit = new_edit;
                edit::set_text(&mut tab, &text);
                edit::set_sel(new_edit, s, e);
            }
            self.tabs.push(tab);
        }
        self.layout();
    }

    fn toggle_fullscreen(&mut self) {
        unsafe {
            if !self.fullscreen {
                let mut placement = WINDOWPLACEMENT::default();
                placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
                let _ = GetWindowPlacement(self.hwnd, &mut placement);
                self.saved_placement = Some(placement);
                let style = GetWindowLongPtrW(self.hwnd, wam::GWL_STYLE) as u32;
                wam::SetWindowLongPtrW(self.hwnd, wam::GWL_STYLE, (style & !WS_OVERLAPPEDWINDOW.0) as isize);
                let monitor = MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST);
                let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
                let _ = GetMonitorInfoW(monitor, &mut mi);
                let _ = SetWindowPos(
                    self.hwnd, HWND::default(),
                    mi.rcMonitor.left, mi.rcMonitor.top,
                    mi.rcMonitor.right - mi.rcMonitor.left,
                    mi.rcMonitor.bottom - mi.rcMonitor.top,
                    SWP_NOZORDER,
                );
                self.fullscreen = true;
            } else {
                if let Some(p) = self.saved_placement {
                    let style = GetWindowLongPtrW(self.hwnd, wam::GWL_STYLE) as u32;
                    wam::SetWindowLongPtrW(self.hwnd, wam::GWL_STYLE, (style | WS_OVERLAPPEDWINDOW.0) as isize);
                    let _ = SetWindowPlacement(self.hwnd, &p);
                }
                self.fullscreen = false;
            }
            self.layout();
        }
    }

    fn choose_font_dialog(&mut self) {
        unsafe {
            let mut lf = LOGFONTW::default();
            lf.lfHeight = -(self.font_size * self.dpi as i32 + 48) / 72;
            set_face(&mut lf, "Consolas");
            let mut cf = windows::Win32::UI::Controls::Dialogs::CHOOSEFONTW {
                lStructSize: std::mem::size_of::<windows::Win32::UI::Controls::Dialogs::CHOOSEFONTW>() as u32,
                hwndOwner: self.hwnd,
                lpLogFont: &mut lf,
                Flags: windows::Win32::UI::Controls::Dialogs::CF_INITTOLOGFONTSTRUCT
                    | windows::Win32::UI::Controls::Dialogs::CF_SCREENFONTS,
                ..Default::default()
            };
            let ok = windows::Win32::UI::Controls::Dialogs::ChooseFontW(&mut cf);
            if ok.as_bool() {
                let px = -lf.lfHeight;
                if px > 0 {
                    let pt = (px * 72 + self.dpi as i32 / 2) / self.dpi as i32;
                    self.font_size = pt.clamp(FONT_MIN, FONT_MAX);
                }
                self.create_fonts();
                self.set_text_font_all();
            }
        }
    }

    fn discard_tab(&mut self, idx: usize) {
        unsafe {
            let edit_h = self.tabs[idx].edit;
            let _ = RemoveWindowSubclass(edit_h, Some(edit_subproc), 1);
            let _ = DestroyWindow(edit_h);
        }
        self.tabs.remove(idx);
        for (i, tab) in self.tabs.iter().enumerate() {
            unsafe {
                wam::SetWindowLongPtrW(tab.edit, wam::GWL_ID, (1000 + i) as isize);
            }
        }
    }

    fn save_config_on_exit(&mut self) {
        self.cfg_text = fileio::ini_set(&self.cfg_text, "视图", "字号", &self.font_size.to_string());
        self.cfg_text = fileio::ini_set(&self.cfg_text, "视图", "自动换行", if self.wrap { "1" } else { "0" });
        fileio::save_config(&self.cfg_text);
    }

    fn findbar_children_contains(&self, h: HWND) -> bool {
        [
            self.find_edit, self.find_count, self.replace_edit, self.btn_prev,
            self.btn_next, self.btn_case, self.btn_replace, self.btn_replace_all, self.btn_close,
        ]
        .iter()
        .any(|x| x.0 == h.0)
    }

    pub fn msg_box(&self, text: &str, style: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
        unsafe {
            let t = fileio::to_wide(text);
            let cap = fileio::to_wide(APP_NAME);
            MessageBoxW(self.hwnd, PCWSTR(t.as_ptr()), PCWSTR(cap.as_ptr()), style)
        }
    }
}

// ================= 菜单 =================

struct MenuBuilder {
    hmenu: HMENU,
}

impl MenuBuilder {
    unsafe fn new() -> Option<Self> {
        CreatePopupMenu().ok().map(|h| MenuBuilder { hmenu: h })
    }

    unsafe fn item(&self, label: &str, id: u32) {
        let t = fileio::to_wide(label);
        let _ = AppendMenuW(self.hmenu, MF_STRING, id as usize, PCWSTR(t.as_ptr()));
    }

    unsafe fn sep(&self) {
        let _ = AppendMenuW(self.hmenu, MF_SEPARATOR, 0, PCWSTR::null());
    }

    unsafe fn sub(&self, label: &str, sub: HMENU) {
        let t = fileio::to_wide(label);
        let _ = AppendMenuW(self.hmenu, MF_POPUP, sub.0 as usize, PCWSTR(t.as_ptr()));
    }

    unsafe fn check(&self, id: u32, on: bool) {
        let _ = EnableMenuItem(self.hmenu, id, MF_BYCOMMAND | if on { MF_CHECKED } else { MF_ENABLED });
    }

    unsafe fn gray(&self, id: u32, off: bool) {
        let _ = EnableMenuItem(self.hmenu, id, MF_BYCOMMAND | if off { MF_DISABLED | MF_GRAYED } else { MF_ENABLED });
    }

    unsafe fn track(&self, app: &App, x: i32, y: i32) {
        let _ = TrackPopupMenu(self.hmenu, TPM_RIGHTBUTTON, x, y, 0, app.hwnd, None);
    }
}

fn encoding_submenus(app: &App) -> (HMENU, HMENU) {
    unsafe {
        let convert = MenuBuilder::new().unwrap();
        for (i, e) in ALL_ENCODINGS.iter().enumerate() {
            let mark = if app.tabs.get(app.active).map(|t| t.encoding) == Some(*e) {
                format!("● {}", e.name())
            } else {
                e.name().to_string()
            };
            let t = fileio::to_wide(&mark);
            let _ = AppendMenuW(convert.hmenu, MF_STRING, (CMD_ENC_BASE + i as u32) as usize, PCWSTR(t.as_ptr()));
        }
        let re = MenuBuilder::new().unwrap();
        let has_raw = app.tabs.get(app.active).map(|t| t.raw.is_some()).unwrap_or(false);
        for (i, e) in ALL_ENCODINGS.iter().enumerate() {
            let label = format!("以 {} 重新解释（修乱码）", e.name());
            let t = fileio::to_wide(&label);
            let flags = MF_STRING | if has_raw { MF_ENABLED } else { MF_DISABLED | MF_GRAYED };
            let _ = AppendMenuW(re.hmenu, flags, (CMD_ENC_RE_BASE + i as u32) as usize, PCWSTR(t.as_ptr()));
        }
        (convert.hmenu, re.hmenu)
    }
}

impl App {
    /// 工具栏 ☰ 按钮：完整主菜单
    pub fn show_main_menu(&mut self) {
        let r = toolbar_button_rect(self, TOOLBAR_BTNS.len() - 1);
        let mut pt = POINT { x: r.left, y: r.bottom };
        unsafe {
            let _ = ClientToScreen(self.toolbar, &mut pt);
        }
        self.show_main_menu_at(pt.x, pt.y);
    }

    pub fn show_main_menu_at(&mut self, x: i32, y: i32) {
        unsafe {
            let Some(m) = MenuBuilder::new() else { return };

            // 文件子菜单
            let file = MenuBuilder::new().unwrap();
            file.item("新建（Ctrl+N）", CMD_NEW);
            file.item("打开…（Ctrl+O）", CMD_OPEN);
            if !self.recents.is_empty() {
                let rec = MenuBuilder::new().unwrap();
                for (i, p) in self.recents.iter().enumerate() {
                    let t = fileio::to_wide(p);
                    let _ = AppendMenuW(rec.hmenu, MF_STRING, (CMD_RECENT_BASE + i as u32) as usize, PCWSTR(t.as_ptr()));
                }
                rec.sep();
                rec.item("清空最近记录", CMD_CLEAR_RECENTS);
                file.sub("最近打开", rec.hmenu);
            }
            file.item("保存（Ctrl+S）", CMD_SAVE);
            file.item("另存为…（Ctrl+Shift+S）", CMD_SAVE_AS);
            file.sep();
            let (enc, re) = encoding_submenus(self);
            file.sub("编码（转换为…）", enc);
            file.sub("以编码重新解释…", re);
            let eol = MenuBuilder::new().unwrap();
            for e in Eol::ALL {
                let mark = if self.tabs.get(self.active).map(|t| t.eol) == Some(e) {
                    format!("● {}", e.name())
                } else {
                    e.name().to_string()
                };
                let id = match e {
                    Eol::CrLf => CMD_EOL_CRLF,
                    Eol::Lf => CMD_EOL_LF,
                    Eol::Cr => CMD_EOL_CR,
                };
                let t = fileio::to_wide(&mark);
                let _ = AppendMenuW(eol.hmenu, MF_STRING, id as usize, PCWSTR(t.as_ptr()));
            }
            file.sub("换行符", eol.hmenu);
            file.sep();
            file.item("关闭标签（Ctrl+W）", CMD_CLOSE_TAB);
            file.sep();
            file.item("退出", CMD_EXIT);

            // 编辑子菜单
            let editm = MenuBuilder::new().unwrap();
            editm.item("撤销（Ctrl+Z）", CMD_UNDO);
            editm.item("重做（Ctrl+Y）", CMD_REDO);
            editm.sep();
            editm.item("剪切（Ctrl+X）", CMD_CUT);
            editm.item("复制（Ctrl+C）", CMD_COPY);
            editm.item("粘贴（Ctrl+V）", CMD_PASTE);
            editm.item("删除（Del）", CMD_DEL);
            editm.item("全选（Ctrl+A）", CMD_SEL_ALL);
            editm.sep();
            editm.item("删除行（Ctrl+Shift+K）", CMD_LINE_DEL);
            editm.item("复制行（Ctrl+D）", CMD_LINE_DUP);
            editm.item("上移行（Alt+↑）", CMD_LINE_UP);
            editm.item("下移行（Alt+↓）", CMD_LINE_DOWN);

            // 查找子菜单
            let findm = MenuBuilder::new().unwrap();
            findm.item("查找…（Ctrl+F）", CMD_FIND);
            findm.item("替换…（Ctrl+H）", CMD_REPLACE);
            findm.item("查找下一个（F3）", CMD_FIND_NEXT);
            findm.item("查找上一个（Shift+F3）", CMD_FIND_PREV);
            findm.item("转到行…（Ctrl+G）", CMD_GOTO);

            // 查看子菜单
            let viewm = MenuBuilder::new().unwrap();
            viewm.item("自动换行", CMD_WRAP);
            viewm.check(CMD_WRAP, self.wrap);
            viewm.item("只读模式", CMD_READONLY);
            viewm.check(CMD_READONLY, self.readonly);
            viewm.sep();
            viewm.item("字体…", CMD_FONT);
            viewm.item("放大（Ctrl+=）", CMD_ZOOM_IN);
            viewm.item("缩小（Ctrl+-）", CMD_ZOOM_OUT);
            viewm.item("重置缩放（Ctrl+0）", CMD_ZOOM_RESET);
            viewm.sep();
            viewm.item("全屏（F11）", CMD_FULLSCREEN);

            let help = MenuBuilder::new().unwrap();
            help.item("关于 Chit", CMD_ABOUT);

            m.sub("文件", file.hmenu);
            m.sub("编辑", editm.hmenu);
            m.sub("查找", findm.hmenu);
            m.sub("查看", viewm.hmenu);
            m.sub("帮助", help.hmenu);
            m.sep();
            m.item("退出", CMD_EXIT);

            m.track(self, x, y);
        }
    }

    /// 编辑区右键菜单
    pub fn show_edit_menu(&mut self, x: i32, y: i32) {
        unsafe {
            let Some(m) = MenuBuilder::new() else { return };
            m.item("剪切（Ctrl+X）", CMD_CUT);
            m.item("复制（Ctrl+C）", CMD_COPY);
            m.item("粘贴（Ctrl+V）", CMD_PASTE);
            m.item("删除（Del）", CMD_DEL);
            m.sep();
            m.item("全选（Ctrl+A）", CMD_SEL_ALL);
            m.sep();
            m.item("查找…（Ctrl+F）", CMD_FIND);
            m.item("替换…（Ctrl+H）", CMD_REPLACE);
            m.sep();
            let (enc, re) = encoding_submenus(self);
            m.sub("编码（转换为…）", enc);
            m.sub("以编码重新解释…", re);
            m.track(self, x, y);
        }
    }

    /// 标签右键菜单
    pub fn show_tab_menu(&mut self, x: i32, y: i32) {
        unsafe {
            let mut pt = POINT { x, y };
            let _ = windows::Win32::Graphics::Gdi::ScreenToClient(self.tabbar, &mut pt);
            let Some(tab_idx) = tabbar_hit_tab(self, pt.x, pt.y) else { return };
            self.activate(tab_idx);
            let Some(m) = MenuBuilder::new() else { return };
            m.item("关闭标签（Ctrl+W）", CMD_CLOSE_TAB);
            m.item("关闭其他标签", CMD_TAB_CLOSE_OTHER);
            m.item("关闭右侧全部标签", CMD_TAB_CLOSE_RIGHT);
            m.sep();
            let has_path = self.tabs.get(tab_idx).map(|t| t.path.is_some()).unwrap_or(false);
            let t1 = fileio::to_wide("复制文件路径");
            let t2 = fileio::to_wide("在资源管理器中显示");
            let flags = MF_STRING | if has_path { MF_ENABLED } else { MF_DISABLED | MF_GRAYED };
            let _ = AppendMenuW(m.hmenu, flags, CMD_TAB_COPY_PATH as usize, PCWSTR(t1.as_ptr()));
            let _ = AppendMenuW(m.hmenu, flags, CMD_TAB_SHOW_IN_EXPLORER as usize, PCWSTR(t2.as_ptr()));
            m.track(self, x, y);
        }
    }

    /// 状态栏点击编码段
    pub fn show_encoding_menu(&mut self, x: i32, y: i32) {
        unsafe {
            let (enc, re) = encoding_submenus(self);
            let Some(m) = MenuBuilder::new() else { return };
            m.sub("转换为…", enc);
            m.sub("以编码重新解释…", re);
            m.track(self, x, y);
        }
    }

    /// 状态栏点击换行符段
    pub fn show_eol_menu(&mut self, x: i32, y: i32) {
        unsafe {
            let Some(m) = MenuBuilder::new() else { return };
            for e in Eol::ALL {
                let mark = if self.tabs.get(self.active).map(|t| t.eol) == Some(e) {
                    format!("● {}", e.name())
                } else {
                    e.name().to_string()
                };
                let id = match e {
                    Eol::CrLf => CMD_EOL_CRLF,
                    Eol::Lf => CMD_EOL_LF,
                    Eol::Cr => CMD_EOL_CR,
                };
                let t = fileio::to_wide(&mark);
                let _ = AppendMenuW(m.hmenu, MF_STRING, id as usize, PCWSTR(t.as_ptr()));
            }
            m.track(self, x, y);
        }
    }

    /// 转到行（一行小输入框，模态）
    pub fn goto_dialog(&mut self) {
        unsafe {
            let max_line = self.tabs.get(self.active).map(|t| edit::line_count(t.edit)).unwrap_or(1);
            let result = run_goto_modal(self, max_line);
            if let Some(line) = result {
                if let Some(tab) = self.tabs.get(self.active) {
                    let line = line.clamp(1, max_line);
                    let idx = SendMessageW(tab.edit, EM_LINEINDEX, WPARAM((line - 1) as usize), LPARAM(0)).0 as i32;
                    edit::set_sel(tab.edit, idx, idx);
                }
            }
        }
    }
}

thread_local! {
    static GOTO_RESULT: RefCell<Option<Option<i32>>> = const { RefCell::new(None) };
}

fn run_goto_modal(app: &App, max_line: i32) -> Option<i32> {
    unsafe {
        // 注册对话框窗口类（重复注册失败无妨）
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(goto_wndproc),
            hInstance: app.hinst.into(),
            hCursor: LoadCursorW(windows::Win32::Foundation::HINSTANCE::default(), wam::IDC_ARROW).unwrap_or_default(),
            lpszClassName: w!("Chit.Goto"),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);

        let (w, h) = (ui::dp(app, 300), ui::dp(app, 130));
        let mut mrc = RECT::default();
        let _ = wam::GetWindowRect(app.hwnd, &mut mrc);
        let x = mrc.left + ((mrc.right - mrc.left) - w) / 2;
        let y = mrc.top + ((mrc.bottom - mrc.top) - h) / 2;

        GOTO_RESULT.with(|s| *s.borrow_mut() = None);

        let title = fileio::to_wide(&format!("转到行（1 - {}）", max_line));
        let hwnd = CreateWindowExW(
            wam::WINDOW_EX_STYLE(0),
            w!("Chit.Goto"),
            PCWSTR(title.as_ptr()),
            WS_POPUP | WS_CAPTION | WS_SYSMENU,
            x, y, w, h,
            app.hwnd, // owner
            HMENU::default(),
            app.hinst,
            None,
        )
        .ok()?;
        wam::SetWindowLongPtrW(hwnd, wam::GWLP_USERDATA, max_line as isize);

        let label = fileio::to_wide("行号：");
        let lbl = CreateWindowExW(
            wam::WINDOW_EX_STYLE(0), w!("STATIC"), PCWSTR(label.as_ptr()),
            WS_CHILD | WS_VISIBLE,
            ui::dp(app, 16), ui::dp(app, 22), ui::dp(app, 50), ui::dp(app, 22),
            hwnd, HMENU::default(), app.hinst, None,
        )
        .unwrap_or_default();
        let edit_h = CreateWindowExW(
            wam::WINDOW_EX_STYLE(0), w!("EDIT"), PCWSTR::null(),
            WS_CHILD | WS_VISIBLE | wam::WS_BORDER | wam::WINDOW_STYLE(wam::ES_NUMBER as u32),
            ui::dp(app, 70), ui::dp(app, 20), ui::dp(app, 160), ui::dp(app, 26),
            hwnd, HMENU(1 as *mut core::ffi::c_void), app.hinst, None,
        )
        .unwrap_or_default();
        SendMessageW(lbl, WM_SETFONT, WPARAM(app.ui_font.0 as usize), LPARAM(1));
        SendMessageW(edit_h, WM_SETFONT, WPARAM(app.ui_font.0 as usize), LPARAM(1));
        // 回车/Esc 支持
        let _ = SetWindowSubclass(edit_h, Some(goto_edit_subproc), 1, 0);

        let ok_btn = fileio::to_wide("转到");
        let ok_h = CreateWindowExW(
            wam::WINDOW_EX_STYLE(0), w!("BUTTON"), PCWSTR(ok_btn.as_ptr()),
            WS_CHILD | WS_VISIBLE | wam::WINDOW_STYLE(wam::BS_DEFPUSHBUTTON as u32),
            ui::dp(app, 90), ui::dp(app, 64), ui::dp(app, 60), ui::dp(app, 28),
            hwnd, HMENU(2 as *mut core::ffi::c_void), app.hinst, None,
        )
        .unwrap_or_default();
        let cancel_btn = fileio::to_wide("取消");
        let cancel_h = CreateWindowExW(
            wam::WINDOW_EX_STYLE(0), w!("BUTTON"), PCWSTR(cancel_btn.as_ptr()),
            WS_CHILD | WS_VISIBLE,
            ui::dp(app, 158), ui::dp(app, 64), ui::dp(app, 60), ui::dp(app, 28),
            hwnd, HMENU(3 as *mut core::ffi::c_void), app.hinst, None,
        )
        .unwrap_or_default();
        SendMessageW(ok_h, WM_SETFONT, WPARAM(app.ui_font.0 as usize), LPARAM(1));
        SendMessageW(cancel_h, WM_SETFONT, WPARAM(app.ui_font.0 as usize), LPARAM(1));

        // 模态循环
        let _ = EnableWindow(app.hwnd, false);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let mut msg = MSG::default();
        loop {
            let done = GOTO_RESULT.with(|s| s.borrow().is_some());
            if done {
                break;
            }
            let r = GetMessageW(&mut msg, HWND::default(), 0, 0);
            if r.0 <= 0 {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = EnableWindow(app.hwnd, true);
        let _ = DestroyWindow(hwnd);
        if let Some(tab) = app.tabs.get(app.active) {
            let _ = SetFocus(tab.edit);
        }
        GOTO_RESULT.with(|s| s.borrow_mut().take()).flatten()
    }
}

unsafe extern "system" fn goto_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = (wp.0 & 0xFFFF) as u32;
            match id {
                2 => {
                    // 确定
                    let edit_h = GetDlgItem(hwnd, 1).unwrap_or_default();
                    let text = get_window_text(edit_h);
                    let line = text.trim().parse::<i32>().ok();
                    GOTO_RESULT.with(|s| *s.borrow_mut() = Some(line));
                }
                3 => {
                    GOTO_RESULT.with(|s| *s.borrow_mut() = Some(None));
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            GOTO_RESULT.with(|s| *s.borrow_mut() = Some(None));
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// 转到行输入框的回车/Esc 子类过程
unsafe extern "system" fn goto_edit_subproc(
    hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize,
) -> LRESULT {
    match msg {
        wam::WM_CHAR => {
            match wp.0 as u16 {
                13 => {
                    let text = get_window_text(hwnd);
                    let line = text.trim().parse::<i32>().ok();
                    GOTO_RESULT.with(|s| *s.borrow_mut() = Some(line));
                    LRESULT(0)
                }
                27 => {
                    GOTO_RESULT.with(|s| *s.borrow_mut() = Some(None));
                    LRESULT(0)
                }
                _ => DefSubclassProc(hwnd, msg, wp, lp),
            }
        }
        _ => DefSubclassProc(hwnd, msg, wp, lp),
    }
}

/// 复制文本到剪贴板（复制路径用）
pub fn copy_to_clipboard(hwnd: HWND, text: &str) {
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::Foundation::HANDLE;

    unsafe {
        if OpenClipboard(hwnd).is_err() {
            return;
        }
        let _ = EmptyClipboard();
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = wide.len() * 2;
        if let Ok(h) = GlobalAlloc(GMEM_MOVEABLE, bytes) {
            let p = GlobalLock(h);
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, p as *mut u8, bytes);
                let _ = GlobalUnlock(h);
                let _ = SetClipboardData(13 /*CF_UNICODETEXT*/, HANDLE(h.0));
            }
        }
        let _ = CloseClipboard();
    }
}
