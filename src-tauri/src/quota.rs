//! 用量 / 订阅查询。
//!
//! Qoder 的账号用量在 https://qoder.cn/account/usage 可以看到，网页背后调用的就是
//! `openapi.qoder.com.cn` 上的 v2 接口。国际版账号则走 `openapi.qoder.sh`。
//! 这里按「首选域名 → 备用域名」的顺序尝试，任一成功即可。

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 用量接口候选域名（按优先级排序）
pub const API_BASES: &[&str] = &[
    "https://openapi.qoder.com.cn",
    "https://openapi.qoder.sh",
];

pub const USAGE_PAGE_URL: &str = "https://qoder.cn/account/usage";

// ===== Plan API 响应结构 =====

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct UserPlan {
    #[serde(default)]
    pub user_type: String,
    #[serde(default)]
    pub plan_tier_name: String,
    #[serde(default)]
    pub is_personal_version: bool,
    #[serde(default)]
    pub is_highest_tier: bool,
    #[serde(default)]
    pub start_date: i64,
    #[serde(default)]
    pub end_date: i64,
}

// ===== Quota/Usage API 响应结构 =====

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct QuotaUsage {
    #[serde(rename = "userId", default)]
    pub user_id: String,
    #[serde(rename = "userType", default)]
    pub user_type: String,
    #[serde(rename = "usageType", default)]
    pub usage_type: String,
    #[serde(rename = "totalUsagePercentage", default)]
    pub total_usage_percentage: f64,
    #[serde(rename = "isQuotaExceeded", default)]
    pub is_quota_exceeded: bool,
    #[serde(rename = "expiresAt", default)]
    pub expires_at: i64,
    #[serde(rename = "userQuota", default)]
    pub user_quota: UserQuota,
    #[serde(rename = "addOnQuota", default)]
    pub add_on_quota: Option<AddOnQuota>,
    #[serde(rename = "isPlanQuotaProrated", default)]
    pub is_plan_quota_prorated: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct UserQuota {
    #[serde(default)]
    pub total: f64,
    #[serde(default)]
    pub used: f64,
    #[serde(default)]
    pub remaining: f64,
    #[serde(default)]
    pub percentage: f64,
    #[serde(default)]
    pub unit: String,
}

/// 个人资源包（购买的一次性额度包），接口字段 addOnQuota；未购买时 total 为 0 或字段缺失
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AddOnQuota {
    #[serde(default)]
    pub total: f64,
    #[serde(default)]
    pub used: f64,
    #[serde(default)]
    pub remaining: f64,
    #[serde(default)]
    pub percentage: f64,
    #[serde(default)]
    pub unit: String,
    #[serde(rename = "detailUrl", default)]
    pub detail_url: String,
}

impl AddOnQuota {
    /// 资源包是否存在有效额度（total>0 视为已购买）
    pub fn has_quota(&self) -> bool {
        self.total > 0.0
    }
}

// ===== 返回给前端的结构 =====

#[derive(Debug, Serialize, Clone)]
pub struct QuotaResult {
    pub account_id: String,
    pub label: String,
    pub plan_name: String,
    pub user_type: String,
    pub quota_used: f64,
    pub quota_total: f64,
    pub quota_remaining: f64,
    pub quota_unit: String,
    pub usage_percentage: f64,
    pub is_exceeded: bool,
    pub expire_date: String,
    /// 个人资源包（未购买或接口未返回时为 None）
    pub add_on_quota: Option<AddOnQuota>,
    pub api_base: String,
    pub error: Option<String>,
}

/// 归一化后的用量数据
#[derive(Debug, Clone)]
pub struct UsageSnapshot {
    pub plan_name: String,
    pub user_type: String,
    pub used: f64,
    pub total: f64,
    pub remaining: f64,
    pub unit: String,
    pub percentage: f64,
    pub is_exceeded: bool,
    pub expire_date: String,
    /// 个人资源包（接口未返回或未购买时为 None）
    pub add_on: Option<AddOnQuota>,
    pub api_base: String,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("QoderSwitch/2.0 (macOS)")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))
}

fn short(body: &str) -> String {
    body.chars().take(200).collect()
}

/// 调用 Plan 接口（失败不致命，仅缺少套餐信息）
async fn fetch_plan(client: &reqwest::Client, base: &str, token: &str) -> Option<UserPlan> {
    let url = format!("{}/api/v2/user/plan", base);
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/json")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        log::warn!("[quota] Plan 接口 {} 返回 {}", url, resp.status());
        return None;
    }
    let text = resp.text().await.ok()?;
    match serde_json::from_str::<UserPlan>(&text) {
        Ok(p) => Some(p),
        Err(e) => {
            log::warn!("[quota] 解析 Plan 失败: {} | {}", e, short(&text));
            None
        }
    }
}

/// 调用 Quota/Usage 接口（核心数据）
async fn fetch_quota(client: &reqwest::Client, base: &str, token: &str) -> Result<QuotaUsage, String> {
    let url = format!("{}/api/v2/quota/usage", base);
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("请求 {} 失败: {}", url, e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("读取响应失败: {}", e))?;

    if !status.is_success() {
        return Err(format!("{} 返回 {}: {}", url, status, short(&text)));
    }

    serde_json::from_str::<QuotaUsage>(&text)
        .map_err(|e| format!("解析用量 JSON 失败: {} | body: {}", e, short(&text)))
}

fn format_date(ts_ms: i64) -> String {
    if ts_ms <= 0 {
        return String::new();
    }
    let ts = if ts_ms > 10_000_000_000 { ts_ms / 1000 } else { ts_ms };
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// 查询某账号用量：优先 CN 域名，失败后自动切换国际域名。
pub async fn fetch_usage(token: &str) -> Result<UsageSnapshot, String> {
    let client = client()?;

    let mut last_err = String::from("用量接口不可用");
    for base in API_BASES.iter().copied() {
        match fetch_quota(&client, base, token).await {
            Ok(usage) => {
                let plan = fetch_plan(&client, base, token).await.unwrap_or_default();
                let expire_date = if usage.expires_at > 0 {
                    format_date(usage.expires_at)
                } else {
                    format_date(plan.end_date)
                };
                let user_type = if !usage.user_type.is_empty() {
                    usage.user_type.clone()
                } else {
                    plan.user_type.clone()
                };
                // totalUsagePercentage 是 0~1 的比例，统一换算成百分数
                let percentage = if usage.user_quota.total > 0.0 {
                    (usage.user_quota.used / usage.user_quota.total * 100.0).min(100.0)
                } else if usage.total_usage_percentage > 0.0 {
                    let p = usage.total_usage_percentage;
                    if p <= 1.0 { p * 100.0 } else { p }
                } else if usage.user_quota.percentage > 0.0 {
                    let p = usage.user_quota.percentage;
                    if p <= 1.0 { p * 100.0 } else { p }
                } else {
                    0.0
                };
                let remaining = if usage.user_quota.total > 0.0 {
                    usage.user_quota.remaining
                } else {
                    (usage.user_quota.total - usage.user_quota.used).max(0.0)
                };
                log::info!(
                    "[quota] {} 查询成功: 订阅 {}/{} {}，资源包 {:?}",
                    base,
                    usage.user_quota.used,
                    usage.user_quota.total,
                    usage.user_quota.unit,
                    usage.add_on_quota.as_ref().map(|a| (a.used, a.total)),
                );
                return Ok(UsageSnapshot {
                    plan_name: if plan.plan_tier_name.is_empty() {
                        user_type.clone()
                    } else {
                        plan.plan_tier_name
                    },
                    user_type,
                    used: usage.user_quota.used,
                    total: usage.user_quota.total,
                    remaining,
                    unit: usage.user_quota.unit,
                    percentage,
                    is_exceeded: usage.is_quota_exceeded,
                    expire_date,
                    add_on: usage
                        .add_on_quota
                        .filter(|a| a.has_quota()),
                    api_base: base.to_string(),
                });
            }
            Err(e) => {
                log::warn!("[quota] {} 查询失败: {}", base, e);
                last_err = e;
            }
        }
    }

    Err(last_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// addOnQuota 与 userQuota 同时存在时都能解析（真实接口返回形态）
    #[test]
    fn parses_add_on_quota() {
        let body = r#"{
            "userId": "u1", "userType": "personal_professional", "usageType": "credits",
            "totalUsagePercentage": 0.35, "isQuotaExceeded": false,
            "expiresAt": 1797004800000,
            "userQuota": {"total": 2000.0, "used": 447.0, "remaining": 1553.0, "percentage": 0.22, "unit": "credits"},
            "addOnQuota": {"total": 1000.0, "used": 600.0, "remaining": 400.0, "percentage": 0.6, "unit": "credits",
                           "detailUrl": "https://qoder.com.cn/account/usage"},
            "isPlanQuotaProrated": false
        }"#;
        let usage: QuotaUsage = serde_json::from_str(body).unwrap();
        let addon = usage.add_on_quota.expect("addOnQuota missing");
        assert!(addon.has_quota());
        assert_eq!(addon.total, 1000.0);
        assert_eq!(addon.used, 600.0);
        assert_eq!(addon.remaining, 400.0);
        assert_eq!(addon.unit, "credits");
        assert_eq!(usage.user_quota.total, 2000.0);
    }

    /// 未购买资源包：字段缺失不报错
    #[test]
    fn handles_missing_add_on_quota() {
        let body = r#"{
            "userId": "u2", "userType": "personal_free", "usageType": "credits",
            "userQuota": {"total": 100.0, "used": 50.0, "remaining": 50.0, "unit": "credits"},
            "isPlanQuotaProrated": false
        }"#;
        let usage: QuotaUsage = serde_json::from_str(body).unwrap();
        assert!(usage.add_on_quota.is_none());
    }

    /// 资源包 total=0 视为未购买（filter 掉）
    #[test]
    fn zero_total_add_on_treated_as_absent() {
        let body = r#"{
            "userQuota": {"total": 100.0, "used": 10.0, "unit": "credits"},
            "addOnQuota": {"total": 0.0, "used": 0.0, "remaining": 0.0, "unit": "credits"}
        }"#;
        let usage: QuotaUsage = serde_json::from_str(body).unwrap();
        assert!(!usage.add_on_quota.as_ref().unwrap().has_quota());
        // fetch_usage 中 filter 后应丢弃 → 模拟同样的过滤逻辑
        let filtered = usage.add_on_quota.filter(|a| a.has_quota());
        assert!(filtered.is_none());
    }
}

