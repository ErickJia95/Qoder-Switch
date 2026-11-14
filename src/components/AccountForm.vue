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

        <span class="w-px h-6 bg-gray-700 mx-1"></span>

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
        <span v-if="includeTokens" class="text-yellow-500/80">导出文件含账号凭证，请妥善保管。</span>
        <span class="text-gray-500">换电脑时用「从 JSON 导入」即可还原账号库。</span>
      </span>
    </div>

    <div class="text-xs text-right leading-5 shrink-0">
      <div v-if="lastImport" class="text-green-400">
        已{{ lastImport.updated ? '刷新' : '添加' }}：{{ lastImport.label }}
        <div v-if="lastImport.expires_at" class="text-gray-500">凭证有效期至 {{ lastImport.expires_at.slice(0, 10) }}</div>
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
import type { LocalAccountInfo, ExportResult, ImportResult } from '../types'

const store = useAppStore()
const lastImport = ref<LocalAccountInfo | null>(null)
const lastExport = ref<ExportResult | null>(null)
const lastImportFile = ref<ImportResult | null>(null)
const includeTokens = ref(true)
const restoreCurrent = ref(true)
const exporting = ref(false)
const importing = ref(false)

async function handleImportLocal() {
  if (store.importing) return
  store.importing = true
  try {
    const info = await store.importLocalAccount()
    lastImport.value = info
    alert(`${info.updated ? '已刷新' : '已添加'}账号：${info.label}`)
  } catch (e: any) {
    alert('读取失败: ' + (e?.message || e))
  } finally {
    store.importing = false
  }
}

async function handleExport() {
  if (exporting.value) return
  if (includeTokens.value && !confirm('备份文件将包含账号凭证（Token），可用于恢复登录。\n请妥善保管该文件，确定导出？')) return
  exporting.value = true
  try {
    const r = await store.exportAccounts(includeTokens.value)
    if (!r.saved) return
    lastExport.value = r
    alert(`已导出 ${r.count} 个账号\n\n${r.path}`)
  } catch (e: any) {
    const msg = typeof e === 'string' ? e : (e?.message || JSON.stringify(e))
    alert('导出失败: ' + msg)
  } finally {
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
    const lines = [
      `已从备份导入 ${r.total} 个账号`,
      `新增 ${r.added} 个 · 更新 ${r.updated} 个${r.skipped ? ` · 跳过 ${r.skipped} 个（缺凭证）` : ''}`,
    ]
    if (r.restored_current) lines.push(`当前账号：${r.restored_current}`)
    alert(lines.join('\n'))
  } catch (e: any) {
    const msg = typeof e === 'string' ? e : (e?.message || JSON.stringify(e))
    alert('导入失败: ' + msg)
  } finally {
    importing.value = false
  }
}

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`
  return `${(bytes / 1024).toFixed(1)} KB`
}
</script>
