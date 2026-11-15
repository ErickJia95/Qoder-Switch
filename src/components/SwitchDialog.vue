<template>
  <div
    v-if="open"
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    @click.self="$emit('close')"
  >
    <div class="w-[460px] bg-dark-800 border border-gray-700 rounded-xl p-5 shadow-2xl">
      <div class="flex items-start justify-between mb-1">
        <h3 class="text-white font-semibold text-sm">接入账号</h3>
        <button class="text-gray-500 hover:text-gray-300 leading-none" @click="$emit('close')">✕</button>
      </div>
      <p class="text-xs text-gray-400 mb-4">
        目标账号：<span class="text-blue-400">{{ account?.label || '-' }}</span>
        <span v-if="account?.email" class="text-gray-500"> · {{ account.email }}</span>
      </p>

      <div class="flex flex-col gap-2">
        <button
          v-for="opt in options"
          :key="opt.key"
          :disabled="busy !== null || opt.disabled"
          class="text-left px-3 py-2.5 rounded-lg border transition disabled:opacity-40"
          :class="opt.primary
            ? 'border-blue-600/60 bg-blue-600/10 hover:bg-blue-600/20'
            : 'border-gray-700 bg-dark-700/60 hover:bg-dark-700'"
          @click="run(opt)"
        >
          <div class="text-sm text-white flex items-center gap-2">
            {{ opt.title }}
            <span v-if="busy === opt.key" class="text-xs text-blue-400">处理中，稍等…</span>
          </div>
          <div class="text-xs text-gray-500 mt-0.5">{{ opt.desc }}</div>
        </button>
      </div>

      <p v-if="account && !account.can_activate" class="text-xs text-yellow-500/80 mt-3">
        该账号没有保存登录数据，无法自动切换。请先在 IDE 里登录它，再点顶部「读取当前登录账号」补齐。
      </p>
      <p v-else-if="account && !account.has_full_credentials" class="text-xs text-yellow-500/80 mt-3">
        该账号只有旧版 token（缺 refreshToken）。实测写入这种数据 IDE 会判定无效并掉登录，
        需先在 IDE 里登录该账号后点「读取当前登录账号」补齐完整登录数据。
      </p>
      <p v-else-if="!ideInstalled" class="text-xs text-yellow-500/80 mt-3">
        未检测到 {{ ideName }}，无法切换。请先安装该版本 Qoder。
      </p>
      <p v-else class="text-[11px] text-gray-500 mt-3 leading-4">
        切换会退出 {{ ideName }}，把该账号的登录态写入 IDE 后再重新打开；原登录态已备份到
        <code class="text-gray-400">~/Library/Application Support/Qoder/auth-backup-{{ account?.ide_kind || 'cn' }}.dat</code>。
        未保存的编辑器内容可能丢失。
      </p>

      <div class="flex items-center justify-between mt-3">
        <button
          v-if="ideInstalled"
          class="text-[11px] text-gray-500 hover:text-gray-300 underline disabled:opacity-40"
          :disabled="busy !== null"
          @click="restoreBackup"
        >恢复切换前的登录态</button>
        <span v-else></span>
        <button class="text-xs text-gray-400 hover:text-gray-200" @click="$emit('close')">取消</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useAppStore } from '../stores/app'
import { useToast, toErrMsg } from '../composables/useToast'
import type { AccountToken } from '../types'

const props = defineProps<{ open: boolean; account: AccountToken | null }>()
const emit = defineEmits<{ close: []; done: [message: string] }>()

const store = useAppStore()
const toast = useToast()
const busy = ref<string | null>(null)

const ideName = computed(() => (props.account?.ide_kind === 'intl' ? 'Qoder' : 'Qoder CN'))
const ideInstalled = computed(() =>
  props.account?.ide_kind === 'intl'
    ? !!store.status?.ide_intl_installed
    : !!store.status?.ide_cn_installed
)

interface Option {
  key: string
  title: string
  desc: string
  primary: boolean
  disabled?: boolean
}

const options = computed<Option[]>(() => [
  {
    key: 'activate',
    title: `切换到该账号并重启 ${ideName.value}`,
    desc: '写入该账号登录态后重开 IDE，打开即是这个账号',
    primary: true,
    disabled: !ideInstalled.value || !props.account?.can_activate,
  },
  {
    key: 'mark',
    title: '仅标记为当前账号',
    desc: '不改 IDE 登录态，稍后自己手动退出并登录',
    primary: false,
  },
])

async function restoreBackup() {
  if (busy.value) return
  const kind = props.account?.ide_kind === 'intl' ? 'intl' : 'cn'
  if (!confirm(`将把 ${ideName.value} 的登录态恢复为上次切换前保存的版本并重启，确定继续？`)) return
  busy.value = 'restore'
  try {
    const r = await store.restoreAuthBackup(kind)
    toast.success('恢复完成', r.message)
    emit('done', r.message)
    emit('close')
  } catch (e: unknown) {
    toast.error('恢复登录态失败', toErrMsg(e))
  } finally {
    busy.value = null
  }
}

async function run(opt: Option) {
  if (!props.account || busy.value || opt.disabled) return
  busy.value = opt.key
  try {
    if (opt.key === 'activate') {
      const r = await store.activateAccount(props.account.id)
      let msg = r.message
      if (r.verified) msg += ` 已回读确认：IDE 当前登录为 ${r.email || r.label}。`
      if (!r.full_credentials) {
        msg += ' 该账号目前只有旧版 token（无刷新凭证）。若 IDE 提示重新登录，登录一次后点「读取当前登录账号」即可补全。'
      }
      toast.success('接入完成', msg)
      emit('done', msg)
    } else {
      const info = await store.switchAccount(props.account.id)
      const msg = `已将「${info.label}」标记为当前账号（未改动 IDE 登录态）`
      toast.success('已标记当前账号', info.label)
      emit('done', msg)
    }
    emit('close')
  } catch (e: unknown) {
    toast.error('接入失败', toErrMsg(e))
  } finally {
    busy.value = null
  }
}
</script>
