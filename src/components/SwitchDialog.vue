<template>
  <div
    v-if="open"
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    @click.self="$emit('close')"
  >
    <div class="w-[440px] bg-dark-800 border border-gray-700 rounded-xl p-5 shadow-2xl">
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
          :disabled="busy !== null"
          class="text-left px-3 py-2.5 rounded-lg border transition disabled:opacity-50"
          :class="opt.primary
            ? 'border-blue-600/60 bg-blue-600/10 hover:bg-blue-600/20'
            : 'border-gray-700 bg-dark-700/60 hover:bg-dark-700'"
          @click="run(opt)"
        >
          <div class="text-sm text-white flex items-center gap-2">
            {{ opt.title }}
            <span v-if="busy === opt.key" class="text-xs text-blue-400">处理中…</span>
          </div>
          <div class="text-xs text-gray-500 mt-0.5">{{ opt.desc }}</div>
        </button>
      </div>

      <p v-if="!hasAnyIde" class="text-xs text-yellow-500/80 mt-3">
        未检测到已安装的 Qoder，只能标记账号；装好 IDE 后再回来重启。
      </p>
      <p class="text-[11px] text-gray-500 mt-3 leading-4">
        重启会先让 IDE 退出（未保存的内容可能丢失），再自动打开；打开后请用
        {{ account?.email || '该账号' }} 登录，再回到这里点「读取当前登录账号」刷新凭证。
      </p>

      <div class="flex justify-end mt-3">
        <button class="text-xs text-gray-400 hover:text-gray-200" @click="$emit('close')">取消</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useAppStore } from '../stores/app'
import type { AccountToken, IdeKind } from '../types'

const props = defineProps<{ open: boolean; account: AccountToken | null }>()
const emit = defineEmits<{ close: []; done: [message: string] }>()

const store = useAppStore()
const busy = ref<string | null>(null)

interface Option {
  key: string
  title: string
  desc: string
  kind: IdeKind | null
  primary: boolean
}

const options = computed<Option[]>(() => {
  const s = store.status
  const list: Option[] = []

  if (s?.ide_cn_installed) {
    list.push({
      key: 'cn',
      title: s.ide_cn_running ? '重启 Qoder CN' : '启动 Qoder CN',
      desc: s.ide_cn_running ? '退出 Qoder CN 后重新打开' : 'Qoder CN 当前未运行，直接打开',
      kind: 'cn',
      primary: true,
    })
  }
  if (s?.ide_intl_installed) {
    list.push({
      key: 'intl',
      title: s.ide_intl_running ? '重启 Qoder' : '启动 Qoder',
      desc: s.ide_intl_running ? '退出 Qoder 后重新打开' : 'Qoder 当前未运行，直接打开',
      kind: 'intl',
      primary: !list.length,
    })
  }
  list.push({
    key: 'mark',
    title: '仅标记为当前账号',
    desc: '不重启 IDE，稍后自己在 IDE 里退出并登录',
    kind: null,
    primary: !list.length,
  })
  return list
})

const hasAnyIde = computed(() => !!store.status?.ide_cn_installed || !!store.status?.ide_intl_installed)

async function run(opt: Option) {
  if (!props.account || busy.value) return
  busy.value = opt.key
  try {
    const info = await store.switchAccount(props.account.id)
    let message = `已将「${info.label}」标记为当前账号`
    if (opt.kind) {
      const r = await store.restartIde(opt.kind)
      message += r.launched ? `，${r.message}` : `；${r.message}`
    }
    emit('done', message)
    emit('close')
  } catch (e: any) {
    const msg = typeof e === 'string' ? e : (e?.message || JSON.stringify(e))
    alert('接入失败: ' + msg)
  } finally {
    busy.value = null
  }
}
</script>
