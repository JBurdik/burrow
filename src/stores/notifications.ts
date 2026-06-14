import { defineStore } from "pinia";
import { ref } from "vue";

export interface Toast {
  id: number;
  title: string;
  body?: string;
  type: "done" | "info" | "error";
  workspaceId?: number;
  tabId?: number;
}

export interface HistoryItem extends Toast {
  ts: number;
}

let nextId = 0;

export const useNotificationsStore = defineStore("notifications", () => {
  const toasts = ref<Toast[]>([]);
  const history = ref<HistoryItem[]>([]);
  const unreadCount = ref(0);

  function push(toast: Omit<Toast, "id">): number {
    const id = ++nextId;
    toasts.value.push({ ...toast, id });
    setTimeout(() => dismiss(id), 5000);
    history.value.unshift({ ...toast, id, ts: Date.now() });
    if (history.value.length > 50) history.value.pop();
    unreadCount.value++;
    return id;
  }

  function dismiss(id: number) {
    const idx = toasts.value.findIndex((t) => t.id === id);
    if (idx !== -1) toasts.value.splice(idx, 1);
  }

  function markAllRead() {
    unreadCount.value = 0;
  }

  function clearHistory() {
    history.value = [];
    unreadCount.value = 0;
  }

  return { toasts, history, unreadCount, push, dismiss, markAllRead, clearHistory };
});
