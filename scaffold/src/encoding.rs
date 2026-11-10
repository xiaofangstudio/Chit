//! encoding.rs —— 编码识别与转换（docs/02 模块 E）
//!
//! 探测规则（docs/02 A3）：
//! 1. 有 BOM → 直接用 BOM 声明的编码
//! 2. 严格 UTF-8 校验通过 → UTF-8
//! 3. GB18030 能无歧义解码且含 CJK → GB18030
//! 4. 兜底 → 系统 ANSI 代码页

use windows::Win32::Foundation::BOOL;
use windows::Win32::Globalization::{
    MultiByteToWideChar, WideCharToMultiByte, CP_ACP, MB_ERR_INVALID_CHARS,
    MULTI_BYTE_TO_WIDE_CHAR_FLAGS, WC_NO_BEST_FIT_CHARS,
};

/// 支持的编码集合
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Gb18030,
    Gbk,
    Big5,
    Sjis,
    Latin1,
    Ansi,
}

pub const ALL_ENCODINGS: [Encoding; 10] = [
    Encoding::Utf8,
    Encoding::Utf8Bom,
    Encoding::Utf16Le,
    Encoding::Utf16Be,
    Encoding::Gb18030,
    Encoding::Gbk,
    Encoding::Big5,
    Encoding::Sjis,
    Encoding::Latin1,
    Encoding::Ansi,
];

impl Encoding {
    pub fn name(self) -> &'static str {
        match self {
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf8Bom => "UTF-8 BOM",
            Encoding::Utf16Le => "UTF-16 LE",
            Encoding::Utf16Be => "UTF-16 BE",
            Encoding::Gb18030 => "GB18030",
            Encoding::Gbk => "GBK",
            Encoding::Big5 => "Big5",
            Encoding::Sjis => "Shift-JIS",
            Encoding::Latin1 => "Latin-1",
            Encoding::Ansi => "系统 ANSI",
        }
    }

    /// 对应的 Windows 代码页；UTF-16 系列返回 0（手动处理）
    pub fn code_page(self) -> u32 {
        match self {
            Encoding::Utf8 | Encoding::Utf8Bom => 65001,
            Encoding::Utf16Le | Encoding::Utf16Be => 0,
            Encoding::Gb18030 => 54936,
            Encoding::Gbk => 936,
            Encoding::Big5 => 950,
            Encoding::Sjis => 932,
            Encoding::Latin1 => 28591,
            Encoding::Ansi => CP_ACP,
        }
    }

    pub fn from_index(i: usize) -> Option<Encoding> {
        ALL_ENCODINGS.get(i).copied()
    }
}

/// BOM 长度（按编码）
pub fn bom_len(enc: Encoding, data: &[u8]) -> usize {
    match enc {
        Encoding::Utf8Bom if data.starts_with(&[0xEF, 0xBB, 0xBF]) => 3,
        Encoding::Utf16Le if data.starts_with(&[0xFF, 0xFE]) => 2,
        Encoding::Utf16Be if data.starts_with(&[0xFE, 0xFF]) => 2,
        _ => 0,
    }
}

/// 剥掉 BOM（重新解释前调用）
pub fn trim_bom(enc: Encoding, data: &[u8]) -> &[u8] {
    let n = bom_len(enc, data);
    &data[n..]
}

/// 严格 UTF-8 校验（手写状态机，不分配内存）
pub fn is_valid_utf8(data: &[u8]) -> bool {
    let mut i = 0;
    let n = data.len();
    while i < n {
        let b = data[i];
        let extra: usize = match b {
            0x00..=0x7F => {
                i += 1;
                continue;
            }
            0xC2..=0xDF => 1,
            0xE0..=0xEF => 2,
            0xF0..=0xF4 => 3,
            _ => return false, // 0x80..=0xC1、0xF5..=0xFF 非法
        };
        if i + extra >= n {
            return false; // 序列被截断
        }
        for k in 1..=extra {
            if data[i + k] & 0xC0 != 0x80 {
                return false;
            }
        }
        // 排除过长编码与代理区
        match b {
            0xE0 if data[i + 1] < 0xA0 => return false,
            0xED if data[i + 1] >= 0xA0 => return false,
            0xF0 if data[i + 1] < 0x90 => return false,
            0xF4 if data[i + 1] >= 0x90 => return false,
            _ => {}
        }
        i += extra + 1;
    }
    true
}

/// 判断一段 UTF-16 文本是否包含常用汉字 / 中文标点
fn contains_cjk(text: &[u16]) -> bool {
    text.iter()
        .any(|&c| matches!(c, 0x4E00..=0x9FFF | 0x3000..=0x303F | 0xFF00..=0xFFEF))
}

pub struct DetectResult {
    pub encoding: Encoding,
    pub text: Vec<u16>,
}

/// 编码探测（docs/02 A3 四步）
pub fn detect_and_decode(data: &[u8]) -> DetectResult {
    // 1. BOM
    if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return DetectResult { encoding: Encoding::Utf8Bom, text: decode(Encoding::Utf8, &data[3..]) };
    }
    if data.starts_with(&[0xFF, 0xFE]) {
        return DetectResult { encoding: Encoding::Utf16Le, text: decode(Encoding::Utf16Le, &data[2..]) };
    }
    if data.starts_with(&[0xFE, 0xFF]) {
        return DetectResult { encoding: Encoding::Utf16Be, text: decode(Encoding::Utf16Be, &data[2..]) };
    }
    // 2. 严格 UTF-8
    if is_valid_utf8(data) {
        return DetectResult { encoding: Encoding::Utf8, text: decode(Encoding::Utf8, data) };
    }
    // 3. GB18030 启发式
    if let Some(text) = try_decode_strict(Encoding::Gb18030, data) {
        if contains_cjk(&text) {
            return DetectResult { encoding: Encoding::Gb18030, text };
        }
    }
    // 4. 系统 ANSI 兜底
    DetectResult { encoding: Encoding::Ansi, text: decode(Encoding::Ansi, data) }
}

/// 解码为 UTF-16
pub fn decode(enc: Encoding, data: &[u8]) -> Vec<u16> {
    match enc {
        Encoding::Utf16Le => data
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect(),
        Encoding::Utf16Be => data
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect(),
        _ => {
            let cp = enc.code_page();
            // UTF-8 用严格校验解码；其他代码页宽松（探测阶段已判定）
            let flags = if cp == 65001 { MB_ERR_INVALID_CHARS } else { MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0) };
            unsafe {
                let len = MultiByteToWideChar(cp, flags, data, None);
                if len <= 0 {
                    return Vec::new();
                }
                let mut buf = vec![0u16; len as usize];
                MultiByteToWideChar(cp, flags, data, Some(&mut buf));
                buf
            }
        }
    }
}

/// 严格解码：失败返回 None（GB18030 无歧义判定用）
fn try_decode_strict(enc: Encoding, data: &[u8]) -> Option<Vec<u16>> {
    let cp = enc.code_page();
    unsafe {
        let len = MultiByteToWideChar(cp, MB_ERR_INVALID_CHARS, data, None);
        if len <= 0 {
            return None;
        }
        let mut buf = vec![0u16; len as usize];
        let written = MultiByteToWideChar(cp, MB_ERR_INVALID_CHARS, data, Some(&mut buf));
        if written <= 0 {
            return None;
        }
        Some(buf)
    }
}

pub struct EncodeResult {
    pub bytes: Vec<u8>,
    pub lossy: bool,
}

/// 把 UTF-16 文本编码为目标编码
pub fn encode(enc: Encoding, text: &[u16]) -> EncodeResult {
    match enc {
        Encoding::Utf16Le => {
            let mut bytes = vec![0xFF, 0xFE];
            for &u in text {
                bytes.extend_from_slice(&u.to_le_bytes());
            }
            EncodeResult { bytes, lossy: false }
        }
        Encoding::Utf16Be => {
            let mut bytes = vec![0xFE, 0xFF];
            for &u in text {
                bytes.extend_from_slice(&u.to_be_bytes());
            }
            EncodeResult { bytes, lossy: false }
        }
        Encoding::Utf8 => EncodeResult { bytes: encode_exact(65001, text, false), lossy: false },
        Encoding::Utf8Bom => {
            let mut bytes = vec![0xEF, 0xBB, 0xBF];
            bytes.extend_from_slice(&encode_exact(65001, text, false));
            EncodeResult { bytes, lossy: false }
        }
        _ => {
            let bytes = encode_exact(enc.code_page(), text, true);
            let lossy = bytes.is_empty();
            EncodeResult { bytes, lossy }
        }
    }
}

/// 按代码页编码；check_loss 时检测「无法表示的字符」，有则返回空（由调用方提示）
pub fn encode_exact(cp: u32, text: &[u16], check_loss: bool) -> Vec<u8> {
    use windows::core::PCSTR;
    unsafe {
        let len = WideCharToMultiByte(cp, 0, text, None, PCSTR::null(), None);
        if len <= 0 {
            return Vec::new();
        }
        let mut out = vec![0u8; len as usize];
        if check_loss {
            let mut used = BOOL::default();
            let mut q: u8 = b'?';
            let w = WideCharToMultiByte(
                cp,
                WC_NO_BEST_FIT_CHARS,
                text,
                Some(&mut out),
                PCSTR(&q as *const u8),
                Some(&mut used),
            );
            if w <= 0 {
                return Vec::new();
            }
            out.truncate(w as usize);
            if used.as_bool() {
                return Vec::new(); // 有无法表示的字符
            }
        } else {
            let w = WideCharToMultiByte(cp, 0, text, Some(&mut out), PCSTR::null(), None);
            if w <= 0 {
                return Vec::new();
            }
            out.truncate(w as usize);
        }
        out
    }
}
