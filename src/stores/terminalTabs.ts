import { defineStore } from "pinia";
import { ref } from "vue";

// Lightweight mirror of each workspace's terminal tabs so the Sidebar can render
// them nested under its project. The Terminal component remains the source of
// truth (it owns the split trees / PTYs); it pushes summaries here and listens
// for activate/add/close requests coming back from the sidebar.
export interface TabSummary {
  id: number;
  title: string;
  isAgent: boolean;
  busy: boolean;
  status: "idle" | "running" | "waiting" | "done" | "review";
  leafCount?: number;
}

type TabRequest = {
  wsId: number;
  action: "activate" | "add" | "close" | "reorder";
  tabId?: number;
  fromIdx?: number;
  toIdx?: number;
  nonce: number;
};

export const useTerminalTabsStore = defineStore("terminalTabs", () => {
  const tabsByWs = ref<Record<number, TabSummary[]>>({});
  const activeByWs = ref<Record<number, number>>({});
  const request = ref<TabRequest | null>(null);
  let nonce = 0;

  function setTabs(wsId: number, tabs: TabSummary[]) {
    tabsByWs.value[wsId] = tabs;
  }
  function setActive(wsId: number, tabId: number) {
    activeByWs.value[wsId] = tabId;
  }
  function clear(wsId: number) {
    delete tabsByWs.value[wsId];
    delete activeByWs.value[wsId];
  }

  function activate(wsId: number, tabId: number) {
    request.value = { wsId, action: "activate", tabId, nonce: ++nonce };
  }
  function add(wsId: number) {
    request.value = { wsId, action: "add", nonce: ++nonce };
  }
  function close(wsId: number, tabId: number) {
    request.value = { wsId, action: "close", tabId, nonce: ++nonce };
  }
  function reorder(wsId: number, fromIdx: number, toIdx: number) {
    request.value = { wsId, action: "reorder", fromIdx, toIdx, nonce: ++nonce };
  }

  return { tabsByWs, activeByWs, request, setTabs, setActive, clear, activate, add, close, reorder };
});
