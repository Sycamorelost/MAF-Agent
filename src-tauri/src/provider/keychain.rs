//! API Key 本地加密存储。
//!
//! 对应 plan/01-phase1-mvp.md §3.3「API Key 本地加密存储」。
//!
//! 方案(2026-10 升级):
//!   - Windows:DPAPI(CryptProtectData/CryptUnprotectData),密钥由 OS 按用户凭据托管,
//!     无需自管密钥;文件前缀 `dpapi:` + base64(blob)
//!   - 非 Windows / 旧格式回退:AES-256-GCM,密钥派生 SHA-256(APP_SALT|user|host),
//!     文件格式 base64(nonce || ciphertext),aad 绑定 provider_id
//!
//! 读取时按前缀自动识别两种格式,旧的 AES-GCM 密文可无缝解密。
//!
//! 安全边界:DPAPI 保护级别 = 当前 Windows 用户登录会话;攻击者需能在该用户上下文执行代码
//! 才能解密。这是 Windows 桌面应用的标准实践(Chrome/Edge 本地密码存储同源)。
//! 依据:Microsoft DPAPI 文档、OWASP Secrets Management Cheat Sheet。

use std::{fs, path::PathBuf};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use rand::RngCore;
use sha2::{Digest, Sha256};

use super::super::db;

/// 应用级盐(仅用于旧格式 AES-GCM 回退)。
const APP_SALT: &[u8] = b"metaagent-float/keychain/v1";
/// DPAPI 密文文件前缀,用于格式识别。
const DPAPI_PREFIX: &str = "dpapi:";

fn secrets_dir() -> PathBuf {
    crate::paths::data_dir().join("secrets")
}

/// 计算密钥文件路径(slot 0 沿用旧命名 `{id}.bin`,其余为 `{id}.{n}.bin`)。
/// provider_id 先做路径遍历白名单校验(CWE-22 防护,单点收敛)。
fn slot_path(provider_id: &str, slot: u32) -> Result<PathBuf, String> {
    crate::memory::validate_id(provider_id)?;
    let name = if slot == 0 {
        format!("{}.bin", provider_id)
    } else {
        format!("{}.{}.bin", provider_id, slot)
    };
    Ok(secrets_dir().join(name))
}

/// slot 0 的密钥文件路径(向后兼容旧调用点)。
fn secret_path(provider_id: &str) -> Result<PathBuf, String> {
    slot_path(provider_id, 0)
}

// ---------------- DPAPI(Windows) ----------------

#[cfg(target_os = "windows")]
mod dpapi {
    use std::ffi::c_void;

    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            p_data_in: *const DataBlob,
            sz_data_descr: *const u16,
            p_optional_entropy: *const DataBlob,
            pv_reserved: *mut c_void,
            p_prompt_struct: *const c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;
        fn CryptUnprotectData(
            p_data_in: *const DataBlob,
            ppsz_data_descr: *mut *mut u16,
            p_optional_entropy: *const DataBlob,
            pv_reserved: *mut c_void,
            p_prompt_struct: *const c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(h_mem: *mut c_void) -> *mut c_void;
    }

    /// DPAPI 加密:返回密文 blob。
    pub fn protect(plain: &[u8]) -> Result<Vec<u8>, String> {
        unsafe {
            let input = DataBlob {
                cb_data: plain.len() as u32,
                pb_data: plain.as_ptr() as *mut u8,
            };
            let mut output = DataBlob {
                cb_data: 0,
                pb_data: std::ptr::null_mut(),
            };
            let ok = CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                0,
                &mut output,
            );
            if ok == 0 || output.pb_data.is_null() {
                return Err("CryptProtectData 调用失败".into());
            }
            let out = std::slice::from_raw_parts(output.pb_data, output.cb_data as usize).to_vec();
            LocalFree(output.pb_data as *mut c_void);
            Ok(out)
        }
    }

    /// DPAPI 解密。
    pub fn unprotect(blob: &[u8]) -> Result<Vec<u8>, String> {
        unsafe {
            let input = DataBlob {
                cb_data: blob.len() as u32,
                pb_data: blob.as_ptr() as *mut u8,
            };
            let mut output = DataBlob {
                cb_data: 0,
                pb_data: std::ptr::null_mut(),
            };
            let ok = CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                0,
                &mut output,
            );
            if ok == 0 || output.pb_data.is_null() {
                return Err("CryptUnprotectData 调用失败(密钥不匹配或文件损坏)".into());
            }
            let out = std::slice::from_raw_parts(output.pb_data, output.cb_data as usize).to_vec();
            LocalFree(output.pb_data as *mut c_void);
            Ok(out)
        }
    }
}

// ---------------- AES-GCM 回退(非 Windows / 旧格式兼容) ----------------

fn derive_key() -> [u8; 32] {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "unknown".into());
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".into());

    let mut h = Sha256::new();
    h.update(APP_SALT);
    h.update(b"|");
    h.update(user.as_bytes());
    h.update(b"|");
    h.update(host.as_bytes());
    let digest = h.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

/// 非 Windows 平台的加密回退实现;Windows 上 DPAPI 已覆盖,故允许其“未使用”。
#[cfg_attr(target_os = "windows", allow(dead_code))]
fn aes_encrypt(provider_id: &str, api_key: &str) -> Result<String, String> {
    let key = derive_key();
    let cipher = Aes256Gcm::new((&key).into());

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(
            nonce,
            Payload {
                msg: api_key.as_bytes(),
                aad: provider_id.as_bytes(),
            },
        )
        .map_err(|e| format!("encrypt failed: {e}"))?;

    let mut blob = Vec::with_capacity(12 + ciphertext.len());
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ciphertext);
    Ok(B64.encode(&blob))
}

fn aes_decrypt(provider_id: &str, encoded: &str) -> Result<String, String> {
    let blob = B64
        .decode(encoded.trim())
        .map_err(|e| format!("base64 decode failed: {e}"))?;
    if blob.len() < 12 {
        return Err("secret file corrupted".into());
    }
    let (nonce_bytes, ciphertext) = blob.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let key = derive_key();
    let cipher = Aes256Gcm::new((&key).into());
    let plain = cipher
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad: provider_id.as_bytes(),
            },
        )
        .map_err(|_| "decrypt failed (key mismatch or corrupted)".to_string())?;
    String::from_utf8(plain).map_err(|e| format!("utf-8 decode failed: {e}"))
}

// ---------------- 公开接口 ----------------

/// 加密并存储指定槽位的 API Key。Windows 优先 DPAPI,其他平台回退 AES-GCM。
pub fn store_slot(provider_id: &str, slot: u32, api_key: &str) -> Result<(), String> {
    let dir = secrets_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir secrets failed: {e}"))?;

    #[cfg(target_os = "windows")]
    let encoded = {
        let blob = dpapi::protect(api_key.as_bytes())?;
        format!("{}{}", DPAPI_PREFIX, B64.encode(&blob))
    };
    #[cfg(not(target_os = "windows"))]
    let encoded = aes_encrypt(provider_id, api_key)?;

    fs::write(slot_path(provider_id, slot)?, encoded)
        .map_err(|e| format!("write secret failed: {e}"))?;
    log::info!("api key stored for provider {} slot {}", provider_id, slot);
    Ok(())
}

/// 读取并解密指定槽位的 API Key。文件不存在返回 Ok(None)。
/// 自动识别 `dpapi:` 前缀新格式与旧 AES-GCM 格式。
pub fn load_slot(provider_id: &str, slot: u32) -> Result<Option<String>, String> {
    let p = slot_path(provider_id, slot)?;
    if !p.exists() {
        return Ok(None);
    }
    let encoded = fs::read_to_string(&p).map_err(|e| format!("read secret failed: {e}"))?;
    let trimmed = encoded.trim();

    let plain = if let Some(b64) = trimmed.strip_prefix(DPAPI_PREFIX) {
        #[cfg(target_os = "windows")]
        {
            let blob = B64
                .decode(b64)
                .map_err(|e| format!("base64 decode failed: {e}"))?;
            let bytes = dpapi::unprotect(&blob)?;
            String::from_utf8(bytes).map_err(|e| format!("utf-8 decode failed: {e}"))?
        }
        #[cfg(not(target_os = "windows"))]
        {
            return Err("DPAPI 密文仅能在原 Windows 用户会话解密".into());
        }
    } else {
        aes_decrypt(provider_id, trimmed)?
    };
    Ok(Some(plain))
}

/// 删除指定槽位的 Key(不存在时静默)。
pub fn delete_slot(provider_id: &str, slot: u32) -> Result<(), String> {
    let p = slot_path(provider_id, slot)?;
    if p.exists() {
        fs::remove_file(&p).map_err(|e| format!("remove secret failed: {e}"))?;
    }
    Ok(())
}

/// 列出某 provider 已存在 Key 的槽位号(升序)。slot 0 为历史遗留的单 Key 文件。
pub fn list_slots(provider_id: &str) -> Result<Vec<u32>, String> {
    crate::memory::validate_id(provider_id)?;
    let dir = secrets_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let legacy = format!("{}.bin", provider_id);
    let prefix = format!("{}.", provider_id);
    let mut slots: Vec<u32> = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let Some(s) = name.to_str() else { continue };
        if s == legacy {
            slots.push(0);
            continue;
        }
        // `{id}.{n}.bin`(id 不含点号,故解析唯一)
        if let Some(rest) = s.strip_prefix(&prefix)
            && let Some(n) = rest.strip_suffix(".bin")
            && let Ok(v) = n.parse::<u32>()
        {
            slots.push(v);
        }
    }
    slots.sort_unstable();
    slots.dedup();
    Ok(slots)
}

/// 加密并存储 API Key(slot 0)。
pub fn store(provider_id: &str, api_key: &str) -> Result<(), String> {
    store_slot(provider_id, 0, api_key)
}

/// 读取并解密 API Key(slot 0)。文件不存在返回 Ok(None)。
pub fn load(provider_id: &str) -> Result<Option<String>, String> {
    load_slot(provider_id, 0)
}

/// 删除某 provider 的全部 Key(所有槽位)。
pub fn remove(provider_id: &str) -> Result<(), String> {
    crate::memory::validate_id(provider_id)?;
    for slot in list_slots(provider_id)? {
        delete_slot(provider_id, slot)?;
    }
    // 兜底:即便 list_slots 未识别到文件名,也尝试直接删 slot 0
    let p = secret_path(provider_id)?;
    if p.exists() {
        fs::remove_file(&p).map_err(|e| format!("remove secret failed: {e}"))?;
    }
    Ok(())
}

/// 列出已存 Key 的 provider id 列表(含多槽位文件)。
pub fn list_stored() -> Result<Vec<String>, String> {
    let dir = secrets_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out: Vec<String> = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if let Some(s) = name.to_str()
            && let Some(stem) = s.strip_suffix(".bin")
        {
            // `{id}` 或 `{id}.{n}`,统一取点号前的 provider_id
            let pid = stem.split('.').next().unwrap_or(stem).to_string();
            if !pid.is_empty() {
                out.push(pid);
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// 防止模块未使用的死代码警告;db 模块在这里被引用来确保 data 目录初始化完成。
#[allow(dead_code)]
fn _ensure_db_init() {
    let _ = db::conn;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encrypt_decrypt() {
        let pid = "test_provider_roundtrip";
        let key = "sk-test-1234567890abcdef";
        store(pid, key).unwrap();
        let got = load(pid).unwrap();
        assert_eq!(got, Some(key.to_string()));
        remove(pid).unwrap();
        assert_eq!(load(pid).unwrap(), None);
    }

    #[test]
    fn different_provider_different_file() {
        store("p_a", "key_a").unwrap();
        store("p_b", "key_b").unwrap();
        assert_eq!(load("p_a").unwrap().unwrap(), "key_a");
        assert_eq!(load("p_b").unwrap().unwrap(), "key_b");
        remove("p_a").unwrap();
        remove("p_b").unwrap();
    }

    #[test]
    fn list_stored_returns_ids() {
        store("p_list_1", "k1").unwrap();
        store("p_list_2", "k2").unwrap();
        let ids = list_stored().unwrap();
        assert!(ids.contains(&"p_list_1".to_string()));
        assert!(ids.contains(&"p_list_2".to_string()));
        remove("p_list_1").unwrap();
        remove("p_list_2").unwrap();
    }

    /// 多槽位:各槽位独立存储,互不影响;list_slots 精确枚举。
    #[test]
    fn slot_roundtrip_and_listing() {
        let pid = "test_provider_slots";
        store_slot(pid, 0, "k0").unwrap();
        store_slot(pid, 1, "k1").unwrap();
        store_slot(pid, 3, "k3").unwrap();
        assert_eq!(load_slot(pid, 0).unwrap().unwrap(), "k0");
        assert_eq!(load_slot(pid, 1).unwrap().unwrap(), "k1");
        assert_eq!(load_slot(pid, 2).unwrap(), None);
        assert_eq!(load_slot(pid, 3).unwrap().unwrap(), "k3");
        assert_eq!(list_slots(pid).unwrap(), vec![0, 1, 3]);
        delete_slot(pid, 1).unwrap();
        assert_eq!(list_slots(pid).unwrap(), vec![0, 3]);
        // remove 清空全部槽位
        remove(pid).unwrap();
        assert!(list_slots(pid).unwrap().is_empty());
    }

    /// list_stored 对多槽位 provider 去重后只出现一次。
    #[test]
    fn list_stored_dedupes_multislot() {
        let pid = "p_multi_stored";
        store_slot(pid, 0, "k0").unwrap();
        store_slot(pid, 1, "k1").unwrap();
        let ids = list_stored().unwrap();
        assert_eq!(ids.iter().filter(|s| *s == pid).count(), 1);
        remove(pid).unwrap();
    }

    /// 槽位路径同样拒绝路径遍历。
    #[test]
    fn slot_path_rejects_traversal() {
        assert!(slot_path("../../evil", 1).is_err());
        assert!(store_slot("a/b", 1, "k").is_err());
        assert!(load_slot("a\\b", 1).is_err());
        assert!(delete_slot("../../evil", 2).is_err());
        assert!(list_slots("../evil").is_err());
    }

    /// H2 回归:路径遍历 provider_id 必须在 secret_path 被拒绝。
    #[test]
    fn secret_path_rejects_traversal() {
        assert!(secret_path("../../evil").is_err());
        assert!(secret_path("a/b").is_err());
        assert!(secret_path("a\\b").is_err());
        assert!(secret_path("").is_err());
        // 入口层同步覆盖:store/load/remove 均应直接报错且不触盘
        assert!(store("../../evil", "k").is_err());
        assert!(load("../../evil").is_err());
        assert!(remove("../../evil").is_err());
    }
}
