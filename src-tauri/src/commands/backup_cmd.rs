use crate::error::{AppError, AppResult};
use crate::state::AppState;
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

/// 导出文件里单个账号的结构。绝不包含旧代理时代的 machine_* 字段。
#[derive(Serialize)]
pub struct ExportedAccount {
    pub id: String,
    pub label: String,
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub user_type: String,
    pub expire_date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_used: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_total: Option<u64>,
    pub is_current: bool,
    /// 仅在「包含凭证」模式下写入
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// 完整登录数据（auth.v1.dat 明文），同样只在含凭证时写入
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_blob: Option<String>,
    #[serde(default)]
    pub ide_kind: String,
}

#[derive(Serialize)]
pub struct ExportPayload {
    pub schema: &'static str,
    pub schema_version: u32,
    pub app: &'static str,
    pub app_version: &'static str,
    pub exported_at: String,
    pub account_count: usize,
    pub includes_tokens: bool,
    pub accounts: Vec<ExportedAccount>,
}

#[derive(Serialize)]
pub struct ExportResult {
    pub saved: bool,
    pub path: Option<String>,
    pub count: usize,
    pub bytes: usize,
    pub includes_tokens: bool,
}

/// 组装导出内容（纯函数，便于单测）
fn build_payload(accounts: Vec<crate::auth::token::AccountToken>, include_tokens: bool) -> ExportPayload {
    let now = chrono::Local::now();
    ExportPayload {
        schema: "qoder-switch/accounts",
        schema_version: 1,
        app: "Qoder Switch",
        app_version: env!("CARGO_PKG_VERSION"),
        exported_at: now.to_rfc3339(),
        account_count: accounts.len(),
        includes_tokens: include_tokens,
        accounts: accounts
            .into_iter()
            .map(|a| ExportedAccount {
                id: a.id,
                label: a.label,
                user_id: a.user_id,
                email: a.email,
                name: a.name,
                user_type: a.user_type,
                expire_date: a.expire_date,
                quota_used: a.quota_used,
                quota_total: a.quota_total,
                is_current: a.status == "current",
                token: if include_tokens { Some(a.token) } else { None },
                auth_blob: if include_tokens && !a.auth_blob.is_empty() {
                    Some(a.auth_blob)
                } else {
                    None
                },
                ide_kind: a.ide_kind,
            })
            .collect(),
    }
}

/// 导出对话框的默认目录：桌面 → 下载 → 用户目录
fn default_export_dir() -> Option<std::path::PathBuf> {
    let home = dirs::home_dir()?;
    for sub in ["Desktop", "Downloads"] {
        let p = home.join(sub);
        if p.is_dir() {
            return Some(p);
        }
    }
    Some(home)
}

/// 导出所有账号信息为 JSON 备份文件（直接落到 桌面/下载 目录）。
/// 之前用 NSSavePanel「另存为」时，部分 macOS 环境下面板不显示且回调不触发，
/// rx.await 永久挂起导致「导出中…」卡死；改为固定目录 + 时间戳文件名彻底绕开原生对话框，
/// 失败原因通过 [export] 日志与 toast 上报。
#[tauri::command]
pub async fn export_accounts(
    state: State<'_, AppState>,
    include_tokens: bool,
) -> AppResult<ExportResult> {
    let accounts = state.token_manager.list().await;
    log::info!(
        "[export] 收到导出请求: {} 个账号，含凭证: {}",
        accounts.len(),
        include_tokens
    );

    if accounts.is_empty() {
        log::warn!("[export] 账号库为空，取消导出");
        return Err(AppError::Config("账号库为空，暂无可导出的内容".to_string()));
    }

    let now = chrono::Local::now();
    let file_name = format!("qoder-switch-accounts-{}.json", now.format("%Y%m%d-%H%M%S"));
    let dir = default_export_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let path = dir.join(&file_name);

    log::info!("[export] 保存路径: {}", path.display());

    match write_payload_to(&path, accounts, include_tokens) {
        Ok(result) => Ok(result),
        Err(e) => {
            // 失败原因必须留日志，同时透传给前端 toast
            log::error!("[export] 导出失败: {}", e);
            Err(e)
        }
    }
}

/// 把账号列表序列化成 JSON 并写入指定路径（对话框之外的实际落盘逻辑）
fn write_payload_to(
    path: &std::path::Path,
    accounts: Vec<crate::auth::token::AccountToken>,
    include_tokens: bool,
) -> AppResult<ExportResult> {
    let payload = build_payload(accounts, include_tokens);
    let text = serde_json::to_string_pretty(&payload)
        .map_err(|e| AppError::Config(format!("生成 JSON 失败: {}", e)))?;

    // 若目标目录不存在则尝试创建（用户手输了多级路径的情况）
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Config(format!("创建目录失败: {}", e)))?;
        }
    }

    std::fs::write(path, text.as_bytes())
        .map_err(|e| AppError::Config(format!("写入文件失败: {}", e)))?;

    log::info!(
        "[export] 已导出 {} 个账号到 {}（含凭证: {}，{} 字节）",
        payload.account_count,
        path.display(),
        include_tokens,
        text.len()
    );

    Ok(ExportResult {
        saved: true,
        path: Some(path.to_string_lossy().to_string()),
        count: payload.account_count,
        bytes: text.len(),
        includes_tokens: include_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::token::AccountToken;

    fn sample(status: &str) -> AccountToken {
        AccountToken {
            id: "tok_test".into(),
            label: "本地 · Tester".into(),
            token: "secret-token-value".into(),
            user_id: "u_1".into(),
            email: "tester@example.com".into(),
            name: "Tester".into(),
            status: status.into(),
            user_type: "Pro".into(),
            expire_date: "2026-12-11".into(),
            quota_used: Some(447),
            quota_total: Some(2000),
            auth_blob: String::new(),
            ide_kind: "cn".into(),
            machine_token: String::new(),
            machine_id: String::new(),
            machine_code: String::new(),
            machine_type: String::new(),
        }
    }

    #[test]
    fn includes_tokens_when_requested() {
        let payload = build_payload(vec![sample("current")], true);
        assert_eq!(payload.account_count, 1);
        let text = serde_json::to_string_pretty(&payload).unwrap();
        assert!(text.contains("\"token\": \"secret-token-value\""));
        assert!(text.contains("\"is_current\": true"));
        assert!(text.contains("\"schema\": \"qoder-switch/accounts\""));
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["accounts"][0]["email"], "tester@example.com");
        assert_eq!(v["accounts"][0]["quota_total"], 2000);
    }

    #[test]
    fn omits_tokens_when_disabled() {
        let payload = build_payload(vec![sample("available")], false);
        let text = serde_json::to_string_pretty(&payload).unwrap();
        assert!(!text.contains("secret-token-value"));
        assert!(!text.contains("\"token\""));
        assert!(text.contains("\"includes_tokens\": false"));
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["account_count"], 1);
    }

    /// 真实落盘：写到临时目录再读回来校验
    #[test]
    fn writes_valid_json_file() {
        let dir = std::env::temp_dir().join(format!("qsw-export-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("backup.json");

        let res = write_payload_to(&path, vec![sample("current"), sample("available")], true).unwrap();
        assert!(res.saved);
        assert_eq!(res.count, 2);
        assert_eq!(res.path.as_deref(), Some(path.to_string_lossy().as_ref()));

        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.len(), res.bytes);
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["account_count"], 2);
        assert_eq!(v["accounts"].as_array().unwrap().len(), 2);
        assert_eq!(v["accounts"][0]["is_current"], true);
        assert_eq!(v["accounts"][0]["token"], "secret-token-value");
        println!("--- exported file ---\n{}\n--- end ---", text);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parses_wrapper_and_bare_array() {
        let wrapped = r#"{"schema":"qoder-switch/accounts","accounts":[
            {"id":"tok_1","label":"A","user_id":"u1","email":"a@x.com","is_current":true,"token":"t1","quota_total":2000},
            {"label":"B","userId":"u2","email":"b@x.com","quotaTotal":100.0}
        ]}"#;
        let list = parse_backup(wrapped).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].label, "A");
        assert!(list[0].is_current);
        assert_eq!(list[1].user_id, "u2");          // camelCase 别名
        assert_eq!(list[1].quota_total, Some(100.0));
        assert!(list[1].token.is_empty());

        let bare = r#"[{"label":"C","email":"c@x.com","token":"t3"}]"#;
        assert_eq!(parse_backup(bare).unwrap().len(), 1);

        assert!(parse_backup(r#"{"foo":1}"#).is_err());
        assert!(parse_backup("not json").is_err());
    }

    #[tokio::test]
    async fn merge_adds_updates_and_skips() {
        let dir = std::env::temp_dir().join(format!("qsw-merge-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tm = crate::auth::token::TokenManager::new(dir.join("tokens.json"));

        // 已存在一个账号
        tm.add("old-token".into(), "旧账号".into(), "u1".into(), "a@x.com".into(), "旧".into(), "Pro".into(), "2026-12-01".into())
            .await;

        let incoming = vec![
            // 同 user_id → 更新凭证与信息
            RawAccount {
                id: "tok_old".into(),
                label: "新标签".into(),
                user_id: "u1".into(),
                email: "a@x.com".into(),
                name: "新".into(),
                user_type: "Pro+".into(),
                expire_date: "2027-01-01".into(),
                quota_used: Some(10.0),
                quota_total: Some(3000.0),
                is_current: true,
                token: "new-token".into(),
                auth_blob: r#"{"token":"new-token"}"#.into(),
                ide_kind: "cn".into(),
            },
            // 全新账号（有 token）→ 新增
            RawAccount {
                label: "全新".into(),
                user_id: "u2".into(),
                email: "b@x.com".into(),
                token: "t2".into(),
                ..Default::default()
            },
            // 全新账号但没有 token → 跳过
            RawAccount {
                label: "无凭证".into(),
                user_id: "u3".into(),
                email: "c@x.com".into(),
                ..Default::default()
            },
        ];
        let (added, updated, skipped) = tm.merge_accounts(incoming.into_iter().map(RawAccount::into_token).collect()).await;
        assert_eq!((added, updated, skipped), (1, 1, 1));

        let accounts = tm.list().await;
        assert_eq!(accounts.len(), 2);
        let a = accounts.iter().find(|x| x.user_id == "u1").unwrap();
        assert_eq!(a.token, "new-token");
        assert_eq!(a.label, "新标签");
        assert_eq!(a.user_type, "Pro+");
        assert_eq!(a.quota_total, Some(3000));
        assert!(tm.find_id("u2", "").await.is_some());
        assert!(tm.find_id("", "b@x.com").await.is_some());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 端到端：导出 → 换一个空账号库导入 → 重启后仍能解密读回
    #[tokio::test]
    async fn roundtrip_export_then_import() {
        let dir = std::env::temp_dir().join(format!("qsw-roundtrip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let src = crate::auth::token::TokenManager::new(dir.join("src.json"));
        src.add("secret-token".into(), "甲".into(), "u1".into(), "a@x.com".into(), "甲".into(), "Pro".into(), "2026-12-11".into())
            .await;
        src.update_account_quota(&src.list().await[0].id.clone(), Some(447), Some(2000)).await;

        let backup = dir.join("backup.json");
        write_payload_to(&backup, src.list().await, true).unwrap();

        // 模拟换机：全新空库
        let dst = crate::auth::token::TokenManager::new(dir.join("dst.json"));
        let raw = parse_backup(&std::fs::read_to_string(&backup).unwrap()).unwrap();
        let stats = dst
            .merge_accounts(raw.into_iter().map(RawAccount::into_token).collect())
            .await;
        assert_eq!(stats, (1, 0, 0));

        // 重新从磁盘加载（验证 AES 加密落盘可用）
        let reloaded = crate::auth::token::TokenManager::new(dir.join("dst.json"));
        reloaded.load().await.unwrap();
        let accounts = reloaded.list().await;
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].token, "secret-token");
        assert_eq!(accounts[0].email, "a@x.com");
        assert_eq!(accounts[0].quota_total, Some(2000));

        std::fs::remove_dir_all(&dir).ok();
    }
}

// ---------------------------------------------------------------------------
// 导入 / 恢复
// ---------------------------------------------------------------------------

/// 备份文件里的账号条目，字段全部宽松匹配（缺字段不报错，兼容手改过的文件）
#[derive(serde::Deserialize, Default)]
struct RawAccount {
    #[serde(default)]
    id: String,
    #[serde(default)]
    label: String,
    #[serde(default, alias = "userId")]
    user_id: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    name: String,
    #[serde(default, alias = "userType", alias = "plan")]
    user_type: String,
    #[serde(default, alias = "expireDate", alias = "expires_at")]
    expire_date: String,
    #[serde(default, alias = "quotaUsed")]
    quota_used: Option<f64>,
    #[serde(default, alias = "quotaTotal")]
    quota_total: Option<f64>,
    #[serde(default, alias = "isCurrent", alias = "current")]
    is_current: bool,
    #[serde(default)]
    token: String,
    #[serde(default, alias = "authBlob", alias = "auth_json")]
    auth_blob: String,
    #[serde(default, alias = "ideKind")]
    ide_kind: String,
}

#[derive(Serialize)]
pub struct ImportResult {
    pub imported: bool,
    pub path: Option<String>,
    pub added: usize,
    pub updated: usize,
    pub skipped: usize,
    pub total: usize,
    /// 恢复的「当前账号」标签（备份里带 is_current 且用户选择恢复时）
    pub restored_current: Option<String>,
}

impl RawAccount {
    fn into_token(self) -> crate::auth::token::AccountToken {
        crate::auth::token::AccountToken {
            id: self.id,
            label: self.label,
            token: self.token,
            user_id: self.user_id,
            email: self.email,
            name: self.name,
            status: "available".to_string(),
            user_type: self.user_type,
            expire_date: self.expire_date,
            quota_used: self.quota_used.map(|v| v.round().max(0.0) as u64),
            quota_total: self.quota_total.map(|v| v.round().max(0.0) as u64),
            auth_blob: self.auth_blob,
            ide_kind: self.ide_kind,
            machine_token: String::new(),
            machine_id: String::new(),
            machine_code: String::new(),
            machine_type: String::new(),
        }
    }
}

/// 解析备份文件内容：既支持本应用导出的 `{ accounts: [...] }`，也支持纯数组
fn parse_backup(text: &str) -> AppResult<Vec<RawAccount>> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| AppError::Config(format!("不是有效的 JSON: {}", e)))?;

    let list = if value.is_array() {
        value
    } else {
        value
            .get("accounts")
            .cloned()
            .ok_or_else(|| AppError::Config("备份文件缺少 accounts 数组".to_string()))?
    };

    let items = list
        .as_array()
        .ok_or_else(|| AppError::Config("accounts 字段不是数组".to_string()))?;

    let mut parsed = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let acc: RawAccount = serde_json::from_value(item.clone())
            .map_err(|e| AppError::Config(format!("第 {} 个账号格式错误: {}", i + 1, e)))?;
        parsed.push(acc);
    }
    Ok(parsed)
}

/// 选择备份文件并合并进本地账号库（按 user_id 去重，不删除现有账号）
#[tauri::command]
pub async fn import_accounts(
    app: AppHandle,
    state: State<'_, AppState>,
    restore_current: bool,
) -> AppResult<ImportResult> {
    let mut dialog = app
        .dialog()
        .file()
        .set_title("选择账号备份（JSON）")
        .add_filter("JSON 文件", &["json"]);

    if let Some(dir) = default_export_dir() {
        dialog = dialog.set_directory(dir);
    }

    // 同样不能用 blocking_pick_file（async 命令里会死锁），改回调式 API
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<tauri_plugin_dialog::FilePath>>();
    dialog.pick_file(move |picked| {
        let _ = tx.send(picked);
    });

    // 用户取消
    let Ok(Some(picked)) = rx.await else {
        return Ok(ImportResult {
            imported: false,
            path: None,
            added: 0,
            updated: 0,
            skipped: 0,
            total: 0,
            restored_current: None,
        });
    };

    let path = picked
        .into_path()
        .map_err(|e| AppError::Config(format!("无效的文件路径: {}", e)))?;

    let text = std::fs::read_to_string(&path)
        .map_err(|e| AppError::Config(format!("读取文件失败: {}", e)))?;

    let raw = parse_backup(&text)?;
    if raw.is_empty() {
        return Err(AppError::Config("备份文件里没有账号".to_string()));
    }

    // 记下备份里标记为「当前」的账号，合并完成后尝试恢复该标记
    let preferred_current = raw
        .iter()
        .find(|a| a.is_current)
        .map(|a| (a.user_id.clone(), a.email.clone()));
    let total = raw.len();

    let (added, updated, skipped) = state
        .token_manager
        .merge_accounts(raw.into_iter().map(RawAccount::into_token).collect())
        .await;

    let mut restored_current = None;
    if restore_current {
        if let Some((user_id, email)) = preferred_current {
            if let Some(id) = state.token_manager.find_id(&user_id, &email).await {
                if state.token_manager.set_current(&id).await {
                    restored_current = state
                        .token_manager
                        .list()
                        .await
                        .into_iter()
                        .find(|a| a.id == id)
                        .map(|a| a.label);
                }
            }
        }
    }

    log::info!(
        "[import] {} → 新增 {}，更新 {}，跳过 {}，恢复当前标记: {:?}",
        path.display(),
        added,
        updated,
        skipped,
        restored_current
    );

    Ok(ImportResult {
        imported: true,
        path: Some(path.to_string_lossy().to_string()),
        added,
        updated,
        skipped,
        total,
        restored_current,
    })
}
