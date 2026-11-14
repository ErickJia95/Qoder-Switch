export interface AccountToken {
  id: string
  label: string
  token: string
  user_id: string
  email: string
  name: string
  status: string // "available" | "current"
  user_type: string
  expire_date: string
  quota_used?: number | null
  quota_total?: number | null
}

/** 应用状态（不含任何代理/证书信息） */
export interface AppStatus {
  account_count: number
  current_account_id: string | null
  current_label: string | null
  ide_running: boolean
  ide_cn_running: boolean
  ide_cn_installed: boolean
  ide_intl_running: boolean
  ide_intl_installed: boolean
  usage_url: string
}

/** Qoder 的两个发行版本 */
export type IdeKind = 'cn' | 'intl'

export interface RestartResult {
  kind: string
  was_running: boolean
  stopped: boolean
  launched: boolean
  message: string
}

export interface QuotaResult {
  account_id: string
  label: string
  plan_name: string
  user_type: string
  quota_used: number
  quota_total: number
  quota_remaining: number
  quota_unit: string
  usage_percentage: number
  is_exceeded: boolean
  expire_date: string
  api_base: string
  error: string | null
}

export interface SwitchInfo {
  id: string
  label: string
  email: string
}

/** 从本机 Qoder IDE 登录态导入的账号信息 */
export interface LocalAccountInfo {
  label: string
  user_id: string
  name: string
  expires_at: string
  updated: boolean
}

/** 导出备份的结果 */
export interface ExportResult {
  saved: boolean
  path: string | null
  count: number
  bytes: number
  includes_tokens: boolean
}

/** 从备份导入的结果 */
export interface ImportResult {
  imported: boolean
  path: string | null
  added: number
  updated: number
  skipped: number
  total: number
  restored_current: string | null
}
