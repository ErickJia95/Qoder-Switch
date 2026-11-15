//! 读取 / 写入本机 Qoder IDE 的登录态。
//!
//! Qoder IDE（CN 与 Intl）都用 Chromium/Electron safeStorage (OSCrypt v10) 加密认证数据：
//!   - 文件: ~/Library/Application Support/<app-id>/auth.v1.dat
//!   - 格式: "v10" (3 bytes) + AES-128-CBC 密文 (PKCS7 padding)
//!   - 密钥: PBKDF2-HMAC-SHA1(keychain 密码, "saltysalt", 1003, 16 bytes)
//!   - IV:   16 个空格 (0x20)
//!
//! 解密后的 JSON 形如：
//! `{"schemaVersion":1,"authenticationContext":"[...]","token":"dt-...",
//!   "refreshToken":"drt-...","expiresAt":"...","refreshTokenExpiresAt":"...",
//!   "user":{"id":"...","name":"...","email":"...","phone":"...","avatarUrl":"..."}}`

#[derive(Debug, Clone)]
pub struct LocalAccount {
    pub token: String,
    pub user_id: String,
    pub name: String,
    pub email: String,
    pub expires_at: String,
    /// 解密后的 auth.v1.dat 原文，切换账号时原样写回
    pub auth_json: String,
    /// "cn" 或 "intl"
    pub ide_kind: String,
}

/// 读取本机 Qoder IDE 当前登录账号（两个版本都试）
pub fn read_current_account() -> Result<LocalAccount, String> {
    macos_impl::read_macos()
}

/// 把某个账号的登录数据写回对应 IDE 的 auth.v1.dat
pub fn write_auth(kind: &str, auth_json: &str) -> Result<(), String> {
    macos_impl::write_auth_file(kind, auth_json)
}

/// 读取某个版本当前的登录数据（切换后校验用）
pub fn read_account_for(kind: &str) -> Option<LocalAccount> {
    macos_impl::read_account_for(kind)
}

/// 把上次切换前备份的登录态写回去（切换失败时的退路）
pub fn restore_backup(kind: &str) -> Result<(), String> {
    macos_impl::restore_backup(kind)
}

/// 由 token + 用户信息拼一个最小可用的 auth 数据（缺少 refreshToken）。
/// ⚠️ 仅保留给测试/诊断用：实测 Qoder IDE 0.5.11 的 AuthStore 启动校验会拒绝这种
/// 缺 refreshToken 的数据（Credential storage diagnostic result:invalid stage:validate）
/// 并清除 auth.v1.dat，导致切换后掉登录。activate_account 已不再调用此函数。
#[allow(dead_code)]
pub fn build_minimal_auth(
    kind: &str,
    token: &str,
    user_id: &str,
    name: &str,
    email: &str,
    expire_date: &str,
) -> String {
    macos_impl::build_minimal_auth(kind, token, user_id, name, email, expire_date)
}

mod macos_impl {
    use super::LocalAccount;
    use serde_json::{json, Value};
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// (kind, Application Support 目录, Keychain service, Keychain account)
    const APPS: &[(&str, &str, &str, &str)] = &[
        (
            "cn",
            "com.qodercn.app.stable",
            "Qoder CN App Safe Storage",
            "Qoder CN App Key",
        ),
        (
            "intl",
            "com.qoder.app.stable",
            "Qoder App Safe Storage",
            "Qoder App Key",
        ),
    ];

    fn app_meta(kind: &str) -> Option<(&'static str, &'static str, &'static str)> {
        APPS.iter()
            .find(|(k, _, _, _)| *k == kind)
            .map(|(_, dir, svc, acc)| (*dir, *svc, *acc))
    }

    fn data_dir() -> Result<PathBuf, String> {
        dirs::data_dir().ok_or_else(|| "无法定位 Application Support 目录".to_string())
    }

    fn auth_path(kind: &str) -> Result<PathBuf, String> {
        let (dir, _, _) = app_meta(kind).ok_or_else(|| format!("未知的 IDE 版本: {}", kind))?;
        Ok(data_dir()?.join(dir).join("auth.v1.dat"))
    }

    pub(super) fn read_macos() -> Result<LocalAccount, String> {
        let base = data_dir()?;

        let mut files: Vec<(String, PathBuf)> = Vec::new();
        for (kind, dir, _, _) in APPS {
            let p = base.join(dir).join("auth.v1.dat");
            if p.exists() {
                files.push(((*kind).to_string(), p));
            }
        }
        if files.is_empty() {
            return Err("未找到 Qoder IDE 的登录数据，请先在 Qoder IDE 中登录账号".to_string());
        }

        // 收集所有可用的 Keychain 密码（两个版本都试，避免版本串味）
        let mut passwords: Vec<String> = Vec::new();
        for (_, _, svc, acct) in APPS {
            if let Some(pwd) = get_keychain_password(svc, acct) {
                if !passwords.contains(&pwd) {
                    passwords.push(pwd);
                }
            }
        }
        if passwords.is_empty() {
            return Err("无法从钥匙串读取 Qoder 的加密密钥（可能被拒绝访问）".to_string());
        }

        let mut last_err = String::new();
        for (kind, file) in &files {
            let encrypted =
                std::fs::read(file).map_err(|e| format!("读取 {:?} 失败: {}", file, e))?;
            for pwd in &passwords {
                match decrypt(&encrypted, pwd) {
                    Ok(plain) => match parse_account(&plain) {
                        Ok(mut account) => {
                            account.auth_json = plain;
                            account.ide_kind = kind.clone();
                            log::info!(
                                "[local_account] 已从 {:?} 读取到账号 user_id={}",
                                file,
                                account.user_id
                            );
                            return Ok(account);
                        }
                        Err(e) => last_err = e,
                    },
                    Err(e) => last_err = e,
                }
            }
        }

        Err(format!(
            "无法解析 Qoder 登录数据（IDE 版本可能已更新加密格式）: {}",
            last_err
        ))
    }

    /// 写入 auth.v1.dat：先留底旧文件，再原子替换
    pub(super) fn write_auth_file(kind: &str, auth_json: &str) -> Result<(), String> {
        let path = auth_path(kind)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("创建目录失败: {}", e))?;
        }

        let (_, svc, acct) =
            app_meta(kind).ok_or_else(|| format!("未知的 IDE 版本: {}", kind))?;
        let pwd = get_keychain_password(svc, acct)
            .ok_or_else(|| "无法从钥匙串读取加密密钥（可能被拒绝访问）".to_string())?;

        // 留底：把现有文件复制到本应用数据目录，出问题可手工覆盖回去
        if path.exists() {
            if let Ok(dir) = data_dir() {
                let backup = backup_path(kind).unwrap_or_else(|_| dir.join("Qoder").join("auth-backup.dat"));
                if let Some(parent) = backup.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::copy(&path, &backup);
                log::info!("[local_account] 已备份原 auth.v1.dat 到 {:?}", backup);
            }
        }

        write_auth_to(&path, &pwd, auth_json)?;
        log::info!("[local_account] 已写入 {:?}", path);
        Ok(())
    }

    /// 加密写入 + 回读校验（密码显式传入，便于单测）
    fn write_auth_to(path: &Path, pwd: &str, auth_json: &str) -> Result<(), String> {
        let ciphertext = encrypt(auth_json.as_bytes(), pwd)?;
        let tmp = path.with_extension("dat.tmp");
        std::fs::write(&tmp, ciphertext).map_err(|e| format!("写入临时文件失败: {}", e))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("替换 auth.v1.dat 失败: {}", e))?;

        let raw = std::fs::read(path).map_err(|e| format!("回读失败: {}", e))?;
        let back = decrypt(&raw, pwd)?;
        if back != auth_json {
            return Err("写入校验失败：解密结果与预期不一致".to_string());
        }
        Ok(())
    }

    /// 备份文件位置：~/Library/Application Support/Qoder/auth-backup-<kind>.dat
    fn backup_path(kind: &str) -> Result<PathBuf, String> {
        Ok(data_dir()?.join("Qoder").join(format!("auth-backup-{}.dat", kind)))
    }

    pub(super) fn restore_backup(kind: &str) -> Result<(), String> {
        let backup = backup_path(kind)?;
        if !backup.exists() {
            return Err("还没有备份，无法恢复".to_string());
        }
        let raw = std::fs::read(&backup).map_err(|e| format!("读取备份失败: {}", e))?;
        let target = auth_path(kind)?;
        std::fs::write(&target, raw).map_err(|e| format!("写回 auth.v1.dat 失败: {}", e))?;
        log::info!("[local_account] 已从 {:?} 恢复登录态", backup);
        Ok(())
    }

    pub(super) fn read_account_for(kind: &str) -> Option<LocalAccount> {
        let path = auth_path(kind).ok()?;
        let raw = std::fs::read(path).ok()?;
        let (_, svc, acct) = app_meta(kind)?;
        let pwd = get_keychain_password(svc, acct)?;
        let plain = decrypt(&raw, &pwd).ok()?;
        let mut account = parse_account(&plain).ok()?;
        account.auth_json = plain;
        account.ide_kind = kind.to_string();
        Some(account)
    }

    pub(super) fn read_auth_file(kind: &str) -> Option<String> {
        let path = auth_path(kind).ok()?;
        let raw = std::fs::read(path).ok()?;
        let (_, svc, acct) = app_meta(kind)?;
        let pwd = get_keychain_password(svc, acct)?;
        decrypt(&raw, &pwd).ok()
    }

    /// 用最小字段拼 auth 数据：复用当前 authenticationContext（含 region/环境），
    /// 不带 refreshToken。⚠️ 仅测试/诊断用：IDE AuthStore 会拒绝并清除这种数据。
    #[allow(dead_code)]
    pub(super) fn build_minimal_auth(
        kind: &str,
        token: &str,
        user_id: &str,
        name: &str,
        email: &str,
        expire_date: &str,
    ) -> String {
        let ctx = read_auth_file(kind)
            .and_then(|txt| serde_json::from_str::<Value>(&txt).ok())
            .and_then(|v| v.get("authenticationContext").cloned())
            .unwrap_or_else(|| Value::String(default_context(kind)));

        let expires_at = if expire_date.is_empty() {
            String::new()
        } else {
            format!("{}T23:59:59Z", expire_date)
        };

        json!({
            "schemaVersion": 1,
            "authenticationContext": ctx,
            "token": token,
            "expiresAt": expires_at,
            "user": {
                "id": user_id,
                "name": name,
                "email": email,
            }
        })
        .to_string()
    }

    fn default_context(kind: &str) -> String {
        if kind == "intl" {
            r#"["https://qoder.com","https://openapi.qoder.sh","prod"]"#.to_string()
        } else {
            r#"["https://qoder.cn","https://openapi.qoder.com.cn","prod"]"#.to_string()
        }
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
        if pwd.is_empty() {
            None
        } else {
            Some(pwd)
        }
    }

    fn derive_key(password: &str) -> [u8; 16] {
        let mut key = [0u8; 16];
        pbkdf2_sha1(password.as_bytes(), b"saltysalt", 1003, &mut key);
        key
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

        let key = derive_key(password);
        let iv = [0x20u8; 16];

        let plaintext = aes128_cbc_decrypt(ciphertext, &key, &iv)?;
        String::from_utf8(plaintext)
            .map_err(|e| format!("解密结果不是有效 UTF-8: {}", e))
    }

    /// Chromium OSCrypt v10 加密（输出 "v10" + 密文），与 IDE 自身写出的格式逐字节一致
    fn encrypt(plaintext: &[u8], password: &str) -> Result<Vec<u8>, String> {
        let key = derive_key(password);
        let iv = [0x20u8; 16];
        let body = aes128_cbc_encrypt(plaintext, &key, &iv)?;
        let mut out = Vec::with_capacity(3 + body.len());
        out.extend_from_slice(b"v10");
        out.extend_from_slice(&body);
        Ok(out)
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

    /// AES-128-CBC + PKCS7 加密
    fn aes128_cbc_encrypt(
        plaintext: &[u8],
        key: &[u8; 16],
        iv: &[u8; 16],
    ) -> Result<Vec<u8>, String> {
        use aes::Aes128;
        use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
        use cbc::Encryptor;

        type Aes128CbcEnc = Encryptor<Aes128>;

        let enc = Aes128CbcEnc::new_from_slices(key, iv)
            .map_err(|e| format!("初始化 AES 失败: {}", e))?;
        let mut buf = vec![0u8; plaintext.len() + 16];
        buf[..plaintext.len()].copy_from_slice(plaintext);
        let out = enc
            .encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len())
            .map_err(|e| format!("AES 加密失败: {}", e))?;
        Ok(out.to_vec())
    }

    /// 从解密后的 JSON 提取账号字段
    fn parse_account(json_str: &str) -> Result<LocalAccount, String> {
        let v: Value =
            serde_json::from_str(json_str).map_err(|e| format!("解析登录数据 JSON 失败: {}", e))?;

        let token = v
            .get("token")
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
            expires_at: v
                .get("expiresAt")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            auth_json: json_str.to_string(),
            ide_kind: String::new(),
        })
    }

    /// 供测试与调试使用：把任意路径的 v10 文件解出来
    #[allow(dead_code)]
    fn decrypt_file(path: &Path, password: &str) -> Result<String, String> {
        let raw = std::fs::read(path).map_err(|e| format!("读取失败: {}", e))?;
        decrypt(&raw, password)
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
        }

        #[test]
        fn encrypt_matches_chromium_output() {
            // 用同一密码加密相同明文，必须逐字节复现上面的样本
            const ENCRYPTED: &str = "763130d9da6e467649eeb2b9440986fc835e419cdfc499a356d5394261462ad7e479f3118920a79e83a98e8ef209ffc7b30c1bb6cde341557842d4d3c10f53f32500d14f15a99b67232861291662e147622298";
            const PLAIN: &str =
                r#"{"token":"unit-test-token","user":{"id":"u1","name":"n","email":"e"}}"#;
            let out = encrypt(PLAIN.as_bytes(), "unit-test-password").unwrap();
            assert_eq!(out, from_hex(ENCRYPTED));
        }

        #[test]
        fn write_auth_file_roundtrip() {
            let dir = std::env::temp_dir().join(format!("qsw-auth-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("auth.v1.dat");

            let json = r#"{"schemaVersion":1,"token":"dt-x","refreshToken":"drt-y","expiresAt":"2026-12-14T13:45:14Z","user":{"id":"u1","name":"甲","email":"a@x.com"}}"#;
            write_auth_to(&path, "unit-test-password", json).unwrap();

            let raw = std::fs::read(&path).unwrap();
            assert_eq!(&raw[..3], b"v10");
            assert_eq!(decrypt(&raw, "unit-test-password").unwrap(), json);

            // 写进去的 JSON 必须还能被 parse_account 解析成账号
            let acc = parse_account(json).unwrap();
            assert_eq!(acc.token, "dt-x");
            assert_eq!(acc.user_id, "u1");
            assert_eq!(acc.email, "a@x.com");

            // 用错密码必须解不开
            assert!(decrypt(&raw, "wrong-password").is_err());

            std::fs::remove_dir_all(&dir).ok();
        }

        #[test]
        fn minimal_auth_has_expected_shape() {
            let txt = build_minimal_auth("cn", "tok", "u1", "甲", "a@x.com", "2026-12-11");
            let v: Value = serde_json::from_str(&txt).unwrap();
            assert_eq!(v["token"], "tok");
            assert_eq!(v["user"]["id"], "u1");
            assert_eq!(v["expiresAt"], "2026-12-11T23:59:59Z");
            assert!(v.get("refreshToken").is_none());
        }
    }
}
