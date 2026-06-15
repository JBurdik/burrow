<template>
  <div class="browser-pane">
    <div class="browser-toolbar">
      <button class="browser-btn" @click="refresh" title="Refresh">
        <PhArrowClockwise :size="13" />
      </button>
      <input
        v-model="inputUrl"
        class="browser-url"
        spellcheck="false"
        placeholder="Enter URL or localhost:3000…"
        @keydown.enter="navigate"
        @focus="($event.target as HTMLInputElement).select()"
      />
      <button class="browser-btn" @click="openExternal" title="Open in system browser">
        <PhArrowSquareOut :size="13" />
      </button>
    </div>
    <div class="browser-frame-wrap">
      <iframe
        v-if="committedUrl"
        ref="iframeEl"
        class="browser-frame"
        :src="committedUrl"
        allow="clipboard-read; clipboard-write"
        @load="onLoad"
        @error="onError"
      />
      <div v-else class="browser-empty">
        <PhGlobe :size="36" class="browser-empty-icon" />
        <p>Enter a URL above to browse</p>
        <p class="browser-empty-hint">Works best with localhost dev servers.<br />External sites may block embedding.</p>
      </div>
      <div v-if="blocked" class="browser-blocked">
        <PhProhibit :size="28" class="browser-empty-icon" />
        <p>This site blocked embedding (X-Frame-Options).</p>
        <button class="browser-ext-btn" @click="openExternal">Open in system browser</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from "vue";
import { PhArrowClockwise, PhArrowSquareOut, PhGlobe, PhProhibit } from "@phosphor-icons/vue";
import { open as shellOpen } from "@tauri-apps/plugin-shell";

const props = defineProps<{
  initialUrl?: string;
}>();

const inputUrl = ref(props.initialUrl ?? "");
const committedUrl = ref(props.initialUrl ?? "");
const blocked = ref(false);
const iframeEl = ref<HTMLIFrameElement | null>(null);

watch(() => props.initialUrl, (u) => {
  if (u) {
    inputUrl.value = u;
    go(u);
  }
});

function normalizeUrl(raw: string): string {
  const s = raw.trim();
  if (!s) return s;
  if (/^https?:\/\//i.test(s)) return s;
  // bare host or localhost:port — assume http
  if (/^localhost(:\d+)?/i.test(s) || /^\d{1,3}(\.\d{1,3}){3}(:\d+)?/.test(s)) {
    return `http://${s}`;
  }
  return `https://${s}`;
}

function go(url: string) {
  blocked.value = false;
  committedUrl.value = normalizeUrl(url);
  inputUrl.value = committedUrl.value;
}

function navigate() {
  go(inputUrl.value);
}

function refresh() {
  if (iframeEl.value?.contentWindow) {
    iframeEl.value.contentWindow.location.reload();
  } else if (committedUrl.value) {
    const u = committedUrl.value;
    committedUrl.value = "";
    setTimeout(() => { committedUrl.value = u; }, 0);
  }
}

function openExternal() {
  const url = committedUrl.value || normalizeUrl(inputUrl.value);
  if (url) shellOpen(url);
}

function onLoad() {
  // Try to detect X-Frame-Options block: cross-origin iframes still fire load
  // but contentDocument will be null or inaccessible.
  try {
    const doc = iframeEl.value?.contentDocument;
    if (doc === null) blocked.value = true;
  } catch {
    blocked.value = true;
  }
}

function onError() {
  blocked.value = true;
}
</script>

<style scoped>
.browser-pane {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  background: var(--bg-panel, #1a1a1a);
  overflow: hidden;
}

.browser-toolbar {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 6px;
  background: var(--bg-panel, #1a1a1a);
  border-bottom: 1px solid var(--border, #333);
  flex-shrink: 0;
}

.browser-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  background: none;
  border: none;
  color: var(--text-muted, #888);
  cursor: pointer;
  padding: 3px 5px;
  border-radius: 4px;
  transition: color .1s, background .1s;
  flex-shrink: 0;
}
.browser-btn:hover {
  background: var(--bg-hover, #2a2a2a);
  color: var(--text, #ddd);
}

.browser-url {
  flex: 1;
  background: var(--bg-input, #111);
  border: 1px solid var(--border, #333);
  border-radius: 5px;
  color: var(--text, #ddd);
  font-size: 12px;
  font-family: var(--font-ui, sans-serif);
  padding: 3px 8px;
  outline: none;
  min-width: 0;
}
.browser-url:focus {
  border-color: var(--accent, #555);
}

.browser-frame-wrap {
  flex: 1;
  position: relative;
  overflow: hidden;
}

.browser-frame {
  width: 100%;
  height: 100%;
  border: none;
  display: block;
  background: #fff;
}

.browser-empty,
.browser-blocked {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  color: var(--text-muted, #888);
  font-size: 13px;
  text-align: center;
  padding: 24px;
}

.browser-empty-icon {
  opacity: 0.35;
  margin-bottom: 4px;
}

.browser-empty-hint {
  font-size: 11px;
  opacity: 0.6;
  line-height: 1.5;
}

.browser-blocked {
  background: var(--bg-panel, #1a1a1a);
}

.browser-ext-btn {
  margin-top: 6px;
  background: var(--accent, #444);
  border: 1px solid var(--border, #555);
  border-radius: 5px;
  color: var(--text, #ddd);
  cursor: pointer;
  font-size: 12px;
  padding: 5px 14px;
  transition: background .1s;
}
.browser-ext-btn:hover {
  background: var(--accent-hover, #555);
}
</style>
