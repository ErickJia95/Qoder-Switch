use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};
use sysinfo::System;

/// 两个发行版本：Qoder CN（国内版）与 Qoder（国际版）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdeKind {
    Cn,
    Intl,
}

impl IdeKind {
    /// 解析前端传入的 kind 参数
    pub fn parse(raw: &str) -> Option<IdeKind> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "cn" | "qoder_cn" | "qoder-cn" | "qodercn" => Some(IdeKind::Cn),
            "intl" | "global" | "qoder" | "qoder_intl" | "qoder-intl" => Some(IdeKind::Intl),
            _ => None,
        }
    }

    pub fn key(&self) -> &'static str {
        match self {
            IdeKind::Cn => "cn",
            IdeKind::Intl => "intl",
        }
    }

    /// macOS 应用名（也是 .app 包名与主进程名）
    pub fn app_name(&self) -> &'static str {
        match self {
            IdeKind::Cn => "Qoder CN",
            IdeKind::Intl => "Qoder",
        }
    }

    /// macOS Bundle Identifier，移动/重命名应用后依然可定位
    pub fn bundle_id(&self) -> &'static str {
        match self {
            IdeKind::Cn => "com.qodercn.app",
            IdeKind::Intl => "com.qoder.app",
        }
    }

    /// 进程名关键字：CN 版含 "cn"，国际版不含
    fn cn_hints(&self) -> &'static [&'static str] {
        &["qoderwork cn", "qoder cn", "qoder-cn", "qodercn"]
    }

    fn intl_hints(&self) -> &'static [&'static str] {
        &["qoderwork", "qoder"]
    }
}

/// 本应用自身进程名，避免「qoder-proxy」被当成 IDE 正在运行
fn is_self_process(name: &str) -> bool {
    name.contains("qoder-proxy")
        || name.contains("qoder_proxy")
        || name.contains("qoder switch")
        || name.contains("qoderswitch")
}

fn classify(name: &str) -> Option<IdeKind> {
    let n = name.to_lowercase();
    if is_self_process(&n) {
        return None;
    }
    // 先判 CN，因为国际版关键字是 CN 关键字的子串
    if IdeKind::Cn.cn_hints().iter().any(|h| n.contains(h)) {
        return Some(IdeKind::Cn);
    }
    if IdeKind::Intl.intl_hints().iter().any(|h| n.contains(h)) {
        return Some(IdeKind::Intl);
    }
    None
}

/// 当前是否有一个该版本的 IDE 在运行
pub fn is_running(kind: IdeKind) -> bool {
    let mut sys = System::new_all();
    sys.refresh_all();
    let current = sysinfo::get_current_pid().ok();

    for (pid, process) in sys.processes() {
        if Some(*pid) == current {
            continue;
        }
        if classify(&process.name().to_string_lossy()) == Some(kind) {
            return true;
        }
    }
    false
}

/// 应用是否已安装（macOS 走 LaunchServices，找不到再退回常见路径）
pub fn is_installed(kind: IdeKind) -> bool {
    #[cfg(target_os = "macos")]
    {
        if Command::new("open")
            .args(["-Ra", kind.app_name()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return true;
        }
    }
    app_path(kind).is_some()
}

fn app_path(kind: IdeKind) -> Option<String> {
    let mut candidates = vec![format!("/Applications/{}.app", kind.app_name())];
    if let Some(home) = dirs::home_dir() {
        candidates.push(format!("{}/Applications/{}.app", home.display(), kind.app_name()));
    }
    candidates.into_iter().find(|p| Path::new(p).exists())
}

#[derive(Debug)]
pub struct RestartReport {
    pub was_running: bool,
    pub stopped: bool,
    pub launched: bool,
    pub message: String,
}

/// 退出指定版本的 Qoder：AppleEvent → TERM → KILL 逐级升级，返回是否已退出
pub fn quit(kind: IdeKind) -> bool {
    if !is_running(kind) {
        return true;
    }

    // 1) 优雅退出：让 IDE 自己收尾（保存窗口状态等）。
    //    走 AppleEvent，若系统弹出自动化授权框用户没理，osascript 会一直等，因此加超时兜底。
    let mut quit_cmd = Command::new("osascript");
    quit_cmd
        .args([
            "-e",
            &format!("tell application id \"{}\" to quit", kind.bundle_id()),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if !run_with_timeout(quit_cmd, Duration::from_secs(4)) {
        log::warn!("[ide] {} 的 AppleEvent 退出未生效，改用信号", kind.app_name());
    }
    if wait_until_stopped(kind, Duration::from_secs(8)) {
        return true;
    }

    // 2) 还被对话框拦住 → TERM
    log::warn!("[ide] {} 未在超时内退出，发送 TERM", kind.app_name());
    kill(kind, "-TERM");
    if wait_until_stopped(kind, Duration::from_secs(3)) {
        return true;
    }

    // 3) 仍然不动 → KILL
    log::warn!("[ide] {} 仍在运行，发送 KILL", kind.app_name());
    kill(kind, "-KILL");
    wait_until_stopped(kind, Duration::from_secs(3))
}

/// 重启（或启动）指定版本的 Qoder：先优雅退出，必要时强制结束，再重新打开
pub fn restart(kind: IdeKind) -> RestartReport {
    let was_running = is_running(kind);

    if !was_running && !is_installed(kind) {
        return RestartReport {
            was_running: false,
            stopped: true,
            launched: false,
            message: format!("未找到 {}，请确认已安装", kind.app_name()),
        };
    }

    let stopped = quit(kind);
    if !stopped {
        return RestartReport {
            was_running,
            stopped: false,
            launched: false,
            message: format!(
                "{} 未能退出（可能被未保存的对话框阻塞），请手动关闭后重试",
                kind.app_name()
            ),
        };
    }

    match launch(kind) {
        Ok(()) => RestartReport {
            was_running,
            stopped,
            launched: true,
            message: if was_running {
                format!("已重启 {}", kind.app_name())
            } else {
                format!("已启动 {}", kind.app_name())
            },
        },
        Err(e) => RestartReport {
            was_running,
            stopped,
            launched: false,
            message: format!("启动 {} 失败: {}", kind.app_name(), e),
        },
    }
}

fn kill(kind: IdeKind, signal: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        Command::new("killall")
            .args([signal, kind.app_name()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (kind, signal);
        false
    }
}

/// 执行外部命令并限制等待时间：超时就杀掉子进程，避免权限弹窗把主流程卡死
fn run_with_timeout(mut cmd: Command, timeout: Duration) -> bool {
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[ide] 启动命令失败: {}", e);
            return false;
        }
    };
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return false;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                log::warn!("[ide] 等待命令结束失败: {}", e);
                return false;
            }
        }
    }
}

fn wait_until_stopped(kind: IdeKind, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if !is_running(kind) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

pub fn launch(kind: IdeKind) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // 优先按 .app 路径打开；找不到则按 Bundle ID 让 LaunchServices 定位
        let mut cmd = Command::new("open");
        match app_path(kind) {
            Some(path) => {
                cmd.arg("-a").arg(path);
            }
            None => {
                cmd.arg("-b").arg(kind.bundle_id());
            }
        }
        let status = cmd
            .status()
            .map_err(|e| format!("调用 open 失败: {}", e))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("open 退出码 {:?}", status.code()))
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = kind;
        Err("当前系统暂不支持一键重启 IDE".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_prefers_cn_and_ignores_self() {
        assert_eq!(classify("Qoder CN"), Some(IdeKind::Cn));
        assert_eq!(classify("Qoder CN Helper (Renderer)"), Some(IdeKind::Cn));
        assert_eq!(classify("qoder-cn"), Some(IdeKind::Cn));
        assert_eq!(classify("Qoder"), Some(IdeKind::Intl));
        assert_eq!(classify("Qoder Helper (GPU)"), Some(IdeKind::Intl));
        // 本应用不能被误判为 IDE
        assert_eq!(classify("qoder-proxy"), None);
        assert_eq!(classify("Qoder Switch"), None);
        assert_eq!(classify("Safari"), None);
    }

    #[test]
    fn parses_kind_argument() {
        assert_eq!(IdeKind::parse("cn"), Some(IdeKind::Cn));
        assert_eq!(IdeKind::parse("qoder_cn"), Some(IdeKind::Cn));
        assert_eq!(IdeKind::parse("intl"), Some(IdeKind::Intl));
        assert_eq!(IdeKind::parse("qoder"), Some(IdeKind::Intl));
        assert_eq!(IdeKind::parse("nope"), None);
    }

    /// 只读检查：两个版本是否安装、是否在运行（不改动任何进程）
    #[test]
    fn reports_install_and_running_state() {
        for kind in [IdeKind::Cn, IdeKind::Intl] {
            println!(
                "IDE {} installed={} running={}",
                kind.app_name(),
                is_installed(kind),
                is_running(kind)
            );
        }
    }
}
