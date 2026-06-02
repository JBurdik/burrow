import { defineStore } from "pinia";
import { ref } from "vue";

export interface Toast {
  id: number;
  title: string;
  body?: string;
  type: "done" | "info" | "error";
}

let nextId = 0;

export const useNotificationsStore = defineStore("notifications", () => {
  const toasts = ref<Toast[]>([]);

  function push(toast: Omit<Toast, "id">): number {
    const id = ++nextId;
    toasts.value.push({ ...toast, id });
    setTimeout(() => dismiss(id), 5000);
    return id;
  }

  function dismiss(id: number) {
    const idx = toasts.value.findIndex((t) => t.id === id);
    if (idx !== -1) toasts.value.splice(idx, 1);
  }

  return { toasts, push, dismiss };
});
