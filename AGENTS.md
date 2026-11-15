# AGENTS.md — AI 编码规范

本文件是 AI 编码助手在本仓库工作的强制规范。改动代码前先读完本文。

## 项目概览

Qoder Switch：Qoder IDE 多账号管理与用量查询的 macOS 桌面工具。

- 前端：Vue 3 + TypeScript + Pinia + TailwindCSS（`src/`）
- 后端：Rust + Tauri v2（`src-tauri/`），IPC 命令在 `src-tauri/src/commands/`
- 数据：账号库 AES-256-GCM 加密落盘，涉及凭证 Token 等敏感信息

## 常用命令

```bash
pnpm tauri dev      # 开发调试
pnpm build          # 类型检查（vue-tsc --noEmit）+ 前端构建
pnpm tauri build    # 打包 dmg / app
```

提交前必须通过 `pnpm build`（类型检查零报错）；改了 Rust 代码还需 `cargo check` 通过。

---

## 一、必须打印日志

**原则：关键操作有迹可循，排查问题不靠猜。**

### Rust 端（使用 `log` crate，禁止 `println!`/`eprintln!`）

- 新增/修改 `#[tauri::command]` 时，入口处 `log::info!` 记录关键参数（不含敏感值），失败处 `log::error!`/`log::warn!` 记录错误与上下文
- 所有外部接口调用（`quota.rs`、`checkin.rs`、`auth/refresh.rs` 等）必须打印：请求目标（域名/接口）、结果、失败原因。失败必须留日志，不允许静默返回
- 日志统一带 `[模块]` 前缀，如 `[ide]`、`[checkin]`、`[quota]`、`[account]`、`[backup]`，沿用现有风格
- 级别约定：`info` = 关键节点（已写入/已备份/领取成功）；`warn` = 可恢复异常（超时降级、无可用活动）；`error` = 操作失败
- **严禁打印 token、auth_blob、refreshToken 等凭证内容**，只记录账号标识（label/user_id 掩码）与文件路径

### 前端

- 调用后端接口（`invoke`）的关键操作，在 `catch` 中必须 `console.error('[模块] 操作名', e)` 留下排查线索
- 临时调试用 `console.log`，提交前删除或降级为注释说明
- 批量/轮询类操作记录汇总结果（成功数/失败数），不逐条刷屏

---

## 二、接口报错必须 Toast

**原则：任何接口失败都要让用户看到，且用 toast，不是 alert。**

1. 所有 `invoke(...)` 调用必须在 UI 层用 `try/catch` 捕获；**catch 后必须弹 toast 提示用户**，禁止静默吞错、禁止只写 console
2. 空的 `catch {}` 仅允许用于确实无需打扰用户的后台轮询（如 `StatusBar` 的运行状态刷新），且需留 `console.error`
3. Toast 文案格式：`「操作名」失败：<具体原因>`，原因取 `e?.message || String(e)`
4. 成功反馈也用 toast（区分成功/失败样式），如「已导出 N 个账号」
5. **新代码禁止使用 `alert()` 做提示**；存量 `alert()`（AccountForm/AccountList/SwitchDialog/StatusBar/UsagePanel）在触碰相应文件时一并迁移为 toast
6. Toast 机制优先复用全局实现（如 `src/composables/useToast.ts` + ToastHost 挂载在 `App.vue`）；项目尚无该机制时，先实现再使用，不要各组件各自弹
7. 长耗时操作要有 loading 状态（按钮禁用/spinner），并在 `finally` 中复位，失败时同样复位

---

## 三、其他约定

- **敏感数据**：不把 token/auth_blob 写入日志、console、测试文件；导出含凭证的备份文件路径可以打印，内容不行
- **接口容错**：外部接口遵循现有「CN 域名优先、国际域名回退」模式（`openapi.qoder.com.cn` → `openapi.qoder.sh`），新接口沿用
- **凭证刷新**：`POST /api/v1/deviceToken/refresh`，body `{"refresh_token":"drt-..."}`，响应为 **snake_case**（`device_token`/`refresh_token`/`expires_at`/`refresh_token_expires_at`）。域名顺序必须跟着账号版本走（`intl` 优先国际域名，否则会被 CN 域名的 401 误判成「凭证失效」）。刷新会轮换 `refreshToken`，必须按「先本地库、再 IDE 文件」顺序落盘，同一账号刷新要用 `RefreshLocks` 串行
- **文案语言**：UI 文案用简体中文
- **样式**：TailwindCSS，跟随现有深色 UI 风格，不引入新的 UI 框架
- **类型**：新增 IPC 命令的返回类型定义在 `src/types.ts`，前后端字段保持 snake_case/camelCase 一致（前端 camelCase，Rust serde rename）
- **不要改动**：`dist/` 构建产物、`pnpm-lock.yaml`（除非依赖变更）、`.workbuddy/` 目录
- **提交**：改动聚焦单一目的，不顺手重构无关代码
