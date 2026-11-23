# Qoder Switch

> macOS 上的 Qoder IDE 多账号管理工具：把多个账号的登录态收进本地账号库，随时切换、查用量、领签到、给凭证续期。

**只在本机工作**：不启动代理、不安装证书、不接管 IDE 流量、不上传账号数据。唯一会改动 IDE 文件的操作，是你主动点击「接入」。

| | |
|---|---|
| 平台 | macOS 10.15+（Apple Silicon / Intel） |
| 当前版本 | v2.4.0（以 `package.json` / `src-tauri/tauri.conf.json` 为准） |
| 技术栈 | Vue 3 + TypeScript + Pinia + TailwindCSS · Tauri v2 + Rust |
| 许可证 | [MIT](./LICENSE) |

---

## 能做什么

| 功能 | 说明 |
|------|------|
| 收集账号 | 读取本机 Qoder CN / Qoder 当前登录账号的凭证，按 `user_id` 去重入库，重复读取只刷新凭证 |
| 切换登录态 | 点「接入」，退出对应 IDE → 写入该账号的登录态 → 重新打开，打开即是该账号；原登录态自动备份、可一键回退 |
| 凭证续期 | 用 `refreshToken` 换新访问凭证，不用为每个账号轮流登录 IDE；支持单个 / 批量 / 查询前自动续期 |
| 用量查询 | 逐个或批量查看套餐、已用 / 总量 / 剩余、到期、是否超额，以及单独展示「个人资源包」 |
| 每日签到 | 给所有账号逐个领取签到奖励，逐账号显示「领取成功 / 今日已领 / 无可用活动 / 失败」 |
| 备份迁移 | 导出为 JSON（可选是否含凭证），换机或重装后从 JSON 合并恢复 |
| IDE 状态与托盘 | 顶部显示两个版本的安装 / 运行状态并可重启；关闭窗口后托盘常驻，双击恢复 |

### 不做什么

- 不做 MITM 代理、不装 CA 证书、不记录流量、不改 IDE 安装包与配置（除 `auth.v1.dat`）
- 不提供手动粘贴 Token 的入口；账号只能来自本机 IDE 登录态或备份 JSON
- 不把凭证发送给第三方；所有网络请求只发往官方 `openapi.qoder.com.cn` / `openapi.qoder.sh`
- 顶栏「检测更新」目前只显示当前版本号，自动更新尚未实现

---

## 开始使用

### 前提

1. macOS 10.15+，已安装 Qoder CN 或 Qoder，且至少登录过一个账号。
2. 首次读取时允许访问 macOS 登录钥匙串 —— IDE 的 safeStorage 加密密钥存放在这里。
3. 依赖文件 `~/Library/Application Support/com.qodercn.app.stable/auth.v1.dat`（国内版）或 `com.qoder.app.stable/auth.v1.dat`（国际版）。

### 获取应用

- 从 GitHub Releases 下载对应架构的 `.dmg`（CI 同时构建 arm64 与 x86_64）。
- 或从源码构建，见下方「开发」。

### 五步上手

1. 在 IDE 登录账号 A → 点顶部「**读取当前登录账号**」，账号进入左侧列表。
2. 在 IDE 退出并登录账号 B → 再点一次「读取当前登录账号」。同一个 `user_id` 不会重复添加，只会原地刷新凭证。
3. 点「**查询全部用量**」或单行的「查用量」看额度；点「**一键领取签到**」或单行的「签到」领每日奖励。
4. 想换回账号 A：点该行「**接入**」→「切换到该账号并重启 Qoder CN」。只想改标记、不动 IDE，选「仅标记为当前账号」。
5. 换机 / 重装：点「**从 JSON 导入**」选择之前用「**导出备份 (JSON)**」生成的文件，账号库即可还原。

> ⚠️ 切换会退出 IDE，未保存的编辑器内容可能丢失。切换前的登录态已备份，弹窗里提供「恢复切换前的登录态」一键回退。

---

## 功能详解

### 1. 账号库

Qoder IDE 把登录态存在 `auth.v1.dat`：内容是 Chromium/Electron safeStorage（OSCrypt v10，`v10` 前缀 + AES-128-CBC）加密的 JSON，密钥由登录钥匙串里的 safeStorage 口令派生。本工具读取两个版本的文件，解密后保存访问凭证、完整登录数据 `auth_blob`（含 `refreshToken`）与 IDE 版本标记。

- 去重键为 `user_id`；导入时同时清空旧的额度缓存并重新落盘。
- 列表展示名称、邮箱、IDE 版本、套餐、凭证到期日、剩余用量、当前标记；到期前 3 天会显示「将过期」。
- 前端拿到的账号视图经过脱敏，**不含** `token` / `auth_blob`。
- 可复制邮箱、删除账号。

### 2. 切换账号（接入）

**必要条件**：账号有完整登录数据（含 `refreshToken`）、对应版本 IDE 已安装。缺数据时会直接拒绝切换并提示先登录再读取 —— 实测用旧版 token 拼出的「最小登录数据」会被 IDE 启动校验判为无效并清除，表现为切换后掉登录，比不切换更糟。

流程：

1. 退出对应 IDE（AppleEvent 优雅退出 → `TERM` → `KILL` 逐级兜底），避免它在退出时覆盖我们写入的文件。
2. 把原文件备份到 `~/Library/Application Support/Qoder/auth-backup-<cn|intl>.dat`。
3. 用钥匙串密钥重新加密，经临时文件 + 原子 `rename` 写入目标账号的登录态，并立即回读校验。
4. 重新打开 IDE，约 6 秒后回读 `auth.v1.dat` 校验 `user.id` / `token`，并顺手把 IDE 可能刷新过的新凭证同步回账号库。

校验不通过会明确提示，可用弹窗里的「恢复切换前的登录态」回退并重启 IDE。

### 3. 凭证续期

IDE 登录态里有两种凭证：访问凭证 `dt-`（约 30 天）、刷新凭证 `drt-`（约 1 年）。账号库里的账号没被 IDE 登录时没人替它刷新，所以这里补上自行续期。

- 接口：`POST /api/v1/deviceToken/refresh`，body `{"refresh_token":"drt-..."}`；响应为 **snake_case**（`device_token` / `refresh_token` / `expires_at` / `refresh_token_expires_at`）。
- 域名顺序跟着账号版本走：CN 版优先 `openapi.qoder.com.cn`，国际版优先 `openapi.qoder.sh`。用错域名会拿到 401，容易被误判成「凭证失效」。
- 单账号「刷凭证」强制刷新；「刷新全部凭证」只处理快过期的；查用量 / 签到前若剩余不足 **3 天**会自动续期（刷新失败不阻断原操作）。
- 刷新会轮换 `refreshToken`：同一账号按锁串行，且严格按「**先写本地账号库、再写 IDE 文件**」顺序落盘，避免服务端轮换旧值后两边同时失效。
- 若 `drt-` 本身已失效（401），只能在 IDE 重新登录该账号，再点一次「读取当前登录账号」补齐。

### 4. 用量查询

数据来自 [qoder.cn/account/usage](https://qoder.cn/account/usage) 页面背后的官方接口：

- `GET /api/v2/quota/usage` 为主，`GET /api/v2/user/plan` 补套餐与到期信息；Bearer token，15 秒超时。
- CN 域名优先、国际域名回退，接口返回的 `addOnQuota`（个人资源包）会单独成组展示。
- 逐账号串行查询避免限流；成功后把套餐、到期日、用量缓存回账号库，列表中的「剩余用量」同步更新。

### 5. 每日签到

- 链路：`GET /sash/api/v1/me/campaigns` 找 `CLAIM_BENEFIT` 且 `CLAIMABLE` 的活动 → `POST /sash/api/v1/me/campaigns/{id}/claim` 领取，带 `cosy-clienttype: 10` 桌面端标识。
- 接口幂等：重复点击返回 `replayed`，不会重复领取。奖励额度与有效期以接口返回为准，界面逐账号展示并汇总本次领取的总 Credits。
- 出现「无可用签到活动」时，国际版账号可能需要先在官方客户端领取一次。
- 签到前同样会兜底刷新快过期的凭证；单个账号失败不影响其它账号。

### 6. 备份与恢复

**导出**：点「导出备份 (JSON)」**直接写文件，不弹「另存为」对话框**（早期版本用过原生面板，部分 macOS 环境下会挂起）。目录优先级 `~/Desktop` → `~/Downloads` → 家目录，文件名 `qoder-switch-accounts-YYYYMMDD-HHMMSS.json`。导出后可直接「打开所在文件夹」或「复制路径」。

- 勾选「**含凭证 Token**」（默认勾选）：写入 `token` 与 `auth_blob` 明文，可用于完整迁移，**务必妥善保管、用完及时删除**。
- 不勾选：只写账号元信息，适合发给别人排查问题。

```json
{
  "schema": "qoder-switch/accounts",
  "schema_version": 1,
  "app": "Qoder Switch",
  "app_version": "2.4.0",
  "exported_at": "2026-11-23T09:00:00+08:00",
  "account_count": 1,
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
      "token": "<明文访问凭证>",
      "auth_blob": "<完整登录数据 JSON 字符串，含 refreshToken>",
      "ide_kind": "cn"
    }
  ]
}
```

**导入**：点「从 JSON 导入」在文件选择框中选文件（默认打开导出目录），按下表合并，**不会删除**任何已有账号：

| 情况 | 行为 |
|------|------|
| `user_id` 与库中相同 | 更新：覆盖非空字段（label / name / email / 套餐 / 到期 / 用量 / token） |
| `user_id` 为空但 `id` 或 `email` 能对上 | 同上，按 `id` → `email` 顺序匹配 |
| 匹配不到且带 `token` | 作为新账号加入并重新加密落盘 |
| 匹配不到且不带 `token` | 跳过并计入「跳过数」（没有凭证无法使用） |

- 勾选「恢复当前标记」时，会把备份里 `is_current: true` 的账号设为当前账号。
- 兼容 `{ "accounts": [...] }` 包装对象与裸数组 `[ ... ]`；字段名同时兼容 snake_case 与 camelCase（`userId` / `userType` / `quotaTotal` 等），手改过的文件也能读。

### 7. 托盘、状态栏与交互反馈

- 顶部状态栏分别显示 Qoder CN（`com.qodercn.app`）与 Qoder（`com.qoder.app`）的安装 / 运行状态，可单独「重启 / 启动」。
- 关闭窗口只是隐藏；托盘常驻，双击图标或菜单「显示主窗口」恢复，托盘菜单可直接打开用量页面。
- 操作类接口失败一律通过全局 Toast 提示「操作名失败：具体原因」，成功也有 Toast；批量操作汇总成功 / 跳过 / 失败数；长耗时操作带 loading 与按钮禁用。

---

## 数据与隐私

| 路径 | 内容 | 说明 |
|------|------|------|
| `~/Library/Application Support/Qoder/proxy-tokens.json` | 账号库 | AES-256-GCM 加密（12 字节 nonce + 密文 + tag）。密钥由 `HOSTNAME` + `USER` 做 SHA-256 派生；GUI 启动时 `HOSTNAME` 常为空、回退 `default_host`。**这不是硬件绑定的强密钥**，请勿把它当成保险箱 |
| `~/Library/Application Support/Qoder/auth-backup-<cn\|intl>.dat` | 切换前的 IDE 登录态 | 与 IDE 原文件同样的 safeStorage 加密格式 |
| `.../com.qodercn.app.stable/auth.v1.dat`、`.../com.qoder.app.stable/auth.v1.dat` | IDE 登录态 | 读取；仅在「接入」时写入 |
| `~/Desktop/qoder-switch-accounts-*.json` | 导出的备份 | 勾选含凭证时是**明文** |

- 网络请求只发往 `https://openapi.qoder.com.cn` 与 `https://openapi.qoder.sh`（用量 / 签到 / 凭证刷新）。
- 日志：Rust 端用 `log` crate 并带 `[模块]` 前缀（`[ide]` / `[quota]` / `[checkin]` / `[refresh]` / `[export]` 等），前端失败用 `console.error('[模块] ...')`。**从不打印 token、auth_blob、refreshToken**，只记录账号标识与文件路径。
- 旧版遗留文件 `proxy-config.json`、`proxy-ca/`、`proxy-traffic.log` 已不再使用，可手动删除。

---

## 项目结构

```
Qoder-Switch/
├── src/                              # 前端（Vue 3 + TS + TailwindCSS）
│   ├── App.vue                       # 布局：状态栏 / 操作栏 / 两个页签 / 底部栏 / ToastHost
│   ├── components/
│   │   ├── AccountForm.vue           # 顶部操作栏：读取账号、批量查询、签到、刷新、导入导出
│   │   ├── AccountList.vue           # 账号表格与逐行操作
│   │   ├── SwitchDialog.vue          # 「接入」弹窗：切换 / 仅标记 / 恢复登录态
│   │   ├── UsagePanel.vue            # 用量卡片（进度条、个人资源包）
│   │   ├── StatusBar.vue             # 版本号、IDE 运行状态、重启入口
│   │   ├── ToastHost.vue             # 全局 Toast 容器
│   │   └── NavItem.vue
│   ├── composables/useToast.ts       # 全局 Toast 与错误文案归一化
│   ├── stores/app.ts                 # Pinia 状态与所有 invoke 调用
│   ├── types.ts                      # IPC 返回类型定义
│   ├── main.ts / styles.css
├── src-tauri/src/                    # 后端（Rust）
│   ├── lib.rs                        # Tauri 初始化、IPC 命令注册、关闭即隐藏
│   ├── main.rs / state.rs / tray.rs / error.rs
│   ├── commands/                     # IPC 命令：account / ide / quota / checkin / refresh / backup / app
│   ├── auth/
│   │   ├── token.rs                  # 账号库 AES-256-GCM 存取、脱敏视图
│   │   ├── refresh.rs                # deviceToken/refresh 接口与响应解析
│   │   └── session.rs                # 刷新编排：按账号加锁、落盘顺序、自动续期阈值
│   ├── local_account.rs              # IDE 登录态读写（钥匙串 + OSCrypt v10）、备份 / 恢复
│   ├── quota.rs / checkin.rs         # 官方接口调用（CN 优先、国际回退）
│   ├── process.rs                    # 两个 IDE 版本的安装 / 运行检测、退出与重启
│   ├── Cargo.toml / tauri.conf.json / capabilities/
│   └── icons/
├── .github/workflows/build.yml       # macOS 双架构构建与 Release
├── AGENTS.md                         # 仓库编码约定（日志、Toast、类型、提交检查）
├── package.json / vite.config.ts / tailwind.config.js
└── LICENSE
```

---

## 开发

**环境**：Node.js ≥ 18、pnpm ≥ 8（CI 用 pnpm 12）、Rust ≥ 1.75、macOS 10.15+ 与 Xcode Command Line Tools。

```bash
pnpm install
pnpm tauri dev        # 开发调试（Vite 1420 + Tauri 窗口）
pnpm build            # vue-tsc --noEmit && vite build，提交前必须通过
pnpm tauri build      # 打包 .app / .dmg
cd src-tauri && cargo check    # 改过 Rust 代码后必须通过
cd src-tauri && cargo test     # 备份解析/合并、导出往返、用量响应解析等单测
```

**约定**（完整规范见 [AGENTS.md](./AGENTS.md)）：

- 新增 / 修改 `#[tauri::command]`：入口 `log::info!` 记录关键参数，失败 `log::error!` / `log::warn!` 记录上下文；所有外部接口必须记录请求目标、结果、失败原因；禁止 `println!`、禁止打印凭证。
- 前端所有 `invoke(...)` 在 UI 层 `try/catch`，失败必须弹全局 Toast（禁止 `alert()`、禁止静默吞错）；长耗时操作要有 loading 并在 `finally` 复位。
- 新增 IPC 命令的返回类型写在 `src/types.ts`；前端 camelCase、Rust 侧 serde rename 对齐。
- UI 文案用简体中文，样式用 TailwindCSS 并沿用现有深色风格。
- 不要改动 `dist/`、`pnpm-lock.yaml`（除非依赖变更）、`.workbuddy/`；提交聚焦单一目的。

**CI / 发布**：`.github/workflows/build.yml` 在 push `master`、PR、`v*` tag 和手动触发时构建，矩阵为 `macos-latest`（arm64）与 `macos-13`（x86_64）。打 `v*` tag 会自动创建 Release 并上传 `.app` / `.dmg`；普通 push 只上传构建产物 artifacts。

---

## 常见问题

**「读取当前登录账号」失败？** 确认对应版本的 Qoder 已登录、`auth.v1.dat` 存在，且已允许本应用读取登录钥匙串中的 Safe Storage 条目。

**「接入」按钮不可用 / 提示缺完整登录数据？** 该账号是旧版备份或不含凭证的导出文件，缺 `refreshToken`。先在 IDE 登录它，再点「读取当前登录账号」补齐，之后即可一键切换。

**凭证刷新一直 401？** `drt-` 已失效（或被跨版本域名误判）。确认账号的 IDE 版本标记正确；仍失败则在 IDE 重新登录该账号再读取一次。

**签到显示「无可用签到活动」？** 活动由服务端下发，国际版账号可能需先在官方客户端领取一次；该状态不是失败，可次日再试。

**用量查询报超时 / 接口错误？** 请求超时为 15 秒，CN 域名失败会自动回退国际域名；卡片底部会显示本次实际使用的域名，便于定位网络问题。

**切换后 IDE 仍是旧账号？** 可能被 IDE 会话缓存覆盖，工具会明确提示。用「接入」弹窗里的「恢复切换前的登录态」回退后重试。

---

## 许可证

[MIT License](./LICENSE)
