//! 从本机 Qoder IDE 的登录态读取当前账号凭证。
//!
//! Qoder IDE 使用 Chromium/Electron safeStorage (OSCrypt v10, macOS) 加密认证数据：
//!   - 文件: ~/Library/Application Support/<app-id>/auth.v1.dat
//!   - 格式: "v10" (3 bytes) + AES-128-CBC 密文 (PKCS7 padding)
//!   - 密钥: PBKDF2-HMAC-SHA1(keychain 密码, "saltysalt", 1003, 16 bytes)
//!   - IV:   16 个空格 (0x20)
//!
//! 解密后的 JSON 含 token / user.id / user.name / user.email / expiresAt。

#[derive(Debug, Clone)]
pub struct LocalAccount {
    pub token: String,
    pub user_id: String,
    pub name: String,
    pub email: String,
    pub expires_at: String,
}

/// 读取本机 Qoder IDE 当前登录账号
pub fn read_current_account() -> Result<LocalAccount, String> {
    macos_impl::read_macos()
}

mod macos_impl {
    use super::LocalAccount;
    use serde_json::Value;
    use std::path::PathBuf;
    use std::process::Command;

    /// Qoder IDE 可能的 Application Support 目录
    const APP_DIRS: &[&str] = &[
        "com.qodercn.app.stable",
        "com.qoder.app.stable",
    ];

    /// Keychain 中可能的 (service, account) 组合
    const KEYCHAIN_ENTRIES: &[(&str, &str)] = &[
        ("Qoder CN App Safe Storage", "Qoder CN App Key"),
        ("Qoder App Safe Storage", "Qoder App Key"),
    ];

    pub(super) fn read_macos() -> Result<LocalAccount, String> {
        let base = dirs::data_dir().ok_or_else(|| "无法定位 Application Support 目录".to_string())?;

        // 收集所有存在的 auth.v1.dat
        let mut files: Vec<PathBuf> = Vec::new();
        for dir in APP_DIRS {
            let p = base.join(dir).join("auth.v1.dat");
            if p.exists() {
                files.push(p);
            }
        }
        if files.is_empty() {
            return Err(
                "未找到 Qoder IDE 的登录数据，请先在 Qoder IDE 中登录账号".to_string()
            );
        }

        // 收集所有可用的 Keychain 密码
        let mut passwords: Vec<String> = Vec::new();
        for (service, account) in KEYCHAIN_ENTRIES {
            if let Some(pwd) = get_keychain_password(service, account) {
                if !passwords.contains(&pwd) {
                    passwords.push(pwd);
                }
            }
        }
        if passwords.is_empty() {
            return Err("无法从钥匙串读取 Qoder 的加密密钥（可能被拒绝访问）".to_string());
        }

        // 尝试所有 (文件, 密码) 组合，取第一个能解出 token 的
        let mut last_err = String::new();
        for file in &files {
            let encrypted = std::fs::read(file)
                .map_err(|e| format!("读取 {:?} 失败: {}", file, e))?;
            for pwd in &passwords {
                match decrypt(&encrypted, pwd).and_then(|json| parse_account(&json)) {
                    Ok(account) => {
                        log::info!(
                            "[local_account] 已从 {:?} 读取到账号 user_id={}",
                            file,
                            account.user_id
                        );
                        return Ok(account);
                    }
                    Err(e) => last_err = e,
                }
            }
        }

        Err(format!(
            "无法解析 Qoder 登录数据（IDE 版本可能已更新加密格式）: {}",
            last_err
        ))
    }

    /// 从 macOS 钥匙串读取通用密码
    fn get_keychain_password(service: &str, account: &str) -> Option<String> {
        let output = Command::new("security")
            .args(["find-generic-password", "-s", service, "-a", account, "-w"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let pwd = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if pwd.is_empty() { None } else { Some(pwd) }
    }

    /// Chromium OSCrypt v10 解密
    fn decrypt(data: &[u8], password: &str) -> Result<String, String> {
        if data.len() < 19 || &data[..3] != b"v10" {
            return Err("不是有效的 v10 加密格式".to_string());
        }
        let ciphertext = &data[3..];
        if ciphertext.len() % 16 != 0 {
            return Err("密文长度不是 16 的倍数".to_string());
        }

        let mut key = [0u8; 16];
        pbkdf2_sha1(password.as_bytes(), b"saltysalt", 1003, &mut key);
        let iv = [0x20u8; 16];

        let plaintext = aes128_cbc_decrypt(ciphertext, &key, &iv)?;
        String::from_utf8(plaintext)
            .map_err(|e| format!("解密结果不是有效 UTF-8: {}", e))
    }

    /// AES-128-CBC + PKCS7 解密
    fn aes128_cbc_decrypt(
        ciphertext: &[u8],
        key: &[u8; 16],
        iv: &[u8; 16],
    ) -> Result<Vec<u8>, String> {
        use aes::Aes128;
        use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
        use cbc::Decryptor;

        type Aes128CbcDec = Decryptor<Aes128>;

        let dec = Aes128CbcDec::new_from_slices(key, iv)
            .map_err(|e| format!("初始化 AES 失败: {}", e))?;
        let mut buf = ciphertext.to_vec();
        let plain = dec
            .decrypt_padded_mut::<Pkcs7>(&mut buf)
            .map_err(|e| format!("AES 解密/去填充失败: {}", e))?;
        Ok(plain.to_vec())
    }

    /// 从解密后的 JSON 提取账号字段
    fn parse_account(json_str: &str) -> Result<LocalAccount, String> {
        let v: Value = serde_json::from_str(json_str)
            .map_err(|e| format!("解析登录数据 JSON 失败: {}", e))?;

        let token = v.get("token")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        if token.is_empty() {
            return Err("登录数据中缺少 token 字段".to_string());
        }

        let user = v.get("user");
        let pick = |field: &str| -> String {
            user.and_then(|u| u.get(field))
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string()
        };

        Ok(LocalAccount {
            token,
            user_id: pick("id"),
            name: pick("name"),
            email: pick("email"),
            expires_at: v.get("expiresAt")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
        })
    }

    // ===== PBKDF2-HMAC-SHA1（手写，避免额外依赖）=====

    use sha1::{Digest, Sha1};

    fn hmac_sha1(key: &[u8], data: &[u8]) -> [u8; 20] {
        const BLOCK: usize = 64;
        let mut k = [0u8; BLOCK];
        if key.len() > BLOCK {
            let mut h = Sha1::new();
            h.update(key);
            let digest = h.finalize();
            k[..20].copy_from_slice(&digest);
        } else {
            k[..key.len()].copy_from_slice(key);
        }

        let mut inner = Vec::with_capacity(BLOCK + data.len());
        for b in &k {
            inner.push(b ^ 0x36);
        }
        inner.extend_from_slice(data);
        let mut h = Sha1::new();
        h.update(&inner);
        let ih = h.finalize();

        let mut outer = Vec::with_capacity(BLOCK + 20);
        for b in &k {
            outer.push(b ^ 0x5c);
        }
        outer.extend_from_slice(&ih);
        let mut h = Sha1::new();
        h.update(&outer);
        h.finalize().into()
    }

    fn pbkdf2_sha1(password: &[u8], salt: &[u8], rounds: u32, dk: &mut [u8]) {
        const HLEN: usize = 20;
        let blocks = (dk.len() + HLEN - 1) / HLEN;
        for i in 1..=blocks {
            let mut s = Vec::with_capacity(salt.len() + 4);
            s.extend_from_slice(salt);
            s.extend_from_slice(&(i as u32).to_be_bytes());

            let mut u = hmac_sha1(password, &s);
            let mut t = u;
            for _ in 1..rounds {
                u = hmac_sha1(password, &u);
                for j in 0..HLEN {
                    t[j] ^= u[j];
                }
            }

            let start = (i - 1) * HLEN;
            let n = (dk.len() - start).min(HLEN);
            dk[start..start + n].copy_from_slice(&t[..n]);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn from_hex(s: &str) -> Vec<u8> {
            (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
                .collect()
        }

        #[test]
        fn pbkdf2_rfc6070_vector() {
            // RFC 6070: PBKDF2-HMAC-SHA1("password", "salt", c=1, dkLen=20)
            let mut dk = [0u8; 20];
            pbkdf2_sha1(b"password", b"salt", 1, &mut dk);
            let hex: String = dk.iter().map(|b| format!("{:02x}", b)).collect();
            assert_eq!(hex, "0c60c80f961f0e71f3a9b524af6012062fe037a6");
        }

        #[test]
        fn decrypt_v10_roundtrip() {
            // 由 Chromium OSCrypt v10 加密的样本（Node crypto 生成）
            const ENCRYPTED: &str = "763130d9da6e467649eeb2b9440986fc835e419cdfc499a356d5394261462ad7e479f3118920a79e83a98e8ef209ffc7b30c1bb6cde341557842d4d3c10f53f32500d14f15a99b67232861291662e147622298";
            const EXPECTED: &str =
                r#"{"token":"unit-test-token","user":{"id":"u1","name":"n","email":"e"}}"#;

            let plain = decrypt(&from_hex(ENCRYPTED), "unit-test-password").unwrap();
            assert_eq!(plain, EXPECTED);

            let account = parse_account(&plain).unwrap();
            assert_eq!(account.token, "unit-test-token");
            assert_eq!(account.user_id, "u1");
            assert_eq!(account.name, "n");
            assert_eq!(account.email, "e");
        }
    }
}
