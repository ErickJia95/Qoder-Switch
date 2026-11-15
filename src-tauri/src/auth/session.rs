//! 凭证刷新的编排层：互斥、落盘顺序、写回 IDE 文件。
//!
//! 刷新成功后的落盘顺序**必须**是：
//!   1. 先把新凭证写进本地账号库（AES 加密的 proxy-tokens.json）
//!   2. 若该账号正是 IDE 当前登录的账号，再把新的 auth.v1.dat 写回 IDE
//!
//! 这样即使第 2 步失败（IDE 未退出、被占用等），本地库也已经拿到新 refreshToken，
//! 不会因为服务端轮换了旧值而导致账号永久失效。反过来先写 IDE 再写库，一旦写库
//! 失败就两边都丢了。
//!
//! 并发：同一账号的刷新必须串行。批量查用量 / 批量签到时如果两个请求同时发现
//! 凭证过期，会各自带着同一个旧 refreshToken 去刷新；服务端轮换后第二次必然失败，
//! 更糟的是可能把「已经成功那次的」新凭证覆盖掉。这里用按账号 id 的异步互斥锁。

use crate::auth::refresh::{self, RefreshedTokens};
use crate::auth::token::TokenManager;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// 提前多久刷新：凭证剩余有效期不足 3 天时主动刷新
pub const REFRESH_AHEAD_SECS: i64 = 3 * 24 * 60 * 60;

/// 按账号串行化刷新的锁集合
#[derive(Default)]
pub struct RefreshLocks {
    inner: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl RefreshLocks {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock_for(&self, account_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut map = match self.inner.lock() {
            Ok(m) => m,
            Err(poisoned) => poisoned.into_inner(),
        };
        map.entry(account_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }
}

/// 一个账号刷新后的结果（给前端看的脱敏视图：不含任何 token 内容）
#[derive(Debug, Clone, serde::Serialize)]
pub struct RefreshOutcome {
    pub account_id: String,
    pub label: String,
    /// refreshed = 本次真的换了新凭证；skipped = 还不需要刷新；failed = 刷新失败
    pub status: String,
    /// 刷新后访问凭证的精确到期时间（ISO8601）
    pub expires_at: String,
    pub expire_date: String,
    /// IDE 里的 auth.v1.dat 是否也同步写回（仅当该账号正被 IDE 使用时才写）
    pub synced_to_ide: bool,
    pub message: String,
}

/// 把 ISO8601 时间换算成距离现在的秒数；解析不出来返回 None
fn secs_until(iso: &str) -> Option<i64> {
    let dt = chrono::DateTime::parse_from_rfc3339(iso).ok()?;
    Some((dt.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_seconds())
}

/// 凭证是否已经足够旧、值得刷新
pub fn needs_refresh(expires_at: &str) -> bool {
    match secs_until(expires_at) {
        Some(secs) => secs <= REFRESH_AHEAD_SECS,
        // 解析不出到期时间：不主动刷，避免把还能用的凭证折腾掉
        None => false,
    }
}

/// 取账号时用的轻量快照（避免长时间持锁）
struct Snapshot {
    id: String,
    label: String,
    auth_blob: String,
    ide_kind: String,
}

/// 刷新单个账号。`force = true` 时忽略「还不需要刷新」的判断。
pub async fn refresh_account(
    tm: &TokenManager,
    locks: &RefreshLocks,
    account_id: &str,
    force: bool,
) -> Result<RefreshOutcome, String> {
    let lock = locks.lock_for(account_id);
    let _guard = lock.lock().await;

    // 拿到锁后再读一次：可能前一个并发调用已经把它刷好了
    let account = tm
        .get(account_id)
        .await
        .ok_or_else(|| format!("账号不存在: {}", account_id))?;

    let snapshot = Snapshot {
        id: account.id.clone(),
        label: account.label.clone(),
        auth_blob: account.auth_blob.clone(),
        ide_kind: account.ide_kind.clone(),
    };

    if snapshot.auth_blob.is_empty() {
        return Err(format!(
            "「{}」没有完整登录数据（缺 refreshToken），请先在 IDE 登录一次再点「读取当前登录账号」",
            snapshot.label
        ));
    }

    let tokens = crate::local_account::read_tokens_from_json(&snapshot.auth_blob)?;

    // 不强制刷新时，先看凭证是否真的快过期（用 authBlob 里的精确时间，而不是库里的日期）
    if !force && !needs_refresh(&tokens.expires_at) {
        log::info!(
            "[refresh] 「{}」凭证仍有效（{}），跳过刷新",
            snapshot.label,
            tokens.expires_at
        );
        return Ok(RefreshOutcome {
            account_id: snapshot.id,
            label: snapshot.label,
            status: "skipped".to_string(),
            expires_at: tokens.expires_at.clone(),
            expire_date: tokens.expires_at.chars().take(10).collect(),
            synced_to_ide: false,
            message: format!("凭证仍有效（到期 {}），无需刷新", tokens.expires_at),
        });
    }

    log::info!(
        "[refresh] 开始刷新「{}」的新凭证（原到期 {}）",
        snapshot.label,
        tokens.expires_at
    );
    let refreshed = refresh::refresh(&tokens.refresh_token, &snapshot.ide_kind).await?;
    warn_if_rotated(&tokens.refresh_token, &refreshed);

    apply_refreshed(tm, &snapshot, &tokens, refreshed).await
}

fn warn_if_rotated(old: &str, new: &RefreshedTokens) {
    if new.refresh_token != old {
        log::info!("[refresh] 服务端已轮换 refreshToken，按新值落盘");
    }
}

/// 落盘：先本地库，再（必要时）IDE 文件
async fn apply_refreshed(
    tm: &TokenManager,
    snapshot: &Snapshot,
    old: &crate::local_account::AuthTokens,
    refreshed: RefreshedTokens,
) -> Result<RefreshOutcome, String> {
    // refreshTokenExpiresAt 接口没给就沿用旧值，避免把有效期写空
    let rt_expires = if refreshed.refresh_token_expires_at.is_empty() {
        old.refresh_token_expires_at.clone()
    } else {
        refreshed.refresh_token_expires_at.clone()
    };

    let patched = crate::local_account::patch_auth_json(
        &snapshot.auth_blob,
        &refreshed.token,
        &refreshed.refresh_token,
        &refreshed.expires_at,
        &rt_expires,
    )?;

    let expire_date: String = refreshed.expires_at.chars().take(10).collect();

    // 第 1 步：本地库先落盘（新 refreshToken 一旦到这里就不会丢）
    // expire_date 传空：库里这一列是「套餐到期日」，由用量接口维护；
    // 凭证自身的到期时间在 auth_blob 里，AccountView.expires_at 会读它，两者不要互相覆盖。
    tm.refresh_credentials(
        &snapshot.id,
        refreshed.token.clone(),
        String::new(),
        patched.clone(),
        snapshot.ide_kind.clone(),
    )
    .await;

    // 第 2 步：只有当这个账号正是 IDE 当前登录的账号时，才回写 auth.v1.dat
    let mut synced_to_ide = false;
    let mut ide_note = String::new();

    // ide_kind 为空说明无法确定属于哪个版本，不去猜，避免写错 IDE 文件
    if snapshot.ide_kind.is_empty() {
        log::info!("[refresh] 该账号未标记 IDE 版本，跳过 auth.v1.dat 回写");
    } else {
        sync_to_ide(tm, snapshot, &patched, &mut synced_to_ide, &mut ide_note).await;
    }

    log::info!(
        "[refresh] 「{}」刷新完成，新到期 {}（本地库已落盘，IDE 同步: {}）",
        snapshot.label,
        expire_date,
        synced_to_ide
    );

    Ok(RefreshOutcome {
        account_id: snapshot.id.clone(),
        label: snapshot.label.clone(),
        status: "refreshed".to_string(),
        expires_at: refreshed.expires_at.clone(),
        expire_date: expire_date.clone(),
        synced_to_ide,
        message: format!("凭证已刷新（到期 {}）{}", expire_date, ide_note),
    })
}

/// 回写 IDE 的 auth.v1.dat：仅当该账号正是这个版本 IDE 当前登录的账号时才写。
/// IDE 运行中不写 —— 它退出时会用内存里的状态覆盖磁盘，我们写了也是白写。
async fn sync_to_ide(
    tm: &TokenManager,
    snapshot: &Snapshot,
    patched: &str,
    synced: &mut bool,
    note: &mut String,
) {
    use crate::process::IdeKind;

    let kind = snapshot.ide_kind.as_str();
    let Some(ide) = IdeKind::parse(kind) else {
        log::warn!("[refresh] 未知的 IDE 版本 {}，跳过回写", kind);
        return;
    };

    let account_user = tm
        .get(&snapshot.id)
        .await
        .map(|a| a.user_id)
        .unwrap_or_default();
    if account_user.is_empty() {
        return;
    }

    // 只有「这个账号就是 IDE 里当前登录的账号」才值得写
    let ide_user = crate::local_account::current_user_id(kind);
    if ide_user.as_deref() != Some(account_user.as_str()) {
        log::info!(
            "[refresh] 该账号不是 {} 当前登录账号，无需回写 auth.v1.dat",
            ide.app_name()
        );
        return;
    }

    if crate::process::is_running(ide) {
        log::warn!(
            "[refresh] {} 正在运行，暂不回写 auth.v1.dat（退出时会覆盖）",
            ide.app_name()
        );
        *note = format!("（{} 正在运行，未回写 IDE 文件，重启后生效）", ide.app_name());
        return;
    }

    match crate::local_account::write_auth(kind, patched) {
        Ok(()) => {
            *synced = true;
            log::info!("[refresh] 已把新凭证回写 auth.v1.dat（{}）", kind);
        }
        Err(e) => {
            // 不影响「刷新成功」：本地库已经拿到新凭证
            log::warn!("[refresh] 回写 auth.v1.dat 失败: {}", e);
            *note = format!("（IDE 文件回写失败: {}）", e);
        }
    }
}

/// 批量刷新：逐个来，单个失败不影响其它账号（避免并发轮换）
pub async fn refresh_all(
    tm: &TokenManager,
    locks: &RefreshLocks,
    force: bool,
) -> Vec<RefreshOutcome> {
    let accounts = tm.list().await;
    let mut results = Vec::new();

    for account in &accounts {
        match refresh_account(tm, locks, &account.id, force).await {
            Ok(outcome) => results.push(outcome),
            Err(e) => {
                log::warn!("[refresh] 账号 {} 刷新失败: {}", account.label, e);
                results.push(RefreshOutcome {
                    account_id: account.id.clone(),
                    label: account.label.clone(),
                    status: "failed".to_string(),
                    expires_at: String::new(),
                    expire_date: account.expire_date.clone(),
                    synced_to_ide: false,
                    message: e,
                });
            }
        }
    }

    let ok = results.iter().filter(|r| r.status == "refreshed").count();
    let skipped = results.iter().filter(|r| r.status == "skipped").count();
    let failed = results.iter().filter(|r| r.status == "failed").count();
    log::info!(
        "[refresh] 批量刷新结束：成功 {}，跳过 {}，失败 {}",
        ok,
        skipped,
        failed
    );
    results
}

/// 查询/签到前的兜底：凭证快过期就先刷一下。
/// 失败**不阻断**后续操作——旧凭证在真正失效前通常仍可用，让业务请求自己报错更准。
pub async fn ensure_fresh(
    tm: &TokenManager,
    locks: &RefreshLocks,
    account_id: &str,
) -> Option<String> {
    match refresh_account(tm, locks, account_id, false).await {
        Ok(outcome) => {
            if outcome.status == "refreshed" {
                Some(outcome.message)
            } else {
                None
            }
        }
        Err(e) => {
            log::warn!("[refresh] 自动刷新失败（不影响本次操作）: {}", e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decides_when_to_refresh() {
        // 过去的时间 → 需要刷新
        assert!(needs_refresh("2020-01-01T00:00:00Z"));
        // 一年后 → 不需要
        assert!(!needs_refresh("2030-01-01T00:00:00Z"));
        // 解析不出 → 不主动刷，避免误伤
        assert!(!needs_refresh(""));
        assert!(!needs_refresh("2026-13-45"));
    }

    #[test]
    fn ahead_window_is_three_days() {
        assert_eq!(REFRESH_AHEAD_SECS, 259200);
    }
}
