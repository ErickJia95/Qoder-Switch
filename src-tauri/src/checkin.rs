//! 每日签到 / 领取活动奖励。
//!
//! 链路（国内版与国际版路径一致，仅域名不同，社区抓包还原）：
//!   1. GET  /sash/api/v1/me/campaigns            → 找 actionType=CLAIM_BENEFIT 且 claimStatus=CLAIMABLE 的活动
//!   2. POST /sash/api/v1/me/campaigns/{id}/claim → 领取（幂等，重复领取返回 replayed=true）
//!
//! 认证：Bearer token + cosy-clienttype: 10（桌面端标识），无需 COSY 签名。

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 签到接口候选域名（与用量接口一致：CN 优先，国际回退）
pub const CHECKIN_BASES: &[&str] = &[
    "https://openapi.qoder.com.cn",
    "https://openapi.qoder.sh",
];

// ===== Campaigns 响应结构 =====

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Campaign {
    #[serde(rename = "campaignId", default)]
    pub campaign_id: String,
    #[serde(rename = "campaignKey", default)]
    pub campaign_key: String,
    #[serde(rename = "actionType", default)]
    pub action_type: String,
    #[serde(rename = "claimStatus", default)]
    pub claim_status: String,
    #[serde(default)]
    pub benefit: Option<CampaignBenefit>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct CampaignBenefit {
    #[serde(default)]
    pub amount: i64,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct CampaignsResponse {
    #[serde(default)]
    pub campaigns: Vec<Campaign>,
    #[serde(default)]
    pub claimable: bool,
}

// ===== Claim 响应结构 =====

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ClaimResponse {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub replayed: bool,
    #[serde(default)]
    pub benefit: Option<CampaignBenefit>,
    #[serde(rename = "expiresAt", default)]
    pub expires_at: serde_json::Value,
}

// ===== 返回给前端的结构 =====

#[derive(Debug, Serialize, Clone)]
pub struct CheckinResult {
    pub account_id: String,
    pub label: String,
    /// claimed = 本次领取成功 / already = 今日已领过 / no_campaign = 无可用活动 / error = 失败
    pub status: String,
    pub amount: i64,
    pub expires_at: String,
    pub message: String,
    pub api_base: String,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("Qoder")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))
}

fn app_headers() -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("accept", "application/json".parse().unwrap());
    headers.insert("accept-language", "zh-CN".parse().unwrap());
    headers.insert("cosy-clienttype", "10".parse().unwrap());
    headers
}

fn short(body: &str) -> String {
    body.chars().take(200).collect()
}

/// 从活动列表中筛选签到活动状态
#[derive(Debug, PartialEq)]
enum CampaignState {
    /// 有 CLAIMABLE 的 CLAIM_BENEFIT 活动，返回 campaignId
    Claimable(String),
    /// 已有 CLAIMED 的 CLAIM_BENEFIT 活动（今日已领）
    AlreadyClaimed,
    /// 没有签到活动（可能缺设备指纹或活动未下发）
    NoCampaign,
}

fn analyze_campaigns(resp: &CampaignsResponse) -> CampaignState {
    let mut claimed = false;
    for c in &resp.campaigns {
        if c.action_type != "CLAIM_BENEFIT" {
            continue;
        }
        match c.claim_status.as_str() {
            "CLAIMABLE" => return CampaignState::Claimable(c.campaign_id.clone()),
            "CLAIMED" => claimed = true,
            _ => {}
        }
    }
    if claimed {
        CampaignState::AlreadyClaimed
    } else {
        CampaignState::NoCampaign
    }
}

fn format_expires(v: &serde_json::Value) -> String {
    // expiresAt 可能是毫秒时间戳（数字）或 ISO 字符串
    if let Some(ts) = v.as_i64() {
        if ts > 0 {
            let t = if ts > 10_000_000_000 { ts / 1000 } else { ts };
            if let Some(dt) = chrono::DateTime::from_timestamp(t, 0) {
                return dt.format("%Y-%m-%d").to_string();
            }
        }
        return String::new();
    }
    if let Some(s) = v.as_str() {
        return s.chars().take(10).collect();
    }
    String::new()
}

/// 在单个域名上执行一轮「查活动 → 领取」
async fn try_checkin_on_base(
    client: &reqwest::Client,
    base: &str,
    token: &str,
) -> Result<(CheckinOutcome, String), String> {
    let headers = app_headers();

    // [1/2] 查询活动列表
    let list_url = format!("{}/sash/api/v1/me/campaigns", base);
    let resp = client
        .get(&list_url)
        .header("Authorization", format!("Bearer {}", token))
        .headers(headers.clone())
        .send()
        .await
        .map_err(|e| format!("请求 {} 失败: {}", list_url, e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("读取响应失败: {}", e))?;
    if !status.is_success() {
        return Err(format!("{} 返回 {}: {}", list_url, status, short(&text)));
    }
    let campaigns: CampaignsResponse = serde_json::from_str(&text)
        .map_err(|e| format!("解析活动列表 JSON 失败: {} | body: {}", e, short(&text)))?;

    // [2/2] 按状态处理
    match analyze_campaigns(&campaigns) {
        CampaignState::AlreadyClaimed => {
            log::info!("[checkin] {} 今日已领取", base);
            Ok((CheckinOutcome::Already, base.to_string()))
        }
        CampaignState::NoCampaign => {
            log::warn!("[checkin] {} 无可用签到活动（活动数 {}）", base, campaigns.campaigns.len());
            Ok((CheckinOutcome::NoCampaign, base.to_string()))
        }
        CampaignState::Claimable(campaign_id) => {
            let claim_url = format!("{}/sash/api/v1/me/campaigns/{}/claim", base, campaign_id);
            let resp = client
                .post(&claim_url)
                .header("Authorization", format!("Bearer {}", token))
                .header("Origin", base)
                .headers(headers.clone())
                .send()
                .await
                .map_err(|e| format!("请求 {} 失败: {}", claim_url, e))?;

            let status = resp.status();
            let text = resp.text().await.map_err(|e| format!("读取响应失败: {}", e))?;
            if !status.is_success() {
                return Err(format!("{} 返回 {}: {}", claim_url, status, short(&text)));
            }
            let claim: ClaimResponse = serde_json::from_str(&text)
                .map_err(|e| format!("解析领取 JSON 失败: {} | body: {}", e, short(&text)))?;

            if claim.status == "CLAIMED" {
                let amount = claim.benefit.as_ref().map(|b| b.amount).unwrap_or(0);
                if claim.replayed {
                    log::info!("[checkin] {} 幂等返回：今日已领取", base);
                    Ok((CheckinOutcome::Already, base.to_string()))
                } else {
                    log::info!("[checkin] {} 领取成功: {} Credits", base, amount);
                    Ok((
                        CheckinOutcome::Claimed {
                            amount,
                            expires_at: format_expires(&claim.expires_at),
                        },
                        base.to_string(),
                    ))
                }
            } else {
                Err(format!(
                    "领取返回异常状态 {}: {}",
                    claim.status,
                    short(&text)
                ))
            }
        }
    }
}

/// 单账号签到结果
pub enum CheckinOutcome {
    /// 本次领取成功
    Claimed { amount: i64, expires_at: String },
    /// 今日已领取过
    Already,
    /// 无可用签到活动
    NoCampaign,
}

/// 对单个账号执行签到：CN 优先，国际回退。
pub async fn checkin(token: &str) -> Result<CheckinOutcome, String> {
    let client = client()?;
    let mut last_err = String::from("签到接口不可用");
    for base in CHECKIN_BASES.iter().copied() {
        match try_checkin_on_base(&client, base, token).await {
            Ok((outcome, _)) => return Ok(outcome),
            Err(e) => {
                log::warn!("[checkin] {} 签到失败: {}", base, e);
                last_err = e;
            }
        }
    }
    Err(last_err)
}
