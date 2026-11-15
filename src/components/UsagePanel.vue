<template>
  <div class="flex flex-col h-full">
    <div class="px-1 pb-2 flex items-center justify-between">
      <div class="flex items-center gap-2">
        <h2 class="text-white font-semibold text-sm">用量查询</h2>
        <span class="text-gray-500 text-xs">数据来自 qoder.cn 账号用量接口</span>
      </div>
      <div class="flex items-center gap-3">
        <span v-if="updatedAt" class="text-gray-500 text-xs">更新于 {{ updatedAt }}</span>
        <button
          @click="refresh"
          :disabled="store.checkingAll"
          class="px-3 py-1.5 bg-dark-700 hover:bg-dark-600 text-gray-300 text-xs rounded disabled:opacity-50"
        >
          {{ store.checkingAll ? '查询中…' : '刷新全部' }}
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-auto">
      <div v-if="quotas.length" class="grid grid-cols-1 xl:grid-cols-2 gap-4">
        <div
          v-for="item in quotas"
          :key="item.account_id"
          class="bg-dark-800 rounded-lg p-4 border"
          :class="cardBorder(item)"
        >
          <div class="flex items-start justify-between mb-2">
            <div>
              <div class="text-white font-medium text-sm">{{ item.label }}</div>
              <div class="text-xs text-gray-500 mt-0.5">
                {{ item.plan_name || '未知套餐' }}<template v-if="item.user_type"> · {{ item.user_type }}</template>
              </div>
            </div>
            <span class="px-2 py-0.5 rounded text-xs" :class="statusBadge(item)">{{ statusText(item) }}</span>
          </div>

          <template v-if="!item.error">
            <div class="mt-3">
              <div class="flex items-end justify-between mb-1">
                <span class="text-xs text-gray-400">已用 {{ fmt(item.quota_used) }} / {{ fmt(item.quota_total) }} {{ item.quota_unit }}</span>
                <span class="text-xs" :class="quotaColor(item)">{{ percent(item) }}%</span>
              </div>
              <div class="h-2 bg-dark-900 rounded overflow-hidden">
                <div class="h-full rounded transition-all" :class="barColor(item)" :style="{ width: percent(item) + '%' }"></div>
              </div>
            </div>

            <div class="mt-3 flex items-center justify-between text-xs text-gray-400">
              <span>剩余 <span :class="quotaColor(item)">{{ fmt(item.quota_remaining) }} {{ item.quota_unit }}</span></span>
              <span v-if="item.expire_date">到期 {{ item.expire_date }}</span>
            </div>
          </template>

          <div v-else class="mt-3 text-xs text-red-400 break-all">{{ item.error }}</div>

          <div class="mt-2 text-[10px] text-gray-600 truncate">{{ item.api_base || '接口未返回' }}</div>
        </div>
      </div>

      <div v-else class="flex flex-col items-center justify-center h-48 text-gray-500 text-sm gap-2">
        <span>还没有用量数据</span>
        <button
          @click="refresh"
          :disabled="store.checkingAll || !store.hasAccounts"
          class="px-3 py-1.5 bg-dark-700 hover:bg-dark-600 text-gray-300 text-xs rounded disabled:opacity-50"
        >{{ store.hasAccounts ? '查询全部账号用量' : '请先在「账号切换」里导入账号' }}</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useAppStore } from '../stores/app'
import { useToast, toErrMsg } from '../composables/useToast'
import type { QuotaResult } from '../types'

const store = useAppStore()
const toast = useToast()
const updatedAt = ref('')

const quotas = computed(() => store.quotas)

async function refresh() {
  try {
    await store.checkAllQuotas()
    updatedAt.value = new Date().toLocaleTimeString()
  } catch (e: unknown) {
    toast.error('查询用量失败', toErrMsg(e))
  }
}

function percent(item: QuotaResult): number {
  if (item.error) return 0
  if (item.usage_percentage > 0) return Math.min(Math.round(item.usage_percentage), 100)
  if (item.quota_total > 0) return Math.min(Math.round((item.quota_used / item.quota_total) * 100), 100)
  return 0
}

function cardBorder(item: QuotaResult) {
  if (item.error) return 'border-red-500/50'
  if (item.is_exceeded) return 'border-red-500/50'
  if (percent(item) >= 80) return 'border-orange-500/50'
  return 'border-gray-700'
}

function statusBadge(item: QuotaResult) {
  if (item.error) return 'bg-red-500/20 text-red-400'
  if (item.is_exceeded) return 'bg-red-500/20 text-red-400'
  if (percent(item) >= 80) return 'bg-orange-500/20 text-orange-400'
  return 'bg-green-500/20 text-green-400'
}

function statusText(item: QuotaResult) {
  if (item.error) return '查询失败'
  if (item.is_exceeded) return '已耗尽'
  if (percent(item) >= 80) return '额度紧张'
  return '正常'
}

function quotaColor(item: QuotaResult) {
  if (item.is_exceeded) return 'text-red-400 font-medium'
  if (percent(item) >= 80) return 'text-orange-400 font-medium'
  return 'text-green-400'
}

function barColor(item: QuotaResult) {
  if (item.is_exceeded) return 'bg-red-500'
  if (percent(item) >= 80) return 'bg-orange-500'
  return 'bg-green-500'
}

function fmt(value: number): string {
  if (value >= 100000) return (value / 1000).toFixed(1) + 'k'
  if (Number.isInteger(value)) return String(value)
  return value.toFixed(1)
}
</script>
