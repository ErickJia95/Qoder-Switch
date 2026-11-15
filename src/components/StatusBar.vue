<template>
  <div class="bg-dark-800 border-b border-gray-700 px-4 py-2.5 flex items-center justify-between">
    <div class="flex items-center gap-3">
      <div class="w-7 h-7 bg-blue-600 rounded-lg flex items-center justify-center">
        <span class="text-white font-bold text-sm">Q</span>
      </div>
      <span class="text-white font-semibold text-sm">Qoder Switch</span>
      <span class="bg-gray-700 text-gray-300 text-xs px-2 py-0.5 rounded">v{{ appVersion }}</span>
      <button @click="checkUpdate" class="text-blue-400 text-xs hover:text-blue-300 transition">检测更新</button>
    </div>

    <div class="flex items-center gap-5">
      <!-- 两个发行版本各自的运行状态 + 快捷重启 -->
      <div v-for="ide in ides" :key="ide.key" class="flex items-center gap-1.5">
        <span class="text-gray-400 text-xs">{{ ide.name }}</span>
        <span class="w-2 h-2 rounded-full" :class="ide.running ? 'bg-green-500' : 'bg-gray-600'"></span>
        <span class="text-xs" :class="ide.running ? 'text-green-400' : 'text-gray-500'">
          {{ ide.installed ? (ide.running ? '运行中' : '未运行') : '未安装' }}
        </span>
        <button
          v-if="ide.installed"
          @click="restart(ide.key, ide.name, ide.running)"
          :disabled="busy !== null"
          class="text-blue-400 text-xs hover:text-blue-300 disabled:opacity-40 ml-0.5"
        >{{ busy === ide.key ? '处理中…' : (ide.running ? '重启' : '启动') }}</button>
      </div>

      <!-- 当前账号 -->
      <div class="flex items-center gap-2">
        <span class="text-gray-400 text-xs">当前账号</span>
        <span class="text-xs text-blue-400 max-w-[150px] truncate">{{ store.status?.current_label || '未选择' }}</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { useAppStore } from '../stores/app'
import { useToast, toErrMsg } from '../composables/useToast'
import type { IdeKind } from '../types'

const store = useAppStore()
const toast = useToast()
const appVersion = ref('2.4.0')
const busy = ref<IdeKind | null>(null)

const ides = computed(() => [
  {
    key: 'cn' as IdeKind,
    name: 'Qoder CN',
    installed: !!store.status?.ide_cn_installed,
    running: !!store.status?.ide_cn_running,
  },
  {
    key: 'intl' as IdeKind,
    name: 'Qoder',
    installed: !!store.status?.ide_intl_installed,
    running: !!store.status?.ide_intl_running,
  },
])

onMounted(async () => {
  try {
    appVersion.value = await getVersion()
  } catch {}
})

async function restart(kind: IdeKind, name: string, running: boolean) {
  if (busy.value) return
  const tip = running
    ? `将退出 ${name} 并重新打开。\n未保存的编辑器内容可能丢失，确定继续？`
    : `将启动 ${name}，确定继续？`
  if (!confirm(tip)) return
  busy.value = kind
  try {
    const r = await store.restartIde(kind)
    if (!r.launched) toast.warning(`${name} 未能启动`, r.message)
    else toast.success(`${name} 已${running ? '重启' : '启动'}`)
  } catch (e: unknown) {
    toast.error(`${name} 操作失败`, toErrMsg(e))
  } finally {
    busy.value = null
  }
}

async function checkUpdate() {
  toast.info(`当前版本 v${appVersion.value}`, '自动更新将在后续版本启用')
}
</script>
