# Qoder Switch

**Qoder IDE 多账号管理与用量查询工具（无代理版）**

Qoder Switch 只做两件事：**收集多个 Qoder 账号**，以及**查看每个账号的用量**。它不再启动本地代理、不安装证书、不接管 IDE 流量，也不改动 IDE 的任何文件。

---

## 功能

- **账号导入**：读取本机 Qoder IDE 当前登录账号的凭证（macOS 钥匙串 + `auth.v1.dat` 解密），按 `user_id` 去重，重复导入只刷新凭证
- **账号切换**：把某个账号标记为「当前账号」，并按提示在 Qoder IDE 内用该账号重新登录（本工具不修改 IDE 登录态）
- **用量查询**：逐个或批量查询账号用量（已用 / 总量 / 剩余 / 到期 / 套餐），数据来自 `openapi.qoder.com.cn` 的账号用量接口，也就是 [qoder.cn/account/usage](https://qoder.cn/account/usage) 页面背后的接口；CN 接口不可用时自动回退到 `openapi.qoder.sh`
- **一键打开**：用系统浏览器打开 qoder.cn 用量页面
- **导出备份**：把所有账号信息导出成一份 JSON 文件（可选是否包含凭证 Token），默认保存到桌面，用于换机迁移或本地留档
- **导入恢复**：选择备份 JSON 合并回账号库 —— 按 `user_id` 去重、只覆盖非空字段、不删除现有账号；可选恢复「当前账号」标记
- **复制邮箱**：方便在 IDE 登录时填写账号
- **托盘常驻**：关闭窗口后仍在托盘中，双击托盘图标恢复窗口

已移除：MITM 代理引擎、CA 证书安装、流量日志、域名管理、仪表盘统计、会话管理、手动粘贴 Token、多语言切换。

---

## 快速开始

1. 在 Qoder IDE 中登录一个要收集的账号
2. 打开 Qoder Switch，点击顶部「读取当前登录账号」，该账号会出现在左侧账号列表
3. 换账号 → 在 IDE 里退出并登录另一个账号 → 再点一次「读取当前登录账号」
4. 需要知道某个账号还剩多少用量时，点该行「查用量」，或点顶部「查询全部用量」
4.1 换电脑/重装后，点顶部「从 JSON 导入」选中之前的备份文件，账号库即可原样还原
5. 想用哪个账号工作：点该行「接入」，然后在 Qoder IDE 里用这个账号登录即可

> 凭证有效期约 30 天，过期后只要在 IDE 重新登录一次，再点「读取当前登录账号」即可刷新（同 `user_id` 原地更新，不会产生重复账号）。
> 本地登录态读取依赖 macOS 登录钥匙串与 Qoder IDE 的 `auth.v1.dat`，需要本机已安装并登录过 Qoder IDE。

---

## 技术栈

| 层级 | 技术 | 说明 |
|------|------|------|
| 前端 | Vue 3 + TypeScript + TailwindCSS | Pinia 状态管理，两个页面：账号切换 / 用量查询 |
| 桌面框架 | Tauri v2 | 原生窗口、系统托盘、IPC |
| 后端 | Rust | 凭证读取与加密存储、用量接口调用 |
| 用量接口 | reqwest + serde | `openapi.qoder.com.cn` / `openapi.qoder.sh` 的 v2 接口 |

### 项目结构

```
Qoder-Switch/
├── src/
│   ├── components/
│   │   ├── AccountForm.vue     # 顶部操作栏（读取账号 / 批量查用量 / 打开用量页）
│   │   ├── AccountList.vue     # 账号列表（接入 / 查用量 / 复制邮箱 / 删除）
│   │   ├── UsagePanel.vue      # 用量卡片（进度条 / 剩余 / 到期）
│   │   ├── StatusBar.vue       # 顶部状态栏
│   │   └── NavItem.vue
│   ├── stores/app.ts           # Pinia 状态
│   ├── types.ts
│   └── App.vue
├── src-tauri/src/
│   ├── auth/token.rs           # 账号存储（AES-256-GCM）
│   ├── commands/               # IPC 命令：account / quota / app / backup（导出导入）
│   ├── local_account.rs        # 本机 IDE 登录态读取（OSCrypt v10）
│   ├── quota.rs                # 用量接口（CN 优先，国际回退）
│   ├── process.rs              # 判断 IDE 是否运行
│   ├── state.rs / tray.rs / lib.rs
│   └── Cargo.toml
├── package.json
└── vite.config.ts
```

### 导出备份格式

点顶部「导出备份 (JSON)」会弹出系统「另存为」对话框，默认文件名 `qoder-switch-accounts-YYYYMMDD-HHMMSS.json`：

```json
{
  "schema": "qoder-switch/accounts",
  "schema_version": 1,
  "app": "Qoder Switch",
  "app_version": "2.2.0",
  "exported_at": "2026-11-14T22:30:00+08:00",
  "account_count": 2,
  "includes_tokens": true,
  "accounts": [
    {
      "id": "tok_...",
      "label": "本地 · 某某",
      "user_id": "...",
      "email": "someone@example.com",
      "name": "某某",
      "user_type": "Pro",
      "expire_date": "2026-12-11",
      "quota_used": 447,
      "quota_total": 2000,
      "is_current": true,
      "token": "..."
    }
  ]
}
```

- 勾选「含凭证 Token」时 `token` 字段为明文凭证，文件可直接用于迁移/恢复，**请妥善保管**
- 取消勾选则不写 `token`，只有账号元信息，适合发给别人排查问题

### 导入 / 恢复规则

点顶部「从 JSON 导入」选择文件后按下表合并，**不会删除**任何已有账号：

| 情况 | 行为 |
|------|------|
| 备份账号的 `user_id` 与库中相同 | 更新：覆盖非空的 label/name/email/user_type/expire_date/quota/token |
| `user_id` 为空，但 `id` 或 `email` 能对上 | 同上，按 `id` → `email` 顺序匹配 |
| 匹配不到且带 `token` | 作为新账号加入账号库（重新加密落盘） |
| 匹配不到且不带 `token` | 跳过并计入「跳过数」（没有凭证无法使用） |

- 勾选「恢复当前标记」时，若备份里有 `is_current: true` 的账号，导入后会把它设为当前账号
- 兼容两种输入格式：本应用导出的 `{ "accounts": [ ... ] }` 包装对象，以及裸数组 `[ ... ]`
- 字段名同时兼容 snake_case 与 camelCase（`userId` / `userType` / `quotaTotal` 等），手改过的文件也能读

### 数据存放

- 账号库：`~/Library/Application Support/Qoder/proxy-tokens.json`（AES-256-GCM 加密，密钥由机器与用户名派生）
- 旧版遗留文件 `proxy-config.json`、`proxy-ca/`、`proxy-traffic.log` 已不再使用，可手动删除

---

## 开发

```bash
pnpm install
pnpm tauri dev      # 开发模式
pnpm tauri build    # 打包 dmg / app
```

环境要求：Node.js ≥ 18、pnpm ≥ 8、Rust ≥ 1.75、macOS 10.15+ 与 Xcode Command Line Tools。

---

## 许可证

[MIT License](./LICENSE)
