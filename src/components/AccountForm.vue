<template>
  <div class="bg-dark-800 border-b border-gray-700 p-4 flex items-start gap-4">
    <div class="flex-1 flex flex-col gap-2">
      <div class="flex items-center gap-3 flex-wrap">
        <button
          @click="handleImportLocal"
          :disabled="store.importing"
          class="px-4 py-2 bg-accent text-white rounded text-sm font-medium hover:bg-accent-hover whitespace-nowrap disabled:opacity-50"
        >
          {{ store.importing ? '读取中…' : '读取当前登录账号' }}
        </button>
        <button
          @click="store.checkAllQuotas()"
          :disabled="store.checkingAll || !store.hasAccounts"
          class="px-4 py-2 bg-dark-700 text-gray-200 rounded text-sm hover:bg-dark-600 whitespace-nowrap disabled:opacity-50"
        >
          {{ store.checkingAll ? '查询中…' : '查询全部用量' }}
        </button>
        <button
          @click="store.openUsagePage()"
          class="px-4 py-2 bg-dark-700 text-gray-200 rounded text-sm hover:bg-dark-600 whitespace-nowrap"
        >
          打开 qoder.cn 用量页
        </button>

        <button
          @click="handleRefreshAll"
          :disabled="store.refreshingAll || !store.hasAccounts"
          class="px-4 py-2 bg-dark-700 text-gray-200 rounded text-sm hover:bg-dark-600 whitespace-nowrap disabled:opacity-50"
          title="用 refreshToken 给所有账号换新凭证，避免逐个重新登录 IDE"
        >
          {{ store.refreshingAll ? `刷新凭证中… ${store.refreshDone}/${store.accounts.length}` : '刷新全部凭证' }}
        </button>

        <span class="w-px h-6 bg-gray-700 mx-1"></span>

        <button
          @click="handleCheckinAll"
          :disabled="store.checkingIn || !store.hasAccounts"
          class="px-4 py-2 bg-green-600 text-white rounded text-sm font-medium hover:bg-green-500 whitespace-nowrap disabled:opacity-50"
        >
          {{ store.checkingIn ? `签到中… ${store.checkinDone}/${store.accounts.length}` : '一键领取签到' }}
        </button>

        <button
          @click="handleExport"
          :disabled="exporting || !store.hasAccounts"
          class="px-4 py-2 bg-dark-700 text-gray-200 rounded text-sm hover:bg-dark-600 whitespace-nowrap disabled:opacity-50"
        >
          {{ exporting ? '导出中…' : '导出备份 (JSON)' }}
        </button>
        <label class="flex items-center gap-1.5 text-xs text-gray-400 select-none cursor-pointer">
          <input v-model="includeTokens" type="checkbox" class="accent-blue-500" />
          含凭证 Token
        </label>

        <button
          @click="handleImport"
          :disabled="importing"
          class="px-4 py-2 bg-dark-700 text-gray-200 rounded text-sm hover:bg-dark-600 whitespace-nowrap disabled:opacity-50"
        >
          {{ importing ? '导入中…' : '从 JSON 导入' }}
        </button>
        <label class="flex items-center gap-1.5 text-xs text-gray-400 select-none cursor-pointer">
          <input v-model="restoreCurrent" type="checkbox" class="accent-blue-500" />
          恢复当前标记
        </label>
      </div>
      <span class="text-xs text-gray-400">
        在 Qoder IDE 里登录要收集的账号，再点「读取当前登录账号」，即可把该账号凭证存入本地账号库。
        <span v-if="includeTokens" class="text-yellow-500/80">导出文件含账号凭证，请妥善保管；文件将直接保存到 桌面/下载 目录。</span>
        <span v-else class="text-gray-500">文件将直接保存到 桌面/下载 目录。</span>
        <span class="text-gray-500">换电脑时用「从 JSON 导入」即可还原账号库。</span>
      </span>
    </div>

    <div class="text-xs text-right leading-5 shrink-0">
      <div v-if="lastImport" class="text-green-400">
        已{{ lastImport.updated ? '刷新' : '添加' }}：{{ lastImport.label }}
        <div v-if="lastImport.expires_at" class="text-gray-500">凭证有效期至 {{ lastImport.expires_at.slice(0, 10) }}</div>
      </div>
      <div v-if="lastCheckin" class="text-green-400">
        签到完成：<span class="text-green-300">成功 {{ lastCheckin.claimed }} 个</span><template v-if="lastCheckin.already">，已领过 {{ lastCheckin.already }} 个</template><template v-if="lastCheckin.noCampaign">，无活动 {{ lastCheckin.noCampaign }} 个</template><template v-if="lastCheckin.failed">，<span class="text-red-400">失败 {{ lastCheckin.failed }} 个</span></template>
        <div v-if="lastCheckin.totalCredits" class="text-gray-500">本次共领取 {{ lastCheckin.totalCredits }} Credits</div>
      </div>
      <div v-if="lastExport?.path" class="text-blue-400">
        已导出 {{ lastExport.count }} 个账号（{{ formatSize(lastExport.bytes) }}）
        <div class="flex items-center justify-end gap-2 mt-0.5">
          <button @click="store.revealPath(lastExport.path!)" class="text-blue-400 hover:text-blue-300 underline">打开所在文件夹</button>
          <button @click="store.copyText(lastExport.path!)" class="text-gray-400 hover:text-gray-200 underline">复制路径</button>
        </div>
      </div>
      <div v-if="lastImportFile" class="text-green-400">
        已导入 {{ lastImportFile.total }} 个账号（新增 {{ lastImportFile.added }} / 更新 {{ lastImportFile.updated }}<template v-if="lastImportFile.skipped"> / 跳过 {{ lastImportFile.skipped }}</template>）
        <div v-if="lastImportFile.restored_current" class="text-gray-500">当前账号：{{ lastImportFile.restored_current }}</div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useAppStore } from '../stores/app'
import { useToast, toErrMsg } from '../composables/useToast'
import type { LocalAccountInfo, ExportResult, ImportResult, CheckinResult } from '../types'

const store = useAppStore()
const toast = useToast()
const lastImport = ref<LocalAccountInfo | null>(null)
const lastExport = ref<ExportResult | null>(null)
const lastImportFile = ref<ImportResult | null>(null)
const lastCheckin = ref<{ claimed: number; already: number; noCampaign: number; failed: number; totalCredits: number; details: string } | null>(null)
const includeTokens = ref(true)
const restoreCurrent = ref(true)
const exporting = ref(false)
const importing = ref(false)

const CHECKIN_STATUS_TEXT: Record<CheckinResult['status'], string> = {
  claimed: '✅',
  already: '☑️',
  no_campaign: '⚠️',
  error: '❌',
}

function summarizeCheckin(results: CheckinResult[]) {
  const claimed = results.filter(r => r.status === 'claimed')
  const already = results.filter(r => r.status === 'already').length
  const noCampaign = results.filter(r => r.status === 'no_campaign').length
  const failed = results.filter(r => r.status === 'error').length
  const totalCredits = claimed.reduce((sum, r) => sum + (r.amount || 0), 0)
  const details = results
    .map(r => {
      const extra =
        r.status === 'claimed'
          ? ` +${r.amount} Credits${r.expires_at ? `（有效期至 ${r.expires_at}）` : ''}`
          : ''
      const msg = ['already', 'no_campaign', 'error'].includes(r.status) ? ` ${r.message}` : ''
      return `${CHECKIN_STATUS_TEXT[r.status] || '·'} ${r.label}${extra}${msg}`
    })
    .join('\n')
  return { claimed: claimed.length, already, noCampaign, failed, totalCredits, details }
}

async function handleCheckinAll() {
  if (store.checkingIn) return
  try {
    const results = await store.checkinAll()
    if (!results.length) {
      toast.warning('没有可签到的账号', '请先在「账号切换」里导入账号')
      return
    }
    lastCheckin.value = summarizeCheckin(results)
    const s = lastCheckin.value

    // 整体结果 toast
    if (s.claimed > 0) {
      toast.success(
        `签到完成：成功 ${s.claimed} 个，共领取 ${s.totalCredits} Credits`,
        `已领过 ${s.already} · 无活动 ${s.noCampaign} · 失败 ${s.failed}`
      )
    } else if (s.failed > 0) {
      toast.error(`签到完成：${s.failed} 个账号失败`, s.details)
    } else {
      toast.info(
        `签到完成：本次无可领取（已领过 ${s.already} · 无活动 ${s.noCampaign}）`,
        s.already + s.noCampaign > 0 ? undefined : '所有账号均已签到'
      )
    }

    // 逐账号失败/异常原因单独弹 toast，方便定位
    for (const r of results) {
      if (r.status === 'error') {
        toast.error(`${r.label} 签到失败`, r.message)
      } else if (r.status === 'no_campaign') {
        toast.warning(`${r.label} 无可用签到活动`, r.message)
      }
    }
  } catch (e: unknown) {
    toast.error('一键签到失败', toErrMsg(e))
  }
}

async function handleRefreshAll() {
  if (store.refreshingAll) return
  try {
    const results = await store.refreshAllTokens(false)
    if (!results.length) {
      toast.warning('没有可刷新的账号', '请先在「账号切换」里导入账号')
      return
    }
    const refreshed = results.filter(r => r.status === 'refreshed')
    const skipped = results.filter(r => r.status === 'skipped').length
    const failed = results.filter(r => r.status === 'failed')

    if (refreshed.length > 0) {
      toast.success(
        `凭证刷新完成：${refreshed.length} 个已续期`,
        `无需刷新 ${skipped} 个 · 失败 ${failed.length} 个`
      )
    } else if (failed.length > 0) {
      toast.error(`凭证刷新完成：${failed.length} 个失败`, failed.map(r => `${r.label}：${r.message}`).join('\n'))
    } else {
      toast.info('凭证刷新完成：所有账号凭证仍有效', `无需刷新 ${skipped} 个`)
    }
    // 逐账号失败原因单独提示，方便定位
    for (const r of failed) {
      toast.error(`${r.label} 刷新失败`, r.message)
    }
  } catch (e: unknown) {
    console.error('[refresh] 批量刷新凭证失败', e)
    toast.error('刷新全部凭证失败', toErrMsg(e))
  }
}

async function handleImportLocal() {
  if (store.importing) return
  store.importing = true
  try {
    const info = await store.importLocalAccount()
    lastImport.value = info
    toast.success(`${info.updated ? '已刷新' : '已添加'}账号`, info.label)
  } catch (e: unknown) {
    toast.error('读取当前登录账号失败', toErrMsg(e))
  } finally {
    store.importing = false
  }
}

async function handleExport() {
  if (exporting.value) return
  // 是否含凭证由「含凭证 Token」勾选项显式决定（勾选时界面已有黄色警示文案），
  // 不再用原生 confirm() —— WKWebView 下原生弹窗不可靠，会静默中断导出流程
  exporting.value = true
  let timeoutId: number | undefined
  try {
    // 兜底保护：后端异常挂起时 20 秒后强制结束，避免按钮永久卡在「导出中…」
    const r = await Promise.race([
      store.exportAccounts(includeTokens.value),
      new Promise<never>((_, reject) => {
        timeoutId = window.setTimeout(() => reject(new Error('导出超时（20 秒），请重试或查看日志')), 20000)
      }),
    ])
    if (!r.saved) return
    lastExport.value = r
    toast.success(`已导出 ${r.count} 个账号（${formatSize(r.bytes)}）`, r.path || undefined)
  } catch (e: unknown) {
    console.error('[export] 导出备份失败', e)
    toast.error('导出备份失败', toErrMsg(e))
  } finally {
    if (timeoutId !== undefined) window.clearTimeout(timeoutId)
    exporting.value = false
  }
}

async function handleImport() {
  if (importing.value) return
  importing.value = true
  try {
    const r = await store.importAccounts(restoreCurrent.value)
    if (!r.imported) return
    lastImportFile.value = r
    const summary = `新增 ${r.added} · 更新 ${r.updated}${r.skipped ? ` · 跳过 ${r.skipped}（缺凭证）` : ''}${r.restored_current ? ` · 当前账号：${r.restored_current}` : ''}`
    toast.success(`已从备份导入 ${r.total} 个账号`, summary)
  } catch (e: unknown) {
    toast.error('导入备份失败', toErrMsg(e))
  } finally {
    importing.value = false
  }
}

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`
  return `${(bytes / 1024).toFixed(1)} KB`
}
</script>
