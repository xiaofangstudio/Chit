//! fileio.rs —— 文件读写、原子写入、配置与最近文件（docs/02 A5、docs/03 §5.1）

use std::path::{Path, PathBuf};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING};
use windows::Win32::System::Threading::CreateMutexW;

/// 读取整个文件为字节
pub fn read_bytes(path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("无法读取文件：{}", e))
}

/// 原子写入（docs/03 §5.1）：先写同目录临时文件 → 冲刷 → 原子替换。
/// 绝不直接原地覆盖用户文件。
pub fn write_bytes_atomic(path: &str, bytes: &[u8]) -> Result<(), String> {
    let p = Path::new(path);
    let dir = p.parent().unwrap_or_else(|| Path::new("."));
    let tmp_path: PathBuf = dir.join(format!("Chit.tmp.{}", std::process::id()));

    // 1+2. 写临时文件并冲刷
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp_path)
            .map_err(|e| format!("无法创建临时文件：{}", e))?;
        f.write_all(bytes).map_err(|e| format!("写入失败：{}", e))?;
        f.sync_all().map_err(|e| format!("冲刷失败：{}", e))?;
    }

    let tmp_w = to_wide(&tmp_path.to_string_lossy());
    let dst_w = to_wide(path);

    // 3. 原子替换（MoveFileExW REPLACE_EXISTING 在同卷上是原子的）
    let ok = unsafe {
        MoveFileExW(PCWSTR(tmp_w.as_ptr()), PCWSTR(dst_w.as_ptr()), MOVEFILE_REPLACE_EXISTING)
    };
    if ok.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("无法替换文件（错误代码 {}）", unsafe { GetLastError().0 }));
    }
    Ok(())
}

/// UTF-16 宽字符串（带结尾 0）
pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 单实例互斥体；返回 Ok(false) 表示已有实例在跑
pub fn create_singleton_mutex() -> Result<bool, String> {
    unsafe {
        let name = to_wide("Chit.SingleInstance");
        let h = CreateMutexW(None, false, PCWSTR(name.as_ptr()))
            .map_err(|e| format!("创建互斥体失败：{}", e))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(h);
            return Ok(false);
        }
        // 故意不关句柄：进程退出自动释放
        Ok(true)
    }
}

/// 获取文件大小
pub fn file_size(path: &str) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

// ---------------- 配置与最近文件（ini，docs/02 A5） ----------------

pub fn config_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string());
    PathBuf::from(base).join("Chit")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.ini")
}

pub fn load_config() -> String {
    std::fs::read_to_string(config_path()).unwrap_or_default()
}

pub fn save_config(text: &str) {
    let _ = std::fs::create_dir_all(config_dir());
    let _ = std::fs::write(config_path(), text);
}

/// 从 ini 文本取一个键值（简单解析，不引入 ini 库）
pub fn ini_get(text: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            in_section = &t[1..t.len() - 1] == section;
            continue;
        }
        if in_section {
            if let Some(eq) = t.find('=') {
                if t[..eq].trim() == key {
                    return Some(t[eq + 1..].trim().to_string());
                }
            }
        }
    }
    None
}

pub fn ini_set(text: &str, section: &str, key: &str, value: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_section = false;
    let mut replaced = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            if in_section && !replaced {
                out.push(format!("{}={}", key, value));
                replaced = true;
            }
            in_section = &t[1..t.len() - 1] == section;
            out.push(line.to_string());
            continue;
        }
        if in_section {
            if let Some(eq) = t.find('=') {
                if t[..eq].trim() == key {
                    if !replaced {
                        out.push(format!("{}={}", key, value));
                        replaced = true;
                    }
                    continue;
                }
            }
        }
        out.push(line.to_string());
    }
    if !replaced {
        if !in_section {
            out.push(format!("[{}]", section));
        }
        out.push(format!("{}={}", key, value));
    }
    let mut s = out.join("\r\n");
    if !s.ends_with("\r\n") {
        s.push_str("\r\n");
    }
    s
}

/// 读最近文件列表（最多 10 个，只保留存在的）
pub fn load_recents(cfg: &str) -> Vec<String> {
    let mut v = Vec::new();
    for i in 0..10 {
        if let Some(p) = ini_get(cfg, "最近打开", &format!("recent{}", i)) {
            if !p.is_empty() && Path::new(&p).exists() && !v.contains(&p) {
                v.push(p);
            }
        }
    }
    v
}

pub fn save_recents(recents: &[String]) -> String {
    let mut cfg = load_config();
    for (i, r) in recents.iter().take(10).enumerate() {
        cfg = ini_set(&cfg, "最近打开", &format!("recent{}", i), r);
    }
    for i in recents.len()..10 {
        cfg = ini_set(&cfg, "最近打开", &format!("recent{}", i), "");
    }
    save_config(&cfg);
    cfg
}

/// 绝对路径化
pub fn absolutize(p: &str) -> String {
    let path = Path::new(p);
    if path.is_absolute() {
        path.to_string_lossy().to_string()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(path).to_string_lossy().to_string(),
            Err(_) => p.to_string(),
        }
    }
}
