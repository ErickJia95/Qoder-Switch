import { reactive } from 'vue'

export type ToastType = 'success' | 'error' | 'info' | 'warning'

export interface ToastItem {
  id: number
  type: ToastType
  title: string
  message?: string
}

const state = reactive<{ toasts: ToastItem[] }>({ toasts: [] })
let nextId = 1

function push(type: ToastType, title: string, message?: string) {
  const id = nextId++
  state.toasts.push({ id, type, title, message })
  // 失败/警告停留更久，方便看原因
  const duration = type === 'error' ? 8000 : type === 'warning' ? 6000 : 4000
  window.setTimeout(() => dismiss(id), duration)
}

export function dismiss(id: number) {
  const idx = state.toasts.findIndex(t => t.id === id)
  if (idx >= 0) state.toasts.splice(idx, 1)
}

/** 把任意异常转成可读字符串 */
export function toErrMsg(e: unknown): string {
  if (typeof e === 'string') return e
  if (e instanceof Error) return e.message
  try {
    return JSON.stringify(e)
  } catch {
    return String(e)
  }
}

export function useToast() {
  return {
    toasts: state.toasts,
    dismiss,
    success: (title: string, message?: string) => push('success', title, message),
    error: (title: string, message?: string) => push('error', title, message),
    info: (title: string, message?: string) => push('info', title, message),
    warning: (title: string, message?: string) => push('warning', title, message),
  }
}
