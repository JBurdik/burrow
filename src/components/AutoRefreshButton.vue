<template>
  <div class="ar-wrap" ref="wrapRef">
    <button
      class="ar-btn"
      :class="{ active: isRunning }"
      :title="isRunning ? `Auto-refresh every ${currentInterval}s (right-click to change)` : 'Auto-refresh off (right-click to change)'"
      @click="toggle()"
      @contextmenu.prevent="menuOpen = !menuOpen"
    >
      <PhArrowClockwise :size="13" :class="{ spin: isRunning && nextRefreshIn <= 1 }" />
      <span class="ar-label">{{ isRunning ? `${nextRefreshIn}s` : "Off" }}</span>
    </button>

    <Transition name="ar-menu">
      <div v-if="menuOpen" class="ar-menu">
        <button
          v-for="n in AUTO_REFRESH_INTERVALS"
          :key="n"
          class="ar-menu-item"
          :class="{ selected: currentInterval === n }"
          @click="pick(n)"
        >
          {{ n === 0 ? "Off" : `${n}s` }}
        </button>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from "vue";
import { PhArrowClockwise } from "@phosphor-icons/vue";
import { AUTO_REFRESH_INTERVALS } from "@/composables/useAutoRefresh";

const props = defineProps<{
  currentInterval: number;
  isRunning: boolean;
  nextRefreshIn: number;
  toggle: () => void;
  setRefreshInterval: (n: number) => void;
}>();

const menuOpen = ref(false);
const wrapRef = ref<HTMLElement | null>(null);

function pick(n: number) {
  props.setRefreshInterval(n);
  menuOpen.value = false;
}

function onOutsideClick(e: MouseEvent) {
  if (wrapRef.value && !wrapRef.value.contains(e.target as Node)) {
    menuOpen.value = false;
  }
}

onMounted(() => document.addEventListener("mousedown", onOutsideClick));
onBeforeUnmount(() => document.removeEventListener("mousedown", onOutsideClick));
</script>

<style scoped>
.ar-wrap {
  position: relative;
  display: flex;
}

.ar-btn {
  display: flex;
  align-items: center;
  gap: 3px;
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  padding: 3px 4px;
  border-radius: 3px;
  font-size: 10px;
  font-family: var(--font-ui);
  transition: color 0.1s, background 0.1s;
}
.ar-btn:hover { color: var(--text-primary); background: var(--bg-hover); }
.ar-btn.active { color: var(--accent); }

.ar-label { min-width: 20px; text-align: left; }

@keyframes spin { to { transform: rotate(360deg); } }
.spin { animation: spin 0.6s linear infinite; }

.ar-menu {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 5px;
  padding: 3px;
  display: flex;
  flex-direction: column;
  gap: 1px;
  z-index: 200;
  box-shadow: 0 4px 12px rgba(0,0,0,0.25);
  min-width: 56px;
}

.ar-menu-item {
  background: none;
  border: none;
  border-radius: 3px;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 11px;
  font-family: var(--font-ui);
  padding: 3px 8px;
  text-align: left;
  transition: background 0.08s, color 0.08s;
}
.ar-menu-item:hover { background: var(--bg-hover); color: var(--text-primary); }
.ar-menu-item.selected { color: var(--accent); font-weight: 600; }

.ar-menu-enter-active, .ar-menu-leave-active { transition: opacity 0.1s, transform 0.1s; }
.ar-menu-enter-from, .ar-menu-leave-to { opacity: 0; transform: translateY(-4px); }
</style>
