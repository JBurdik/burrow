<template>
  <div class="pair-wrap">
    <div class="pair-logo">
      <span class="pair-mark" aria-hidden="true">B</span>
      <span class="pair-product">Burrow Remote</span>
    </div>

    <form class="pair-form" @submit.prevent="connect">
      <label class="pair-label">Burrow address</label>
      <input
        v-model="urlInput"
        class="m-input"
        type="url"
        placeholder="https://mac-name.tailnet.ts.net"
        autocomplete="url"
        autocorrect="off"
        autocapitalize="none"
        spellcheck="false"
        required
      />

      <label class="pair-label">One-time pairing code</label>
      <input
        v-model="tokenInput"
        class="m-input"
        type="text"
        placeholder="xxxxxx"
        autocomplete="one-time-code"
        autocorrect="off"
        autocapitalize="none"
        spellcheck="false"
        required
        minlength="4"
      />

      <div v-if="err" class="pair-error" role="alert">{{ err }}</div>

      <button class="m-btn" type="submit" :disabled="busy">
        {{ busy ? 'Connecting…' : 'Connect' }}
      </button>
    </form>

    <p class="pair-hint">
      Use the address opened through Tailscale and the six-digit code printed by burrow-web.
    </p>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import { useRouter } from 'vue-router';
import { useRemoteStore } from '../store';

const store  = useRemoteStore();
const router = useRouter();

const urlInput   = ref(store.baseUrl || window.location.origin);
const tokenInput = ref('');
const busy = ref(false);
const err  = ref('');

async function connect() {
  busy.value = true;
  err.value  = '';
  try {
    await store.pair(urlInput.value, tokenInput.value);
    await store.refresh();
    if (store.offline) {
      err.value = store.error ?? 'Connection failed';
      store.unpair();
    } else {
      store.startLive();
      router.push('/home');
    }
  } catch (e: any) {
    err.value = e.message ?? 'Connection failed';
    store.unpair();
  } finally {
    busy.value = false;
  }
}
</script>

<style scoped>
.pair-wrap {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 100dvh;
  padding: 32px 24px calc(var(--safe-bottom) + 32px);
  gap: 0;
}

.pair-logo {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  margin-bottom: 32px;
}
.pair-mark {
  width: 56px;
  height: 56px;
  display: grid;
  place-items: center;
  border: 1px solid var(--border);
  border-radius: 12px;
  background: var(--bg-panel);
  color: var(--accent);
  font-family: var(--font-mono);
  font-size: 22px;
  font-weight: 800;
}
.pair-product {
  font-family: var(--font-mono);
  font-size: 15px;
  font-weight: 700;
  color: var(--text-primary);
  letter-spacing: 0.02em;
}

.pair-form {
  width: 100%;
  max-width: 360px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.pair-label {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--text-muted);
  margin-bottom: -4px;
}

.pair-error {
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--red);
  padding: 8px 10px;
  background: rgba(239, 68, 68, 0.08);
  border: 1px solid rgba(239, 68, 68, 0.3);
  border-radius: 4px;
}

.pair-hint {
  margin-top: 20px;
  font-size: 12px;
  color: var(--text-muted);
  text-align: center;
  max-width: 300px;
  line-height: 1.6;
}
</style>
