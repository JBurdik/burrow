import { createApp } from "vue";
import { createPinia } from "pinia";

const pinia = createPinia();

async function boot() {
  // In Tauri, window label is synchronously accessible via internals
  const label: string = (window as any).__TAURI_INTERNALS__?.metadata?.currentWindow?.label ?? "";
  const isFloat = label.startsWith("float-");
  const isGitPanel = label === "gitpanel";

  if (isGitPanel) {
    document.getElementById("app")!.style.height = "100vh";
    const { default: GitPanel } = await import("./components/GitPanel.vue");
    const app = createApp(GitPanel);
    app.use(pinia);
    app.mount("#app");
  } else if (isFloat) {
    // ptyId is always derivable from the label (float-{ptyId})
    const ptyId = Number(label.replace("float-", "")) || 0;
    let wsId = 0;
    let initTitle = `PTY ${ptyId}`;

    // Try to get full params from Rust state
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const params = await invoke<{ pty_id: number; ws_id: number; title: string } | null>(
        "get_float_params", { label },
      );
      if (params) {
        wsId = params.ws_id;
        initTitle = params.title || initTitle;
      }
    } catch { /* params optional */ }

    const { default: FloatBubble } = await import("./components/FloatBubble.vue");
    const app = createApp(FloatBubble, { ptyId, wsId, initTitle });
    app.use(pinia);
    app.mount("#app");
  } else {
    const { default: App } = await import("./App.vue");
    const app = createApp(App);
    app.use(pinia);
    app.mount("#app");
  }
}

boot();
