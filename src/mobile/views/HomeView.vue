<template>
  <!-- auth guard -->
  <template v-if="!store.paired">
    <div class="m-state">
      <span class="m-state-icon">⚡</span>
      <span class="m-state-msg">Not connected.</span>
      <button class="m-btn" style="max-width:200px" @click="router.replace('/')">Pair device</button>
    </div>
  </template>

  <template v-else>
    <header class="m-nav">
      <span class="m-nav-title">Burrow</span>
      <span v-if="store.offline" class="home-offline-badge" aria-label="Offline">offline</span>
      <button class="m-btn-ghost" @click="handleUnpair">Disconnect</button>
    </header>

    <!-- loading -->
    <div v-if="store.loading && !store.workspaces.length" class="m-state">
      <span class="m-state-icon">⏳</span>
      <span class="m-state-msg">Loading sessions…</span>
    </div>

    <!-- error (no data yet) -->
    <div v-else-if="store.offline && !store.workspaces.length" class="m-state">
      <span class="m-state-icon">✕</span>
      <span class="m-state-msg">Cannot reach Burrow desktop.</span>
      <span v-if="store.error" class="m-state-detail">{{ store.error }}</span>
      <button class="m-btn-ghost" style="margin-top:8px" @click="store.refresh()">Retry</button>
    </div>

    <!-- empty -->
    <div v-else-if="!store.workspaces.length" class="m-state">
      <span class="m-state-icon">□</span>
      <span class="m-state-msg">No open workspaces.</span>
    </div>

    <!-- workspace + session list -->
    <div v-else class="m-body">
      <!-- stale-data offline banner -->
      <div v-if="store.offline" class="home-stale-banner" role="status">
        Reconnecting… showing last known state
      </div>

      <div v-for="ws in store.workspaces" :key="ws.id" class="home-ws-group">
        <div class="home-ws-header">
          <span class="home-ws-name">{{ ws.name }}</span>
          <span class="home-ws-path">{{ shortPath(ws.path) }}</span>
        </div>

        <ul class="m-list">
          <li v-if="!ws.sessions.length" class="home-empty-ws">
            No sessions
          </li>
          <li
            v-for="s in ws.sessions"
            :key="s.ptyId"
            class="m-row"
            role="link"
            :aria-label="`${s.title}, status: ${s.status}`"
            @click="router.push(`/output/${s.ptyId}`)"
          >
            <span :class="['s-dot', s.status]" aria-hidden="true" />
            <div class="home-session-info">
              <span class="home-session-title">{{ s.title || `PTY ${s.ptyId}` }}</span>
              <span class="home-session-meta">
                <span class="home-status-label">{{ statusLabel(s.status) }}</span>
                <span v-if="s.statusDetail" class="home-status-detail">{{ s.statusDetail }}</span>
                <span v-if="s.model" class="home-model">{{ s.model }}</span>
              </span>
            </div>
            <span class="home-chevron" aria-hidden="true">›</span>
          </li>
        </ul>
      </div>
    </div>
  </template>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue';
import { useRouter } from 'vue-router';
import { useRemoteStore } from '../store';
import type { SessionStatus } from '../api';

const store  = useRemoteStore();
const router = useRouter();

const STATUS_LABELS: Record<SessionStatus, string> = {
  idle:       'idle',
  running:    'running',
  waiting:    'waiting for input',
  permission: 'needs permission',
  done:       'done',
  review:     'finished',
  error:      'error',
};
function statusLabel(s: SessionStatus) { return STATUS_LABELS[s] ?? s; }

function shortPath(p: string) {
  return p.replace(/^\/Users\/[^/]+/, '~');
}

function handleUnpair() {
  store.stopLive();
  store.unpair();
  router.replace('/');
}

onMounted(() => {
  if (store.paired) {
    store.refresh();
    store.startLive();
  }
});

onUnmounted(() => {
  store.stopLive();
});
</script>

<style scoped>
.home-offline-badge {
  font-family: var(--font-mono);
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--yellow);
  border: 1px solid var(--yellow);
  border-radius: 3px;
  padding: 1px 5px;
  opacity: 0.8;
}

.home-stale-banner {
  font-size: 12px;
  color: var(--yellow);
  background: rgba(234, 179, 8, 0.08);
  border-bottom: 1px solid rgba(234, 179, 8, 0.2);
  padding: 8px 16px;
  text-align: center;
}

.home-ws-group { margin-bottom: 4px; }

.home-ws-header {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 10px 16px 6px;
  background: var(--bg-base);
  position: sticky;
  top: 0;
  z-index: 1;
  border-bottom: 1px solid var(--border);
}
.home-ws-name {
  font-family: var(--font-mono);
  font-size: 12px;
  font-weight: 700;
  color: var(--text-primary);
  text-transform: uppercase;
  letter-spacing: 0.06em;
}
.home-ws-path {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.home-empty-ws {
  padding: 10px 16px;
  font-size: 12px;
  color: var(--text-muted);
  font-style: italic;
}

.home-session-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.home-session-title {
  font-family: var(--font-mono);
  font-size: 13px;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.home-session-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}
.home-status-label {
  font-size: 11px;
  color: var(--text-muted);
}
.home-status-detail {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--red);
}
.home-model {
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--text-muted);
  background: var(--bg-hover);
  border: 1px solid var(--border);
  border-radius: 3px;
  padding: 0 4px;
}

.home-chevron {
  color: var(--text-muted);
  font-size: 18px;
  flex-shrink: 0;
  margin-left: 4px;
}
</style>
