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

    <div class="flex items-center gap-6">
      <!-- IDE 状态 -->
      <div class="flex items-center gap-2">
        <span class="w-2.5 h-2.5 rounded-full" :class="store.status?.ide_running ? 'bg-green-500' : 'bg-gray-500'"></span>
        <span class="text-gray-400 text-xs">Qoder IDE</span>
        <span class="text-xs" :class="store.status?.ide_running ? 'text-green-400' : 'text-gray-500'">
          {{ store.status?.ide_running ? '运行中' : '未运行' }}
        </span>
      </div>
      <!-- 当前账号 -->
      <div class="flex items-center gap-2">
        <span class="text-gray-400 text-xs">当前账号</span>
        <span class="text-xs text-blue-400">{{ store.status?.current_label || '未选择' }}</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { useAppStore } from '../stores/app'

const store = useAppStore()
const appVersion = ref('2.0.0')

onMounted(async () => {
  try {
    appVersion.value = await getVersion()
  } catch {}
})

async function checkUpdate() {
  alert(`当前版本 v${appVersion.value}，自动更新将在后续版本启用`)
}
</script>
