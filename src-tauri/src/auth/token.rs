use aes_gcm::{Aes256Gcm, AeadCore, aead::{Aead, KeyInit, OsRng}};
use aes_gcm::aead::generic_array::GenericArray;
use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountToken {
    pub id: String,
    pub label: String,
    pub token: String,
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub user_type: String,
    #[serde(default)]
    pub expire_date: String,
    #[serde(default)]
    pub quota_used: Option<u64>,
    #[serde(default)]
    pub quota_total: Option<u64>,
    /// 该账号最近一次从 IDE 读取到的 auth.v1.dat 明文（含 refreshToken），切换账号时原样写回
    #[serde(default)]
    pub auth_blob: String,
    /// 该账号属于哪个版本："cn" / "intl"
    #[serde(default)]
    pub ide_kind: String,
    #[serde(default)]
    pub machine_token: String,
    #[serde(default)]
    pub machine_id: String,
    #[serde(default)]
    pub machine_code: String,
    #[serde(default)]
    pub machine_type: String,
}

pub struct TokenManager {
    accounts: Arc<RwLock<Vec<AccountToken>>>,
    current: Arc<RwLock<usize>>,
    storage_path: PathBuf,
}

impl TokenManager {
    pub fn new(storage_path: PathBuf) -> Self {
        Self {
            accounts: Arc::new(RwLock::new(Vec::new())),
            current: Arc::new(RwLock::new(0)),
            storage_path,
        }
    }

    #[allow(dead_code)]
    pub async fn add(&self, token: String, label: String, user_id: String, email: String, name: String, user_type: String, expire_date: String) {
        let id = format!("tok_{:x}", chrono::Utc::now().timestamp_millis());
        let entry = AccountToken {
            id,
            label,
            token,
            user_id,
            email,
            name,
            status: "available".to_string(),
            user_type,
            expire_date,
            quota_used: None,
            quota_total: None,
            auth_blob: String::new(),
            ide_kind: String::new(),
            machine_token: String::new(),
            machine_id: String::new(),
            machine_code: String::new(),
            machine_type: String::new(),
        };
        let mut accounts = self.accounts.write().await;
        accounts.push(entry);
        drop(accounts);
        if let Err(e) = self.save().await {
            log::warn!("保存账号失败: {}", e);
        }
    }

    /// 按 user_id 更新已有账号的凭证；若不存在则新增。
    /// 返回 true 表示更新了已有账号，false 表示新增。
    pub async fn upsert(
        &self,
        token: String,
        label: String,
        user_id: String,
        email: String,
        name: String,
        expire_date: String,
        auth_blob: String,
        ide_kind: String,
    ) -> bool {
        {
            let mut accounts = self.accounts.write().await;
            if !user_id.is_empty() {
                if let Some(a) = accounts.iter_mut().find(|a| a.user_id == user_id) {
                    a.token = token;
                    a.label = label;
                    a.email = email;
                    a.name = name;
                    if !expire_date.is_empty() {
                        a.expire_date = expire_date;
                    }
                    if !auth_blob.is_empty() {
                        a.auth_blob = auth_blob.clone();
                    }
                    if !ide_kind.is_empty() {
                        a.ide_kind = ide_kind.clone();
                    }
                    // 凭证已变，旧的额度缓存作废
                    a.quota_used = None;
                    a.quota_total = None;
                    a.status = "available".to_string();
                    drop(accounts);
                    let _ = self.save().await;
                    return true;
                }
            }
        }
        // 不存在则新增：登录数据一并写入，只保存一次
        let entry = AccountToken {
            id: format!("tok_{:x}", chrono::Utc::now().timestamp_millis()),
            label,
            token,
            user_id,
            email,
            name,
            status: "available".to_string(),
            user_type: String::new(),
            expire_date,
            quota_used: None,
            quota_total: None,
            auth_blob,
            ide_kind,
            machine_token: String::new(),
            machine_id: String::new(),
            machine_code: String::new(),
            machine_type: String::new(),
        };
        self.accounts.write().await.push(entry);
        let _ = self.save().await;
        false
    }

    pub async fn update_account_info(&self, id: &str, user_type: String, expire_date: String) {
        let mut accounts = self.accounts.write().await;
        if let Some(account) = accounts.iter_mut().find(|a| a.id == id) {
            account.user_type = user_type;
            account.expire_date = expire_date;
        }
        drop(accounts);
        let _ = self.save().await;
    }

    /// 切换成功后回写最新登录数据（IDE 可能已刷新 token）
    pub async fn refresh_credentials(
        &self,
        id: &str,
        token: String,
        expire_date: String,
        auth_blob: String,
        ide_kind: String,
    ) {
        let mut accounts = self.accounts.write().await;
        if let Some(a) = accounts.iter_mut().find(|a| a.id == id) {
            if !token.is_empty() {
                a.token = token;
            }
            if !expire_date.is_empty() {
                a.expire_date = expire_date;
            }
            if !auth_blob.is_empty() {
                a.auth_blob = auth_blob;
            }
            if !ide_kind.is_empty() {
                a.ide_kind = ide_kind;
            }
        }
        drop(accounts);
        let _ = self.save().await;
    }

    pub async fn update_account_quota(&self, id: &str, quota_used: Option<u64>, quota_total: Option<u64>) {
        let mut accounts = self.accounts.write().await;
        if let Some(account) = accounts.iter_mut().find(|a| a.id == id) {
            account.quota_used = quota_used;
            account.quota_total = quota_total;
        }
        drop(accounts);
        let _ = self.save().await;
    }

    /// 批量合并导入的账号（用于从 JSON 备份恢复）。
    /// 匹配顺序：user_id → id → email；命中则更新（只覆盖备份里非空字段），否则新增。
    /// 没有 token 且库中不存在的新账号无法使用，计入 skipped。
    /// 返回 (新增数, 更新数, 跳过数)。
    pub async fn merge_accounts(&self, incoming: Vec<AccountToken>) -> (usize, usize, usize) {
        let mut added = 0usize;
        let mut updated = 0usize;
        let mut skipped = 0usize;

        {
            let mut accounts = self.accounts.write().await;
            for inc in incoming {
                let pos = if !inc.user_id.is_empty() {
                    accounts.iter().position(|a| a.user_id == inc.user_id)
                } else {
                    None
                }
                .or_else(|| {
                    if inc.id.is_empty() {
                        None
                    } else {
                        accounts.iter().position(|a| a.id == inc.id)
                    }
                })
                .or_else(|| {
                    if inc.email.is_empty() {
                        None
                    } else {
                        accounts.iter().position(|a| a.email == inc.email)
                    }
                });

                match pos {
                    Some(i) => {
                        let a = &mut accounts[i];
                        if !inc.token.is_empty() {
                            a.token = inc.token.clone();
                        }
                        if !inc.label.is_empty() {
                            a.label = inc.label.clone();
                        }
                        if !inc.name.is_empty() {
                            a.name = inc.name.clone();
                        }
                        if !inc.email.is_empty() {
                            a.email = inc.email.clone();
                        }
                        if !inc.user_id.is_empty() {
                            a.user_id = inc.user_id.clone();
                        }
                        if !inc.user_type.is_empty() {
                            a.user_type = inc.user_type.clone();
                        }
                        if !inc.expire_date.is_empty() {
                            a.expire_date = inc.expire_date.clone();
                        }
                        if inc.quota_used.is_some() {
                            a.quota_used = inc.quota_used;
                        }
                        if inc.quota_total.is_some() {
                            a.quota_total = inc.quota_total;
                        }
                        if !inc.auth_blob.is_empty() {
                            a.auth_blob = inc.auth_blob.clone();
                        }
                        if !inc.ide_kind.is_empty() {
                            a.ide_kind = inc.ide_kind.clone();
                        }
                        updated += 1;
                    }
                    None => {
                        if inc.token.is_empty() {
                            skipped += 1;
                            continue;
                        }
                        let mut entry = inc;
                        if entry.id.is_empty() {
                            entry.id = format!("tok_{:x}", chrono::Utc::now().timestamp_micros());
                        }
                        entry.status = "available".to_string();
                        accounts.push(entry);
                        added += 1;
                    }
                }
            }
        }

        if let Err(e) = self.save().await {
            log::warn!("保存账号失败: {}", e);
        }
        (added, updated, skipped)
    }

    /// 按 user_id / email / id 查找账号 id（导入后恢复「当前账号」标记用）
    pub async fn find_id(&self, user_id: &str, email: &str) -> Option<String> {
        if user_id.is_empty() && email.is_empty() {
            return None;
        }
        let accounts = self.accounts.read().await;
        accounts
            .iter()
            .find(|a| (!user_id.is_empty() && a.user_id == user_id) || (!email.is_empty() && a.email == email))
            .map(|a| a.id.clone())
    }

    pub async fn remove(&self, id: &str) {
        let mut accounts = self.accounts.write().await;
        accounts.retain(|a| a.id != id);
        drop(accounts);
        if let Err(e) = self.save().await {
            log::warn!("保存账号失败: {}", e);
        }
    }

    pub async fn set_current(&self, id: &str) -> bool {
        let accounts = self.accounts.read().await;
        if let Some(idx) = accounts.iter().position(|a| a.id == id) {
            drop(accounts);
            let mut current = self.current.write().await;
            *current = idx;
            if let Err(e) = self.save().await {
                log::warn!("保存账号失败: {}", e);
            }
            true
        } else {
            false
        }
    }

    /// 按 id 取单个账号（含 auth_blob）
    pub async fn get(&self, id: &str) -> Option<AccountToken> {
        let accounts = self.accounts.read().await;
        accounts.iter().find(|a| a.id == id).cloned()
    }

    /// 给前端用的视图：不暴露 token 与 auth_blob，只给 can_activate 标记
    pub async fn list_views(&self) -> Vec<AccountView> {
        self.list().await.into_iter().map(AccountView::from).collect()
    }

    pub async fn list(&self) -> Vec<AccountToken> {
        let accounts = self.accounts.read().await;
        let idx = *self.current.read().await;
        accounts.iter().enumerate().map(|(i, a)| {
            let mut entry = a.clone();
            entry.status = if i == idx { "current".to_string() } else { "available".to_string() };
            entry
        }).collect()
    }

    pub async fn save(&self) -> Result<(), String> {
        let accounts = self.accounts.read().await;
        let data = serde_json::to_string_pretty(&*accounts)
            .map_err(|e| format!("序列化账号失败: {}", e))?;
        let encrypted = Self::encrypt_data(data.as_bytes())?;
        std::fs::write(&self.storage_path, encrypted)
            .map_err(|e| format!("写入账号文件失败: {}", e))?;
        Ok(())
    }

    pub async fn load(&self) -> Result<(), String> {
        match std::fs::read(&self.storage_path) {
            Ok(raw) => {
                // 尝试解密；如果失败则尝试明文 JSON（向后兼容旧格式）
                let json_str = match Self::decrypt_data(&raw) {
                    Ok(decrypted) => String::from_utf8(decrypted)
                        .map_err(|e| format!("解密数据非有效 UTF-8: {}", e))?,
                    Err(_) => {
                        // 可能是旧版明文 JSON 格式，尝试直接解析
                        match String::from_utf8(raw.clone()) {
                            Ok(s) => {
                                log::info!("检测到旧版明文存储，将自动迁移为加密格式");
                                s
                            }
                            Err(e) => return Err(format!("读取账号数据失败: {}", e)),
                        }
                    }
                };
                let accounts: Vec<AccountToken> = serde_json::from_str(&json_str)
                    .map_err(|e| format!("解析账号文件失败: {}", e))?;
                *self.accounts.write().await = accounts;
                // 如果是旧格式，重新保存为加密格式
                drop(json_str);
                if Self::decrypt_data(&std::fs::read(&self.storage_path).unwrap_or_default()).is_err() {
                    if let Err(e) = self.save().await {
                        log::warn!("迁移加密格式失败: {}", e);
                    }
                }
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), // 首次运行
            Err(e) => Err(format!("读取账号文件失败: {}", e)),
        }
    }

    /// 获取基于机器信息的加密密钥
    /// 注意：保持此派生方式不变，否则已有的 proxy-tokens.json 将无法解密
    fn get_machine_key() -> [u8; 32] {
        // macOS GUI 启动时 HOSTNAME 通常未设置，回退到 default_host
        let machine_name = std::env::var("HOSTNAME")
            .unwrap_or_else(|_| "default_host".to_string());
        let user_name = std::env::var("USER")
            .unwrap_or_else(|_| "default_user".to_string());
        let machine_id = format!("{}_{}", machine_name, user_name);
        let mut hasher = Sha256::new();
        hasher.update(machine_id.as_bytes());
        hasher.finalize().into()
    }

    /// AES-256-GCM 加密：输出格式 [12 bytes nonce][ciphertext+tag]
    fn encrypt_data(plaintext: &[u8]) -> Result<Vec<u8>, String> {
        let key = Self::get_machine_key();
        let cipher = Aes256Gcm::new(GenericArray::from_slice(&key));
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher.encrypt(&nonce, plaintext)
            .map_err(|e| format!("加密失败: {}", e))?;
        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce);
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    /// AES-256-GCM 解密：输入格式 [12 bytes nonce][ciphertext+tag]
    fn decrypt_data(data: &[u8]) -> Result<Vec<u8>, String> {
        if data.len() < 12 {
            return Err("数据太短，不是有效的加密格式".to_string());
        }
        let key = Self::get_machine_key();
        let cipher = Aes256Gcm::new(GenericArray::from_slice(&key));
        let nonce = GenericArray::from_slice(&data[..12]);
        let plaintext = cipher.decrypt(nonce, &data[12..])
            .map_err(|e| format!("解密失败: {}", e))?;
        Ok(plaintext)
    }
}

/// 前端可见的账号信息（脱敏：不含 token / auth_blob）
#[derive(Clone, Debug, Serialize)]
pub struct AccountView {
    pub id: String,
    pub label: String,
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub status: String,
    pub user_type: String,
    pub expire_date: String,
    pub quota_used: Option<u64>,
    pub quota_total: Option<u64>,
    pub ide_kind: String,
    /// 是否具备一键切换所需的登录数据
    pub can_activate: bool,
    /// 是否有完整登录数据（含 refreshToken）。没有时切换会被拒绝，
    /// 因为最小数据会被 IDE AuthStore 校验清除、导致掉登录。
    pub has_full_credentials: bool,
}

impl From<AccountToken> for AccountView {
    fn from(a: AccountToken) -> Self {
        Self {
            id: a.id,
            label: a.label,
            user_id: a.user_id,
            email: a.email,
            name: a.name,
            status: a.status,
            user_type: a.user_type,
            expire_date: a.expire_date,
            quota_used: a.quota_used,
            quota_total: a.quota_total,
            ide_kind: a.ide_kind,
            can_activate: !a.auth_blob.is_empty() || !a.token.is_empty(),
            has_full_credentials: !a.auth_blob.is_empty(),
        }
    }
}
