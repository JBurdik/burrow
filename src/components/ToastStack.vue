<template>
  <Teleport to="body">
    <div class="toast-stack" :class="`toast-stack--${ui.toastPosition}`">
      <TransitionGroup name="toast">
        <div
          v-for="toast in store.toasts"
          :key="toast.id"
          class="toast"
          :class="`toast-${toast.type}`"
          @click="store.dismiss(toast.id)"
        >
          <span class="toast-dot" />
          <div class="toast-text">
            <div class="toast-title">{{ toast.title }}</div>
            <div v-if="toast.body" class="toast-body">{{ toast.body }}</div>
          </div>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { useNotificationsStore } from "@/stores/notifications";
import { useUIStore } from "@/stores/ui";
const store = useNotificationsStore();
const ui = useUIStore();
</script>

<style scoped>
.toast-stack {
  position: fixed;
  z-index: 9999;
  display: flex;
  flex-direction: column;
  gap: 8px;
  pointer-events: none;
}

/* Vertical anchor */
.toast-stack--top-left,
.toast-stack--top-center,
.toast-stack--top-right    { top: 20px; }
.toast-stack--bottom-left,
.toast-stack--bottom-center,
.toast-stack--bottom-right { bottom: 20px; }

/* Horizontal anchor */
.toast-stack--top-left,
.toast-stack--bottom-left    { left: 20px; align-items: flex-start; }
.toast-stack--top-right,
.toast-stack--bottom-right   { right: 20px; align-items: flex-end; }
.toast-stack--top-center,
.toast-stack--bottom-center  { left: 50%; transform: translateX(-50%); align-items: center; }

.toast {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px 14px;
  border-radius: 8px;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.4);
  min-width: 220px;
  max-width: 320px;
  cursor: pointer;
  pointer-events: all;
  backdrop-filter: blur(8px);
}

.toast-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
  margin-top: 4px;
}
.toast-done .toast-dot  { background: #84cc16; }
.toast-info .toast-dot  { background: #3b82f6; }
.toast-error .toast-dot { background: #ef4444; }

.toast-text { display: flex; flex-direction: column; gap: 2px; min-width: 0; }

.toast-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.toast-body {
  font-size: 11px;
  color: var(--text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.toast-enter-active { transition: all 0.2s ease; }
.toast-leave-active { transition: all 0.18s ease; }
.toast-enter-from   { opacity: 0; transform: translateY(12px); }
/* Slide out toward the nearest screen edge per anchor */
.toast-stack--bottom-left   .toast-leave-to,
.toast-stack--top-left      .toast-leave-to   { opacity: 0; transform: translateX(-24px); }
.toast-stack--bottom-right  .toast-leave-to,
.toast-stack--top-right     .toast-leave-to   { opacity: 0; transform: translateX(24px); }
.toast-stack--bottom-center .toast-leave-to   { opacity: 0; transform: translateY(24px); }
.toast-stack--top-center    .toast-leave-to   { opacity: 0; transform: translateY(-24px); }
</style>
