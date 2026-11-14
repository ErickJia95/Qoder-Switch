use sysinfo::System;

/// macOS 上 Qoder IDE 的进程名
const QODER_PROCESS_NAMES: &[&str] = &[
    "qoderwork cn",
    "qoder cn",
    "qoderwork",
    "qoder",
];

/// 检查 Qoder IDE 是否在运行（仅用于界面展示状态）
pub fn is_qoder_ide_running() -> bool {
    let mut sys = System::new_all();
    sys.refresh_all();

    let current_pid = sysinfo::get_current_pid().ok();

    for (pid, process) in sys.processes() {
        if Some(*pid) == current_pid {
            continue;
        }
        let name = process.name().to_string_lossy().to_lowercase();
        if is_qoder_process(&name) {
            return true;
        }
    }
    false
}

fn is_qoder_process(name: &str) -> bool {
    QODER_PROCESS_NAMES.iter().any(|p| {
        let p_lower = p.to_lowercase();
        name == p_lower || name.contains(&p_lower)
    })
}
