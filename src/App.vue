<template>
  <div class="min-h-screen bg-dark-900 flex flex-col">
    <!-- 顶部状态栏 -->
    <StatusBar />

    <!-- 操作栏 -->
    <AccountForm />

    <!-- 主内容区 -->
    <div class="flex flex-1 overflow-hidden">
      <!-- 左侧导航 -->
      <nav class="w-24 bg-dark-800 border-r border-gray-700 flex flex-col items-center py-4 gap-1">
        <NavItem :icon="icons.accounts" label="账号切换" :active="store.activeTab === 'accounts'" @click="store.activeTab = 'accounts'" />
        <NavItem :icon="icons.usage" label="用量查询" :active="store.activeTab === 'usage'" @click="store.activeTab = 'usage'" />
      </nav>

      <!-- 右侧内容 -->
      <main class="flex-1 overflow-auto p-4">
        <AccountList v-if="store.activeTab === 'accounts'" />
        <UsagePanel v-else />
      </main>
    </div>

    <!-- 底部状态栏 -->
    <footer class="h-8 bg-dark-800 border-t border-gray-700 flex items-center px-4 text-xs text-gray-500">
      <span v-if="store.currentAccount" class="flex items-center gap-1">
        <span class="w-2 h-2 rounded-full bg-green-500"></span>
        当前账号: {{ store.currentAccount.label }}
      </span>
      <span v-else>尚未标记当前账号</span>
      <span class="ml-4">共 {{ store.accounts.length }} 个账号</span>
      <button class="ml-auto text-blue-400 hover:text-blue-300" @click="store.openUsagePage()">
        打开 qoder.cn 用量页
      </button>
    </footer>
  </div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import { useAppStore } from './stores/app'
import StatusBar from './components/StatusBar.vue'
import AccountForm from './components/AccountForm.vue'
import AccountList from './components/AccountList.vue'
import UsagePanel from './components/UsagePanel.vue'
import NavItem from './components/NavItem.vue'

const store = useAppStore()

const icons = {
  accounts: '<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>',
  usage: '<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 3v18h18"/><path d="M7 15l4-5 3 3 5-7"/></svg>',
}

let statusTimer: number | undefined

onMounted(async () => {
  await store.refreshAll()
  statusTimer = window.setInterval(() => store.fetchStatus(), 10000)
})

onUnmounted(() => {
  if (statusTimer !== undefined) clearInterval(statusTimer)
})
</script>
