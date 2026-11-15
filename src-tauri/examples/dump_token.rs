//! 临时调试：解密本地 token 库，打印脱敏账号信息，完整 token 写临时文件供 curl 使用（不打印到 stdout）
use qoder_proxy_lib::token::TokenManager;

#[tokio::main]
async fn main() {
    let path = dirs::data_dir().unwrap().join("Qoder").join("proxy-tokens.json");
    let tm = TokenManager::new(path);
    tm.load().await.expect("load failed");
    let accounts = tm.list().await;
    for a in accounts {
        let masked: String = a.token.chars().take(10).collect();
        println!(
            "id={} label={} email={} user_type={} status={} token_prefix={} token_len={}",
            a.id, a.label, a.email, a.user_type, a.status, masked, a.token.len()
        );
        std::fs::write(format!("/tmp/qsw_tok_{}", a.id), &a.token).ok();
    }
}
