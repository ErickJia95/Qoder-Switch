import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import type { AccountToken, AppStatus, QuotaResult, LocalAccountInfo, SwitchInfo, ExportResult, ImportResult, IdeKind, RestartResult } from '../types'

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
