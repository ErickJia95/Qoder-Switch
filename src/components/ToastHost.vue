<template>
  <div class="fixed top-4 right-4 z-[100] flex flex-col gap-2 w-80 pointer-events-none">
    <TransitionGroup name="toast">
      <div
        v-for="t in toasts"
        :key="t.id"
        class="pointer-events-auto flex items-start gap-2.5 rounded-lg border px-3.5 py-2.5 shadow-lg bg-dark-800"
        :class="borderClass(t.type)"
      >
        <span class="text-sm leading-5 shrink-0">{{ icon(t.type) }}</span>
        <div class="flex-1 min-w-0">
          <div class="text-sm font-medium" :class="textClass(t.type)">{{ t.title }}</div>
          <div
            v-if="t.message"
            class="text-xs text-gray-400 mt-1 whitespace-pre-wrap break-all max-h-40 overflow-auto"
          >{{ t.message }}</div>
        </div>
        <button
          class="text-gray-500 hover:text-gray-300 text-xs leading-5 shrink-0"
          @click="dismiss(t.id)"
        >✕</button>
      </div>
    </TransitionGroup>
  </div>
</template>

<script setup lang="ts">
import { useToast, type ToastType } from '../composables/useToast'

const { toasts, dismiss } = useToast()

function borderClass(type: ToastType) {
  switch (type) {
    case 'success': return 'border-green-600/60'
    case 'error': return 'border-red-600/60'
    case 'warning': return 'border-yellow-600/60'
    default: return 'border-blue-600/60'
  }
}

function textClass(type: ToastType) {
  switch (type) {
    case 'success': return 'text-green-400'
    case 'error': return 'text-red-400'
    case 'warning': return 'text-yellow-400'
    default: return 'text-blue-400'
  }
}

function icon(type: ToastType) {
  switch (type) {
    case 'success': return '✅'
    case 'error': return '❌'
    case 'warning': return '⚠️'
    default: return 'ℹ️'
  }
}
</script>

<style scoped>
.toast-enter-active,
.toast-leave-active {
  transition: all 0.25s ease;
}
.toast-enter-from {
  opacity: 0;
  transform: translateX(24px);
}
.toast-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}
</style>
