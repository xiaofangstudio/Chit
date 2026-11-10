//! edit.rs —— 标签页 / 文档状态 / 编辑控件操作（多级撤销、行操作、换行、字体）

use std::time::Instant;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Controls::{
    EM_CANUNDO, EM_GETSEL, EM_LINEFROMCHAR, EM_LINEINDEX, EM_LINELENGTH, EM_REPLACESEL,
    EM_SCROLLCARET, EM_SETLIMITTEXT, EM_SETMODIFY, EM_SETREADONLY, EM_SETSEL, EM_UNDO,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowTextLengthW, GetWindowTextW, SendMessageW, SetWindowTextW, WM_CLEAR, WM_COPY,
    WM_CUT, WM_PASTE,
};

use crate::consts::*;
use crate::encoding::Encoding;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Eol {
    CrLf,
    Lf,
    Cr,
}

impl Eol {
    pub fn name(self) -> &'static str {
        match self {
            Eol::CrLf => "CRLF",
            Eol::Lf => "LF",
            Eol::Cr => "CR",
        }
    }
    pub const ALL: [Eol; 3] = [Eol::CrLf, Eol::Lf, Eol::Cr];
}

/// 一个标签页 = 一个文档
pub struct Tab {
    pub edit: HWND,
    pub path: Option<String>,
    pub display: String,
    pub encoding: Encoding,
    pub eol: Eol,
    pub dirty: bool,
    /// 原始字节（用于「以…重新解释」）
    pub raw: Option<Vec<u8>>,
    // ---- 快照式多级撤销（随标签页独立，docs/02 B）----
    pub undo: Vec<Vec<u16>>,
    pub redo: Vec<Vec<u16>>,
    pub last_change: Option<Instant>,
    pub last_text: Vec<u16>,
    /// 程序化修改文本时跳过一次 EN_CHANGE 记录
    pub skip_change: bool,
}

impl Tab {
    pub fn new(edit: HWND) -> Tab {
        Tab {
            edit,
            path: None,
            display: "无标题".to_string(),
            encoding: Encoding::Utf8,
            eol: Eol::CrLf,
            dirty: false,
            raw: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_change: None,
            last_text: Vec::new(),
            skip_change: false,
        }
    }

    pub fn title(&self) -> String {
        if self.dirty {
            format!("* {}", self.display)
        } else {
            self.display.clone()
        }
    }
}

// ---------------- EDIT 控件基本操作 ----------------

/// 读取编辑控件全部文本（UTF-16）
pub fn get_text(edit: HWND) -> Vec<u16> {
    unsafe {
        let len = GetWindowTextLengthW(edit);
        if len <= 0 {
            return Vec::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(edit, &mut buf);
        buf.truncate(n.max(0) as usize);
        buf
    }
}

/// 设置文本（跳过一次撤销记录）
pub fn set_text(tab: &mut Tab, text: &[u16]) {
    tab.skip_change = true;
    unsafe {
        let mut t = text.to_vec();
        t.push(0);
        let _ = SetWindowTextW(tab.edit, windows::core::PCWSTR(t.as_ptr()));
    }
    tab.last_text = text.to_vec();
}

pub fn can_undo(tab: &Tab) -> bool {
    !tab.undo.is_empty()
        || unsafe { SendMessageW(tab.edit, EM_CANUNDO, WPARAM(0), LPARAM(0)).0 != 0 }
}

pub fn can_redo(tab: &Tab) -> bool {
    !tab.redo.is_empty()
}

/// 撤销：优先快照栈，退回 EM_UNDO（大文档快照被禁用时走这里）
pub fn undo(tab: &mut Tab) {
    if let Some(prev) = tab.undo.pop() {
        let cur = get_text(tab.edit);
        tab.redo.push(cur);
        set_text(tab, &prev);
    } else {
        unsafe {
            SendMessageW(tab.edit, EM_UNDO, WPARAM(0), LPARAM(0));
        }
    }
}

/// 重做（快照栈）
pub fn redo(tab: &mut Tab) {
    if let Some(next) = tab.redo.pop() {
        let cur = get_text(tab.edit);
        tab.undo.push(cur);
        set_text(tab, &next);
    }
}

/// EN_CHANGE 处理：快照合并逻辑
pub fn on_change(tab: &mut Tab) {
    if tab.skip_change {
        tab.skip_change = false;
        tab.last_text = get_text(tab.edit);
        return;
    }
    let now = Instant::now();
    let cur = get_text(tab.edit);
    let coalescing = tab
        .last_change
        .map(|t| (now.duration_since(t).as_millis() as u64) < UNDO_COALESCE_MS)
        .unwrap_or(false);
    if !coalescing {
        if tab.last_text.len() <= UNDO_SNAPSHOT_LIMIT {
            tab.undo.push(std::mem::take(&mut tab.last_text));
            if tab.undo.len() > UNDO_STACK_MAX {
                tab.undo.remove(0);
            }
        } else {
            // 大文档：不保留快照（EM_UNDO 兜底），清栈防内存膨胀
            tab.undo.clear();
            tab.redo.clear();
            tab.last_text = Vec::new();
        }
        tab.redo.clear();
    }
    tab.last_change = Some(now);
    tab.last_text = cur;
}

// ---------------- 选区与位置 ----------------

pub fn get_sel(edit: HWND) -> (i32, i32) {
    unsafe {
        let mut start: i32 = 0;
        let mut end: i32 = 0;
        SendMessageW(
            edit,
            EM_GETSEL,
            WPARAM(&mut start as *mut i32 as usize),
            LPARAM(&mut end as *mut i32 as isize),
        );
        (start, end)
    }
}

pub fn set_sel(edit: HWND, start: i32, end: i32) {
    unsafe {
        SendMessageW(edit, EM_SETSEL, WPARAM(start as usize), LPARAM(end as isize));
        SendMessageW(edit, EM_SCROLLCARET, WPARAM(0), LPARAM(0));
    }
}

/// 光标所在行（0 基）
pub fn caret_line(edit: HWND) -> i32 {
    unsafe { SendMessageW(edit, EM_LINEFROMCHAR, WPARAM(usize::MAX), LPARAM(0)).0 as i32 }
}

pub fn line_index(edit: HWND, line: i32) -> i32 {
    unsafe { SendMessageW(edit, EM_LINEINDEX, WPARAM(line as usize), LPARAM(0)).0 as i32 }
}

pub fn line_count(edit: HWND) -> i32 {
    caret_line(edit) + 1
}

// ---------------- 剪贴板 ----------------

pub fn cut(edit: HWND) { unsafe { SendMessageW(edit, WM_CUT, WPARAM(0), LPARAM(0)); } }
pub fn copy(edit: HWND) { unsafe { SendMessageW(edit, WM_COPY, WPARAM(0), LPARAM(0)); } }
pub fn paste(edit: HWND) { unsafe { SendMessageW(edit, WM_PASTE, WPARAM(0), LPARAM(0)); } }
pub fn clear(edit: HWND) { unsafe { SendMessageW(edit, WM_CLEAR, WPARAM(0), LPARAM(0)); } }
pub fn select_all(edit: HWND) { set_sel(edit, 0, -1); }

// ---------------- 行操作 ----------------

/// 全文行切分（内存内操作，CRLF）
fn lines_of(text: &[u16]) -> Vec<Vec<u16>> {
    let mut lines = Vec::new();
    let mut cur = Vec::new();
    let mut i = 0;
    while i < text.len() {
        if text[i] == '\r' as u16 && i + 1 < text.len() && text[i + 1] == '\n' as u16 {
            lines.push(std::mem::take(&mut cur));
            i += 2;
        } else {
            cur.push(text[i]);
            i += 1;
        }
    }
    lines.push(cur);
    lines
}

fn join_lines(lines: &[Vec<u16>]) -> Vec<u16> {
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\r' as u16);
            out.push('\n' as u16);
        }
        out.extend_from_slice(l);
    }
    out
}

/// 通用全文替换；变化经 set_text 落入撤销栈，光标按行号尽量还原
pub fn transform_text(tab: &mut Tab, f: impl FnOnce(Vec<Vec<u16>>) -> Option<Vec<Vec<u16>>>) -> bool {
    let text = get_text(tab.edit);
    let lines = lines_of(&text);
    let (sel_s, _sel_e) = get_sel(tab.edit);
    let caret_line_no = line_at_char(&text, sel_s);
    if let Some(new_lines) = f(lines) {
        let new_text = join_lines(&new_lines);
        set_text(tab, &new_text);
        let target_line = caret_line_no.min(new_lines.len() as i32 - 1).max(0);
        let idx = line_index(tab.edit, target_line);
        set_sel(tab.edit, idx, idx);
        true
    } else {
        false
    }
}

fn line_at_char(text: &[u16], ch: i32) -> i32 {
    let mut line = 0;
    let mut i = 0usize;
    let target = ch.max(0) as usize;
    while i < target && i < text.len() {
        if text[i] == '\r' as u16 && i + 1 < text.len() && text[i + 1] == '\n' as u16 {
            line += 1;
            i += 1;
        }
        i += 1;
    }
    line
}

pub fn delete_line(tab: &mut Tab) -> bool {
    let line = caret_line(tab.edit);
    transform_text(tab, |mut lines| {
        if lines.len() <= 1 {
            return Some(vec![Vec::new()]);
        }
        if line < lines.len() as i32 {
            lines.remove(line as usize);
            Some(lines)
        } else {
            None
        }
    })
}

pub fn duplicate_line(tab: &mut Tab) -> bool {
    let line = caret_line(tab.edit);
    transform_text(tab, |lines| {
        if line < lines.len() as i32 {
            let li = line as usize;
            let mut out = Vec::with_capacity(lines.len() + 1);
            out.extend_from_slice(&lines[..li]);
            out.push(lines[li].clone());
            out.extend_from_slice(&lines[li..]);
            Some(out)
        } else {
            None
        }
    })
}

pub fn move_line(tab: &mut Tab, up: bool) -> bool {
    let line = caret_line(tab.edit);
    transform_text(tab, |mut lines| {
        let n = lines.len() as i32;
        let target = if up { line - 1 } else { line + 1 };
        if target < 0 || target >= n {
            return None;
        }
        lines.swap(line as usize, target as usize);
        Some(lines)
    })
}

// ---------------- 换行符与文本规范化 ----------------

/// 把任意 EOL 文本规范化为 CRLF（EDIT 控件内存表示统一 CRLF）
pub fn normalize_to_crlf(text: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let c = text[i];
        if c == '\r' as u16 {
            out.push('\r' as u16);
            out.push('\n' as u16);
            if i + 1 < text.len() && text[i + 1] == '\n' as u16 {
                i += 2;
            } else {
                i += 1;
            }
        } else if c == '\n' as u16 {
            out.push('\r' as u16);
            out.push('\n' as u16);
            i += 1;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// 把 CRLF 文本转换为指定 EOL（保存前调用）
pub fn convert_eol(text: &[u16], eol: Eol) -> Vec<u16> {
    match eol {
        Eol::CrLf => text.to_vec(),
        Eol::Lf | Eol::Cr => {
            let mut out = Vec::with_capacity(text.len());
            let mut i = 0;
            while i < text.len() {
                let c = text[i];
                if c == '\r' as u16 && i + 1 < text.len() && text[i + 1] == '\n' as u16 {
                    i += 2;
                    if eol == Eol::Lf {
                        out.push('\n' as u16);
                    } else {
                        out.push('\r' as u16);
                    }
                } else {
                    out.push(c);
                    i += 1;
                }
            }
            out
        }
    }
}

/// 探测换行符（多数者为准）
pub fn detect_eol(text: &[u16]) -> Eol {
    let (mut crlf, mut lf, mut cr) = (0i64, 0i64, 0i64);
    let mut i = 0;
    while i < text.len() {
        match text[i] {
            13 => {
                if i + 1 < text.len() && text[i + 1] == 10 {
                    crlf += 1;
                    i += 1;
                } else {
                    cr += 1;
                }
            }
            10 => lf += 1,
            _ => {}
        }
        i += 1;
    }
    if crlf >= lf && crlf >= cr {
        Eol::CrLf
    } else if lf >= cr {
        Eol::Lf
    } else {
        Eol::Cr
    }
}

/// 转换当前文档换行符（docs/02 E2：对整个文件生效；内存统一 CRLF，保存时写入）
pub fn change_eol(tab: &mut Tab, eol: Eol) {
    if tab.eol != eol {
        tab.eol = eol;
        tab.dirty = true;
    }
}

// ---------------- 其他 ----------------

pub fn set_readonly(edit: HWND, ro: bool) {
    unsafe {
        SendMessageW(edit, EM_SETREADONLY, WPARAM(ro as usize), LPARAM(0));
    }
}

/// 放开 EDIT 控件的默认 32K 输入上限
pub fn set_text_limit(edit: HWND) {
    unsafe {
        SendMessageW(edit, EM_SETLIMITTEXT, WPARAM(0x7FFFFFFE), LPARAM(0));
    }
}
