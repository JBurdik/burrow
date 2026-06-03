<template>
  <Transition name="update-pop">
    <div v-if="show" class="update-card" :class="{ 'is-busy': u.downloading }">
      <div class="update-icon">
        <PhArrowCircleUp v-if="!u.installed" :size="20" weight="fill" />
        <PhCheckCircle v-else :size="20" weight="fill" />
      </div>

      <div class="update-body">
        <!-- installed → awaiting relaunch -->
        <template v-if="u.installed">
          <div class="update-title">Update ready</div>
          <div class="update-sub">Restart Burrow to finish updating to v{{ u.newVersion }}.</div>
        </template>

        <!-- downloading -->
        <template v-else-if="u.downloading">
          <div class="update-title">Downloading v{{ u.newVersion }}…</div>
          <div class="update-bar">
            <div
              class="update-bar-fill"
              :class="{ indeterminate: u.progress < 0 }"
              :style="u.progress >= 0 ? { width: Math.round(u.progress * 100) + '%' } : {}"
            />
          </div>
        </template>

        <!-- available -->
        <template v-else>
          <div class="update-title">Update available</div>
          <div class="update-sub">
            v{{ u.newVersion }} is ready to install
            <span class="update-cur">(you have v{{ u.currentVersion }})</span>
          </div>
          <div v-if="u.notes" class="update-notes">{{ u.notes }}</div>
        </template>
      </div>

      <div class="update-actions">
        <template v-if="u.installed">
          <button class="update-btn primary" @click="u.relaunch()">Restart</button>
        </template>
        <template v-else-if="!u.downloading">
          <button class="update-btn primary" @click="u.downloadAndInstall()">Install</button>
          <button class="update-btn ghost" @click="u.dismiss()">Later</button>
        </template>
      </div>
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { PhArrowCircleUp, PhCheckCircle } from "@phosphor-icons/vue";
import { useUpdateStore } from "@/stores/update";

const u = useUpdateStore();
// Stays mounted while installed (restart prompt) even though banner was dismissible.
const show = computed(() => u.bannerVisible || u.downloading || u.installed);
</script>

<style scoped>
.update-card {
  position: fixed;
  right: 16px;
  bottom: 16px;
  z-index: 9998;
  width: 320px;
  display: flex;
  gap: 11px;
  padding: 13px 14px;
  border-radius: 12px;
  background: var(--bg-panel, #111);
  border: 1px solid var(--border, #2a2a2a);
  box-shadow: 0 16px 48px rgba(0, 0, 0, 0.55);
}
.update-icon {
  color: var(--accent, #3b82f6);
  flex-shrink: 0;
  margin-top: 1px;
}
.update-card.is-busy .update-icon { color: var(--text-secondary, #94a3b8); }
.update-body { flex: 1; min-width: 0; }
.update-title {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-primary, #e2e8f0);
}
.update-sub {
  font-size: 11.5px;
  color: var(--text-secondary, #94a3b8);
  margin-top: 2px;
}
.update-cur { color: var(--text-muted, #64748b); }
.update-notes {
  font-size: 11px;
  color: var(--text-muted, #64748b);
  margin-top: 6px;
  max-height: 64px;
  overflow-y: auto;
  white-space: pre-wrap;
  line-height: 1.4;
}
.update-actions {
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex-shrink: 0;
  align-self: center;
}
.update-btn {
  font-size: 11.5px;
  font-weight: 600;
  padding: 5px 12px;
  border-radius: 7px;
  border: 1px solid transparent;
  cursor: pointer;
  white-space: nowrap;
}
.update-btn.primary {
  background: var(--accent, #3b82f6);
  color: #fff;
}
.update-btn.primary:hover { filter: brightness(1.08); }
.update-btn.ghost {
  background: transparent;
  border-color: var(--border, #2a2a2a);
  color: var(--text-secondary, #94a3b8);
}
.update-btn.ghost:hover { background: var(--bg-hover, #1a1a1a); }

/* progress bar */
.update-bar {
  margin-top: 8px;
  height: 4px;
  border-radius: 3px;
  background: var(--bg-hover, #1a1a1a);
  overflow: hidden;
}
.update-bar-fill {
  height: 100%;
  background: var(--accent, #3b82f6);
  border-radius: 3px;
  transition: width 0.15s ease;
}
.update-bar-fill.indeterminate {
  width: 35%;
  animation: update-slide 1.1s ease-in-out infinite;
}
@keyframes update-slide {
  0% { margin-left: -35%; }
  100% { margin-left: 100%; }
}

.update-pop-enter-active,
.update-pop-leave-active { transition: all 0.22s cubic-bezier(0.16, 1, 0.3, 1); }
.update-pop-enter-from,
.update-pop-leave-to { opacity: 0; transform: translateY(12px) scale(0.97); }
</style>
