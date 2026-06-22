import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { BurrowClient, type Workspace, type Session } from './api';

const URL_KEY = 'burrow-remote-url';
const TOK_KEY = 'burrow-remote-token';

export const useRemoteStore = defineStore('remote', () => {
  const baseUrl = ref(localStorage.getItem(URL_KEY) ?? '');
  const token   = ref(localStorage.getItem(TOK_KEY) ?? '');
  const workspaces = ref<Workspace[]>([]);
  const loading    = ref(false);
  const error      = ref<string | null>(null);
  const offline    = ref(false);

  const paired = computed(() => !!baseUrl.value && !!token.value);

  let client: BurrowClient | null = paired.value
    ? new BurrowClient({ baseUrl: baseUrl.value, token: token.value })
    : null;
  let pollTimer: number | null = null;

  const allSessions = computed<Session[]>(() =>
    workspaces.value.flatMap((w) => w.sessions),
  );

  async function pair(url: string, code: string) {
    stopLive();
    baseUrl.value = url.replace(/\/$/, '');
    const pairingClient = new BurrowClient({ baseUrl: baseUrl.value, token: '' });
    token.value = await pairingClient.pair(code);
    localStorage.setItem(URL_KEY, baseUrl.value);
    localStorage.setItem(TOK_KEY, token.value);
    client = new BurrowClient({ baseUrl: baseUrl.value, token: token.value });
  }

  function unpair() {
    stopLive();
    baseUrl.value = '';
    token.value   = '';
    workspaces.value = [];
    client = null;
    localStorage.removeItem(URL_KEY);
    localStorage.removeItem(TOK_KEY);
  }

  async function refresh() {
    if (!client) return;
    loading.value = true;
    error.value   = null;
    try {
      workspaces.value = await client.listWorkspaces();
      offline.value    = false;
    } catch (e: any) {
      error.value  = e.message ?? 'Connection failed';
      offline.value = true;
    } finally {
      loading.value = false;
    }
  }

  function startLive() {
    stopLive();
    if (!client) return;
    pollTimer = window.setInterval(() => refresh(), 3000);
  }

  function stopLive() {
    if (pollTimer !== null) window.clearInterval(pollTimer);
    pollTimer = null;
  }

  async function getOutput(ptyId: number): Promise<string> {
    if (!client) throw new Error('Not paired');
    return client.getOutput(ptyId);
  }

  async function sendInput(ptyId: number, text: string) {
    if (!client) throw new Error('Not paired');
    await client.sendInput(ptyId, text);
  }

  async function interrupt(ptyId: number) {
    if (!client) throw new Error('Not paired');
    await client.interrupt(ptyId);
  }

  return {
    baseUrl, token, workspaces, allSessions,
    loading, error, offline, paired,
    pair, unpair, refresh, startLive, stopLive, getOutput, sendInput, interrupt,
  };
});
