//! consts.rs —— 命令表、资源 ID 与固定常量（docs/03：命令表是唯一动作入口）

pub const APP_NAME: &str = "Chit";
pub const VERSION: &str = "0.1.0";
pub const MAIN_CLASS: &str = "Chit.Main";

// ---------- 命令 ID ----------
pub const CMD_NEW: u32 = 101;
pub const CMD_OPEN: u32 = 102;
pub const CMD_SAVE: u32 = 103;
pub const CMD_SAVE_AS: u32 = 104;
pub const CMD_CLOSE_TAB: u32 = 105;
pub const CMD_EXIT: u32 = 106;

pub const CMD_UNDO: u32 = 110;
pub const CMD_REDO: u32 = 111;
pub const CMD_CUT: u32 = 112;
pub const CMD_COPY: u32 = 113;
pub const CMD_PASTE: u32 = 114;
pub const CMD_DEL: u32 = 115;
pub const CMD_SEL_ALL: u32 = 116;

pub const CMD_LINE_DEL: u32 = 120;
pub const CMD_LINE_DUP: u32 = 121;
pub const CMD_LINE_UP: u32 = 122;
pub const CMD_LINE_DOWN: u32 = 123;

pub const CMD_FIND: u32 = 130;
pub const CMD_REPLACE: u32 = 131;
pub const CMD_FIND_NEXT: u32 = 132;
pub const CMD_FIND_PREV: u32 = 133;
pub const CMD_GOTO: u32 = 134;

pub const CMD_WRAP: u32 = 140;
pub const CMD_FONT: u32 = 141;
pub const CMD_ZOOM_IN: u32 = 142;
pub const CMD_ZOOM_OUT: u32 = 143;
pub const CMD_ZOOM_RESET: u32 = 144;
pub const CMD_READONLY: u32 = 145;
pub const CMD_FULLSCREEN: u32 = 146;

pub const CMD_EOL_CRLF: u32 = 150;
pub const CMD_EOL_LF: u32 = 151;
pub const CMD_EOL_CR: u32 = 152;

// 编码：转换为（+0..9）；以…重新解释（+100..109）
pub const CMD_ENC_BASE: u32 = 160;
pub const CMD_ENC_RE_BASE: u32 = 260;

pub const CMD_ABOUT: u32 = 170;

pub const CMD_RECENT_BASE: u32 = 200; // +0..9
pub const CMD_CLEAR_RECENTS: u32 = 210;

// 标签右键菜单
pub const CMD_TAB_CLOSE_OTHER: u32 = 230;
pub const CMD_TAB_CLOSE_RIGHT: u32 = 231;
pub const CMD_TAB_COPY_PATH: u32 = 232;
pub const CMD_TAB_SHOW_IN_EXPLORER: u32 = 233;

// 查找条
pub const CMD_FIND_CLOSE: u32 = 240;
pub const CMD_FIND_CASE: u32 = 241;
pub const CMD_DO_REPLACE: u32 = 242;
pub const CMD_DO_REPLACE_ALL: u32 = 243;

// 工具栏「更多」菜单按钮
pub const CMD_MENU: u32 = 999;

// ---------- 撤销快照 ----------
/// 超过此大小（UTF-16 单位数）停用快照式多级撤销，退回 EM_UNDO
pub const UNDO_SNAPSHOT_LIMIT: usize = 4 * 1024 * 1024;
/// 快照栈上限
pub const UNDO_STACK_MAX: usize = 100;
/// 连续键入合并窗口（毫秒）
pub const UNDO_COALESCE_MS: u64 = 400;

/// 打开文件大小上限（M0 简单模式，大文件模式在 M2）
pub const MAX_OPEN_BYTES: u64 = 100 * 1024 * 1024;
/// 保留原始字节用于「重新解释」的上限
pub const RAW_KEEP_LIMIT: u64 = 16 * 1024 * 1024;

pub const FONT_MIN: i32 = 8;
pub const FONT_MAX: i32 = 36;
pub const FONT_DEFAULT: i32 = 12;
