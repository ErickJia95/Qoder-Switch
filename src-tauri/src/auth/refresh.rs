//! 凭证刷新（deviceToken refresh）。
//!
//! Qoder IDE 的登录态里有两种凭证：
//!   - `token`         `dt-...`  访问凭证，有效期约 30 天
//!   - `refreshToken`  `drt-...`  刷新凭证，有效期约 1 年
//!
//! 访问凭证过期后，IDE 会用 refreshToken 换一个新的；本工具把多个账号存在本地库，
//! 但 IDE 一次只登录一个账号，其余账号的凭证过期后没人替它们刷新，只能靠人工重新
//! 登录 IDE 补齐。这里补上「自己刷新」的能力。
//!
//! 接口（从 Qoder CN 客户端 app.asar 反混淆得到，并实测确认）：
//!   POST {base}/api/v1/deviceToken/refresh
//!   Content-Type: application/json
//!   body: {"refresh_token":"drt-..."}
//!   响应: token / refreshToken / expiresAt(±expiresIn) / refreshTokenExpiresAt(±refreshTokenExpiresIn)
//!
//! 实测到的校验行为（错误码可直接用于分类提示）：
//!   - body 缺字段      → 400 DeviceRefreshTokenRequired
//!   - 前缀不是 drt-    → 400 DeviceRefreshTokenPrefixInvalid
//!   - 凭证无效/失效    → 401 Unauthorized
//!
//! ⚠️ 刷新成功后 refreshToken 很可能被服务端**轮换**，旧值立即失效。
//! 所以调用方必须在拿到结果后立刻原子落盘（IDE 的 auth.v1.dat + 本地账号库），
//! 否则会两边同时掉登录。写回逻辑见 `auth::session::apply_refreshed`。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

/// 刷新接口候选域名（与用量/签到一致：CN 优先，国际回退）
pub const REFRESH_BASES: &[&str] = &["https://openapi.qoder.com.cn", "https://openapi.qoder.sh"];

/// 按账号所属的 IDE 版本排定域名顺序。
/// 这一点很关键：国内版账号的凭证只认 CN 域名，国际版只认国际域名，
/// 用错域名会拿到 401（而非 404），看起来像「凭证失效」。所以优先域名必须跟着账号走。
pub fn bases_for_kind(ide_kind: &str) -> Vec<&'static str> {
    match ide_kind {
        "intl" => vec!["https://openapi.qoder.sh", "https://openapi.qoder.com.cn"],
        _ => REFRESH_BASES.to_vec(),
    }
}

/// 刷新凭证前缀，服务端强校验
pub const REFRESH_TOKEN_PREFIX: &str = "drt-";

/// 刷新结果（字段名与 auth.v1.dat 一一对应，便于直接回写）
#[derive(Debug, Clone, Serialize)]
pub struct RefreshedTokens {
    pub token: String,
    pub refresh_token: String,
    /// ISO8601，如 2026-12-14T14:41:34Z；接口没给时为空
    pub expires_at: String,
    /// ISO8601；接口没给时为空（沿用旧值，见调用方）
    pub refresh_token_expires_at: String,
    pub api_base: String,
}

/// 刷新接口的响应。**真实响应是 snake_case**（实测）：
/// `{"device_token","refresh_token","token_type","expires_at","refresh_token_expires_at","created_at"}`
/// 同时保留 camelCase 别名，兼容客户端其它接口形态与服务端可能的调整。
#[derive(Debug, Deserialize, Default)]
struct RefreshResponse {
    #[serde(default, alias = "token")]
    device_token: Option<String>,
    #[serde(default, alias = "accessToken")]
    access_token: Option<String>,
    #[serde(default, alias = "refreshToken")]
    refresh_token: Option<String>,
    #[serde(default, alias = "expiresAt")]
    expires_at: Option<Value>,
    #[serde(default, alias = "expiresIn")]
    expires_in: Option<i64>,
    #[serde(default, alias = "refreshTokenExpiresAt")]
    refresh_token_expires_at: Option<Value>,
    #[serde(default, alias = "refreshTokenExpiresIn")]
    refresh_token_expires_in: Option<i64>,
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

/// 把接口返回的时间字段归一化成 ISO8601（可能是毫秒时间戳，也可能是已有字符串）
fn to_iso(v: &Value) -> String {
    if let Some(ts) = v.as_i64() {
        if ts <= 0 {
            return String::new();
        }
        let secs = if ts > 10_000_000_000 { ts / 1000 } else { ts };
        return chrono::DateTime::from_timestamp(secs, 0)
            .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
            .unwrap_or_default();
    }
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    String::new()
}

/// 没有 expiresAt 时用 expiresIn（秒）推算
fn iso_from_now(seconds: i64) -> String {
    if seconds <= 0 {
        return String::new();
    }
    chrono::Utc::now()
        .checked_add_signed(chrono::Duration::seconds(seconds))
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

/// 校验 refreshToken 是否看起来可用（不联网，便于在 UI 层提前拦掉明显无效的值）
pub fn looks_like_refresh_token(token: &str) -> bool {
    let t = token.trim();
    t.starts_with(REFRESH_TOKEN_PREFIX) && t.len() > REFRESH_TOKEN_PREFIX.len()
}

/// 在单个域名上发起刷新
async fn try_refresh_on_base(
    client: &reqwest::Client,
    base: &str,
    refresh_token: &str,
) -> Result<RefreshedTokens, String> {
    let url = format!("{}/api/v1/deviceToken/refresh", base);
    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("cosy-clienttype", "10")
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .send()
        .await
        .map_err(|e| format!("请求 {} 失败: {}", url, e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("读取响应失败: {}", e))?;

    if !status.is_success() {
        // 400/401 属于「凭证本身有问题」，不该再换域名重试；其它情况交给上层回退
        let err = classify_error(status.as_u16(), &text);
        log::warn!("[refresh] {} 刷新失败: {}", base, describe_status(status.as_u16()));
        return Err(err);
    }

    let parsed: RefreshResponse = serde_json::from_str(&text)
        .map_err(|e| format!("解析刷新响应失败: {} | body: {}", e, short(&text)))?;

    let token = parsed
        .device_token
        .or(parsed.access_token)
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| format!("刷新响应缺少 device_token 字段 | body: {}", short(&text)))?;

    let refresh_token = parsed
        .refresh_token
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| refresh_token.to_string());

    let expires_at = parsed
        .expires_at
        .as_ref()
        .map(to_iso)
        .filter(|s| !s.is_empty())
        .or_else(|| parsed.expires_in.map(iso_from_now))
        .unwrap_or_default();

    let refresh_token_expires_at = parsed
        .refresh_token_expires_at
        .as_ref()
        .map(to_iso)
        .filter(|s| !s.is_empty())
        .or_else(|| parsed.refresh_token_expires_in.map(iso_from_now))
        .unwrap_or_default();

    Ok(RefreshedTokens {
        token,
        refresh_token,
        expires_at,
        refresh_token_expires_at,
        api_base: base.to_string(),
    })
}

fn describe_status(code: u16) -> String {
    match code {
        400 => "400 请求被拒（凭证格式或字段有误）".to_string(),
        401 => "401 凭证无效或已失效".to_string(),
        403 => "403 无权限".to_string(),
        429 => "429 请求过于频繁".to_string(),
        c if c >= 500 => format!("{} 服务端异常", c),
        c => format!("{} 请求失败", c),
    }
}

/// 把 HTTP 错误翻译成用户能看懂的提示（服务端 errorCode 优先）
fn classify_error(code: u16, body: &str) -> String {
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let error_code = v.get("errorCode").and_then(|x| x.as_str()).unwrap_or("");
    let error_msg = v.get("errorMessage").and_then(|x| x.as_str()).unwrap_or("");

    match error_code {
        "DeviceRefreshTokenRequired" => {
            "刷新凭证缺失（服务端要求 refresh_token 字段）".to_string()
        }
        "DeviceRefreshTokenPrefixInvalid" => {
            format!("刷新凭证格式无效（必须以 {} 开头）", REFRESH_TOKEN_PREFIX)
        }
        _ => match code {
            401 => "刷新凭证已失效，需要重新在 IDE 登录一次".to_string(),
            400 => format!(
                "刷新请求被拒绝: {}",
                if error_msg.is_empty() { short(body) } else { error_msg.to_string() }
            ),
            429 => "刷新请求过于频繁，请稍后再试".to_string(),
            c if c >= 500 => format!("刷新接口返回 {}，请稍后再试", c),
            c => format!("刷新接口返回 {}: {}", c, short(body)),
        },
    }
}

/// 判断这个错误是否值得换域名重试。
/// 注意：401 也要继续回退 —— 用错域名（比如拿国际版凭证打 CN 域名）同样返回 401，
/// 只有「格式非法」这种本地就能判定的错误才没有回退价值。
fn is_retryable_on_other_base(err: &str) -> bool {
    !err.contains("格式无效") && !err.contains("刷新凭证缺失")
}

/// 刷新一个账号的凭证。`ide_kind` 决定优先域名（intl 走国际域名，其余 CN 优先），
/// 首个域名失败后仍会回退到另一个域名。
pub async fn refresh(refresh_token: &str, ide_kind: &str) -> Result<RefreshedTokens, String> {
    let rt = refresh_token.trim();
    if !looks_like_refresh_token(rt) {
        return Err(format!(
            "刷新凭证格式无效（必须以 {} 开头，当前值长度 {}）",
            REFRESH_TOKEN_PREFIX,
            rt.len()
        ));
    }

    let client = client()?;
    let mut last_err = String::from("刷新接口不可用");
    let mut seen_unauthorized = false;

    for base in bases_for_kind(ide_kind) {
        match try_refresh_on_base(&client, base, rt).await {
            Ok(tokens) => {
                log::info!(
                    "[refresh] {} 刷新成功（refreshToken 是否轮换: {}）",
                    base,
                    if tokens.refresh_token != rt { "是" } else { "否" }
                );
                return Ok(tokens);
            }
            Err(e) => {
                if !is_retryable_on_other_base(&e) {
                    log::warn!("[refresh] {} 凭证格式问题，停止回退: {}", base, e);
                    return Err(e);
                }
                if e.contains("已失效") {
                    seen_unauthorized = true;
                }
                log::warn!("[refresh] {} 刷新失败: {}", base, e);
                last_err = e;
            }
        }
    }

    // 两个域名都 401 → 才判定凭证真的失效
    if seen_unauthorized {
        return Err("刷新凭证已失效，需要重新在 IDE 登录一次".to_string());
    }
    Err(last_err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_only_drt_prefix() {
        assert!(looks_like_refresh_token("drt-abcdef123456"));
        assert!(!looks_like_refresh_token("drt-"));
        assert!(!looks_like_refresh_token("dt-abcdef"));
        assert!(!looks_like_refresh_token(""));
    }

    #[test]
    fn normalizes_expiry_fields() {
        // 毫秒时间戳（1797004800000 = 2026-12-11T16:00:00Z）
        assert_eq!(
            to_iso(&json!(1797004800000i64)),
            "2026-12-11T16:00:00Z"
        );
        // 已是 ISO 字符串则原样保留
        assert_eq!(to_iso(&json!("2026-12-14T14:41:34Z")), "2026-12-14T14:41:34Z");
        // 非法值
        assert_eq!(to_iso(&json!(0)), "");
        assert_eq!(to_iso(&json!(null)), "");
    }

    #[test]
    fn maps_server_error_codes() {
        let body = r#"{"errorCode":"DeviceRefreshTokenRequired","errorMessage":"refresh_token is required"}"#;
        assert!(classify_error(400, body).contains("刷新凭证缺失"));

        let prefix_body = r#"{"errorCode":"DeviceRefreshTokenPrefixInvalid","errorMessage":"invalid refresh_token: must start with drt-"}"#;
        let format_err = classify_error(400, prefix_body);
        assert!(format_err.contains("格式无效"));

        let unauthorized_body =
            r#"{"errorCode":"Unauthorized","errorMessage":"User not authenticated"}"#;
        let unauthorized_err = classify_error(401, unauthorized_body);
        assert!(unauthorized_err.contains("已失效"));

        // 格式类错误本地就能判定，换域名也没用
        assert!(!is_retryable_on_other_base(&format_err));
        // 401 必须继续回退：用错域名（国际版凭证打 CN 域名）同样返回 401
        assert!(is_retryable_on_other_base(&unauthorized_err));
        // 网络/5xx 也可以回退
        assert!(is_retryable_on_other_base("刷新接口返回 502，请稍后再试"));
    }

    /// 域名顺序必须跟着账号版本走，否则国际版凭证会被 CN 域名的 401 误判为失效
    #[test]
    fn base_order_follows_ide_kind() {
        let intl = bases_for_kind("intl");
        assert!(intl[0].ends_with(".sh"), "国际版应优先国际域名: {:?}", intl);

        let cn = bases_for_kind("cn");
        assert!(cn[0].ends_with(".cn"), "国内版应优先 CN 域名: {:?}", cn);

        // 未标记版本时按默认（CN 优先）
        assert_eq!(bases_for_kind(""), REFRESH_BASES.to_vec());
    }

    #[test]
    fn parses_real_snake_case_response() {
        // 实测真实响应（脱敏）
        let body = r#"{"device_token":"dt-x","refresh_token":"drt-y","token_type":"Bearer",
                       "expires_at":"2026-12-15T05:04:32Z","refresh_token_expires_at":"2027-11-10T05:04:32Z",
                       "created_at":"2026-11-15T05:04:32Z"}"#;
        let v: RefreshResponse = serde_json::from_str(body).unwrap();
        assert_eq!(v.device_token.as_deref(), Some("dt-x"));
        assert_eq!(v.refresh_token.as_deref(), Some("drt-y"));
        assert_eq!(
            v.expires_at.as_ref().map(to_iso).as_deref(),
            Some("2026-12-15T05:04:32Z")
        );
    }

    #[test]
    fn parses_lenient_response_shapes() {
        // camelCase 别名与 token 别名都兼容
        let body = r#"{"token":"dt-new","refreshToken":"drt-new","expiresAt":"2027-01-01T00:00:00Z","refreshTokenExpiresAt":"2028-01-01T00:00:00Z"}"#;
        let v: RefreshResponse = serde_json::from_str(body).unwrap();
        assert_eq!(v.device_token.as_deref(), Some("dt-new"));
        assert_eq!(v.refresh_token.as_deref(), Some("drt-new"));

        let body = r#"{"accessToken":"dt-a"}"#;
        assert_eq!(serde_json::from_str::<RefreshResponse>(body).unwrap().access_token.as_deref(), Some("dt-a"));
    }
}
