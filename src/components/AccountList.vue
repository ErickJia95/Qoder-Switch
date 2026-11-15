<template>
  <div class="flex flex-col h-full">
    <!-- 顶部标题区 -->
    <div class="px-1 pb-2 flex items-center justify-between">
      <div class="flex items-center gap-2">
        <h2 class="text-white font-semibold text-sm">账号列表</h2>
        <span class="text-blue-400 text-xs">{{ accounts.length }}</span>
      </div>
      <span class="text-gray-500 text-xs">「接入」= 写入该账号登录态并重启对应 IDE</span>
    </div>

    <!-- 表格区域 -->
    <div class="flex-1 overflow-auto bg-dark-800 rounded-lg border border-gray-700">
      <table class="w-full text-sm">
        <thead class="sticky top-0 bg-dark-800 border-b border-gray-700">
          <tr class="text-gray-400 text-xs">
            <th class="py-2 px-3 text-left w-10">#</th>
            <th class="py-2 px-3 text-left">名称</th>
            <th class="py-2 px-3 text-left">邮箱</th>
            <th class="py-2 px-3 text-left w-16">IDE</th>
            <th class="py-2 px-3 text-left w-24">类型</th>
            <th class="py-2 px-3 text-left w-28">到期</th>
            <th class="py-2 px-3 text-left w-28">剩余用量</th>
            <th class="py-2 px-3 text-left w-16">状态</th>
            <th class="py-2 px-3 text-right w-52">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="(account, index) in accounts"
            :key="account.id"
            class="border-b border-gray-800 hover:bg-dark-700/50 transition"
          >
            <td class="py-2.5 px-3 text-gray-500">{{ index + 1 }}</td>
            <td class="py-2.5 px-3 text-white font-medium">
              {{ getDisplayName(account) }}
            </td>
            <td class="py-2.5 px-3 text-gray-400">{{ account.email || '-' }}</td>
            <td class="py-2.5 px-3">
              <span v-if="account.ide_kind" class="text-xs px-1.5 py-0.5 rounded bg-gray-700/60 text-gray-300">
                {{ account.ide_kind === 'intl' ? 'Qoder' : 'CN' }}
              </span>
              <span v-else class="text-gray-600">-</span>
            </td>
            <td class="py-2.5 px-3 text-gray-400">{{ account.user_type || '-' }}</td>
            <td class="py-2.5 px-3 text-gray-400">{{ account.expire_date || '-' }}</td>
            <td class="py-2.5 px-3" :class="getBalanceClass(account)">{{ getBalanceText(account) }}</td>
            <td class="py-2.5 px-3">
              <span class="text-xs px-2 py-0.5 rounded" :class="getStatusClass(account)">{{ getStatusText(account) }}</span>
            </td>
            <td class="py-2.5 px-3 text-right">
              <div class="flex items-center justify-end gap-3">
                <button @click="handleSwitch(account)" class="text-blue-400 text-xs hover:text-blue-300">接入</button>
                <button
                  @click="handleCheckQuota(account.id)"
                  :disabled="store.checkingIds.includes(account.id)"
                  class="text-blue-400 text-xs hover:text-blue-300 disabled:text-gray-500 disabled:cursor-wait"
                >{{ store.checkingIds.includes(account.id) ? '查询中…' : '查用量' }}</button>
                <button
                  @click="handleCheckin(account)"
                  :disabled="store.checkingIn"
                  class="text-green-400 text-xs hover:text-green-300 disabled:text-gray-500 disabled:cursor-wait"
                >{{ store.checkingIn ? '签到中…' : '签到' }}</button>
                <button @click="handleCopy(account)" class="text-gray-400 text-xs hover:text-gray-200">复制邮箱</button>
                <button @click="handleRemove(account)" class="text-red-400 text-xs hover:text-red-300">删除</button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>

      <!-- 空状态 -->
      <div v-if="accounts.length === 0" class="flex items-center justify-center h-48 text-gray-500 text-sm">
        暂无账号，请点击上方「读取当前登录账号」导入
      </div>
    </div>

    <SwitchDialog
      :open="dialogOpen"
      :account="activeAccount"
      @close="dialogOpen = false"
      @done="onSwitched"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useAppStore } from '../stores/app'
import { useToast, toErrMsg } from '../composables/useToast'
import SwitchDialog from './SwitchDialog.vue'
import type { AccountToken } from '../types'

const store = useAppStore()
const toast = useToast()
const accounts = computed(() => store.accounts)
const dialogOpen = ref(false)
const activeAccount = ref<AccountToken | null>(null)

function handleSwitch(account: AccountToken) {
  activeAccount.value = account
  dialogOpen.value = true
}

function onSwitched(message: string) {
  toast.success('接入完成', message)
}

async function handleCheckQuota(id: string) {
  try {
    const r = await store.checkQuota(id)
    if (r.error) toast.error('查询用量失败', r.error)
    else store.activeTab = 'usage'
  } catch (e: unknown) {
    toast.error('查询用量失败', toErrMsg(e))
  }
}

async function handleCopy(account: AccountToken) {
  await store.copyText(account.email || account.label)
}

async function handleCheckin(account: AccountToken) {
  try {
    const r = await store.checkinOne(account.id)
    if (r.status === 'claimed') {
      toast.success(`${r.label} 领取成功 +${r.amount} Credits`, r.expires_at ? `有效期至 ${r.expires_at}` : undefined)
    } else if (r.status === 'already') {
      toast.info(`${r.label} 今日已领取`)
    } else if (r.status === 'no_campaign') {
      toast.warning(`${r.label} 无可用签到活动`, r.message)
    } else {
      toast.error(`${r.label} 签到失败`, r.message)
    }
  } catch (e: unknown) {
    toast.error(`${account.label} 签到失败`, toErrMsg(e))
  }
}

async function handleRemove(account: AccountToken) {
  if (!confirm(`确定从账号库删除「${account.label}」？`)) return
  try {
    await store.removeAccount(account.id)
    toast.success('已删除账号', account.label)
  } catch (e: unknown) {
    toast.error('删除账号失败', toErrMsg(e))
  }
}

function getStatusText(account: AccountToken) {
  return account.status === 'current' ? '当前' : '可用'
}

function getStatusClass(account: AccountToken) {
  return account.status === 'current'
    ? 'bg-green-600/20 text-green-400'
    : 'bg-blue-600/20 text-blue-400'
}

function getBalanceText(account: AccountToken): string {
  if (account.quota_used == null && account.quota_total == null) return '—'
  if (!account.quota_total) return '—'
  const remaining = Math.max((account.quota_total ?? 0) - (account.quota_used ?? 0), 0)
  return `${remaining} / ${account.quota_total}`
}

function getBalanceClass(account: AccountToken): string {
  if (account.quota_used == null || account.quota_total == null || !account.quota_total) return 'text-gray-500'
  const remaining = account.quota_total - account.quota_used
  if (remaining <= 0) return 'text-red-400'
  if (remaining < account.quota_total * 0.2) return 'text-yellow-400'
  return 'text-green-400'
}

function getDisplayName(account: AccountToken): string {
  if (account.name) return account.name
  if (account.label) {
    const match = account.label.match(/^(.+?)\s*\(.*\)$/)
    return match ? match[1] : account.label
  }
  return '-'
}
</script>
