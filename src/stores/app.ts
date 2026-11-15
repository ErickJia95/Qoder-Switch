import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import type { AccountToken, AppStatus, QuotaResult, LocalAccountInfo, SwitchInfo, ExportResult, ImportResult, IdeKind, RestartResult, ActivateInfo, CheckinResult } from '../types'

export const USAGE_PAGE_URL = 'https://qoder.cn/account/usage'

export const useAppStore = defineStore('app', {
  state: () => ({
    accounts: [] as AccountToken[],
    status: null as AppStatus | null,
    quotas: [] as QuotaResult[],
    activeTab: 'accounts' as 'accounts' | 'usage',
    importing: false,
    checkingAll: false,
    checkingIds: [] as string[],
    checkingIn: false,
    checkinDone: 0,
  }),

  getters: {
    currentAccount(state): AccountToken | null {
      return state.accounts.find(a => a.status === 'current') || null
    },
    hasAccounts(state): boolean {
      return state.accounts.length > 0
    },
  },

  actions: {
    async fetchStatus() {
      this.status = await invoke<AppStatus>('get_app_status')
    },
    async fetchAccounts() {
      this.accounts = await invoke<AccountToken[]>('list_accounts')
    },
    async refreshAll() {
      await Promise.all([this.fetchAccounts(), this.fetchStatus()])
    },
    async importLocalAccount(): Promise<LocalAccountInfo> {
      const info = await invoke<LocalAccountInfo>('import_local_account')
      await this.refreshAll()
      return info
    },
    async switchAccount(id: string): Promise<SwitchInfo> {
      const info = await invoke<SwitchInfo>('switch_account', { id })
      await this.refreshAll()
      return info
    },
    /** 真正切换：退出 IDE → 写入该账号登录态 → 重新打开 */
    async activateAccount(id: string): Promise<ActivateInfo> {
      const info = await invoke<ActivateInfo>('activate_account', { id })
      await this.refreshAll()
      return info
    },
    async removeAccount(id: string) {
      await invoke('remove_account', { id })
      this.quotas = this.quotas.filter(q => q.account_id !== id)
      await this.refreshAll()
    },
    async checkQuota(id: string): Promise<QuotaResult> {
      if (!this.checkingIds.includes(id)) this.checkingIds.push(id)
      try {
        const result = await invoke<QuotaResult>('check_quota', { id })
        const idx = this.quotas.findIndex(q => q.account_id === id)
        if (idx >= 0) this.quotas[idx] = result
        else this.quotas.push(result)
        await this.fetchAccounts()
        return result
      } finally {
        this.checkingIds = this.checkingIds.filter(x => x !== id)
      }
    },
    async checkAllQuotas() {
      if (this.checkingAll) return
      this.checkingAll = true
      try {
        this.quotas = await invoke<QuotaResult[]>('check_all_quotas')
        await this.fetchAccounts()
      } finally {
        this.checkingAll = false
      }
    },
    /** 所有账号一键签到：前端循环逐个调用，单账号失败不中断，返回逐账号结果 */
    async checkinAll(): Promise<CheckinResult[]> {
      if (this.checkingIn) return []
      this.checkingIn = true
      this.checkinDone = 0
      const results: CheckinResult[] = []
      try {
        const targets = [...this.accounts]
        for (const account of targets) {
          try {
            const r = await invoke<CheckinResult>('checkin_account', { id: account.id })
            results.push(r)
          } catch (e: any) {
            const msg = typeof e === 'string' ? e : (e?.message || JSON.stringify(e))
            results.push({
              account_id: account.id,
              label: account.label,
              status: 'error',
              amount: 0,
              expires_at: '',
              message: msg,
              api_base: '',
            })
          }
          this.checkinDone++
        }
        // 领取成功后用量可能变化，顺手刷新账号列表
        await this.fetchAccounts()
        return results
      } finally {
        this.checkingIn = false
      }
    },
    /** 单个账号签到 */
    async checkinOne(id: string): Promise<CheckinResult> {
      const result = await invoke<CheckinResult>('checkin_account', { id })
      await this.fetchAccounts()
      return result
    },
    async restoreAuthBackup(kind: IdeKind): Promise<{ kind: string; message: string }> {
      const r = await invoke<{ kind: string; message: string }>('restore_auth_backup', { kind })
      await new Promise(res => setTimeout(res, 600))
      await this.refreshAll()
      return r
    },
    async restartIde(kind: IdeKind): Promise<RestartResult> {
      const result = await invoke<RestartResult>('restart_ide', { kind })
      // 退出/启动有延迟，给系统一点时间再刷新运行状态
      await new Promise(r => setTimeout(r, 600))
      await this.fetchStatus()
      return result
    },
    async openExternal(url: string) {
      await invoke('open_external', { url })
    },
    async openUsagePage() {
      await this.openExternal(USAGE_PAGE_URL)
    },
    async copyText(text: string) {
      await invoke('copy_to_clipboard', { text })
    },
    async exportAccounts(includeTokens: boolean): Promise<ExportResult> {
      return await invoke<ExportResult>('export_accounts', { includeTokens })
    },
    async importAccounts(restoreCurrent = true): Promise<ImportResult> {
      const result = await invoke<ImportResult>('import_accounts', { restoreCurrent })
      if (result.imported) await this.refreshAll()
      return result
    },
    async revealPath(path: string) {
      await invoke('reveal_path', { path })
    },
  },
})
