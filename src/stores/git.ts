import { defineStore } from "pinia";
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

export interface GitFile {
  path: string;
  x: string;
  y: string;
}

interface GitOutput {
  stdout: string;
  stderr: string;
  code: number;
}

async function runGit(cwd: string, args: string[]): Promise<string> {
  const out = await invoke<GitOutput>("run_git", { cwd, args });
  if (out.code !== 0) throw new Error(out.stderr || "git error");
  return out.stdout;
}

function parseStatus(raw: string): { staged: GitFile[]; unstaged: GitFile[]; untracked: GitFile[] } {
  const staged: GitFile[] = [];
  const unstaged: GitFile[] = [];
  const untracked: GitFile[] = [];

  for (const line of raw.split("\n")) {
    if (line.length < 3) continue;
    const x = line[0];
    const y = line[1];
    const rawPath = line.slice(3);
    const path = rawPath.includes(" -> ") ? rawPath.split(" -> ")[1] : rawPath;
    const file: GitFile = { path, x, y };

    if (x === "?" && y === "?") {
      untracked.push(file);
    } else {
      if (x !== " " && x !== "?") staged.push(file);
      if (y !== " " && y !== "?") unstaged.push(file);
    }
  }

  return { staged, unstaged, untracked };
}

export const useGitStore = defineStore("git", () => {
  const cwd = ref("");
  const branch = ref("");
  const staged = ref<GitFile[]>([]);
  const unstaged = ref<GitFile[]>([]);
  const untracked = ref<GitFile[]>([]);
  const diff = ref("");
  const diffFile = ref<string | null>(null);
  const diffStaged = ref(false);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const commitMsg = ref("");

  async function refresh() {
    if (!cwd.value) return;
    loading.value = true;
    error.value = null;
    try {
      const [statusOut, branchOut] = await Promise.all([
        runGit(cwd.value, ["status", "--porcelain"]),
        runGit(cwd.value, ["branch", "--show-current"]),
      ]);
      const parsed = parseStatus(statusOut);
      staged.value = parsed.staged;
      unstaged.value = parsed.unstaged;
      untracked.value = parsed.untracked;
      branch.value = branchOut.trim();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : "git error";
      staged.value = [];
      unstaged.value = [];
      untracked.value = [];
      branch.value = "";
    } finally {
      loading.value = false;
    }
  }

  function setCwd(path: string) {
    if (path === cwd.value) return;
    cwd.value = path;
    diff.value = "";
    diffFile.value = null;
    commitMsg.value = "";
    refresh();
  }

  async function stageFile(path: string) {
    await runGit(cwd.value, ["add", "--", path]);
    await refresh();
  }

  async function unstageFile(path: string) {
    await runGit(cwd.value, ["reset", "HEAD", "--", path]);
    await refresh();
  }

  async function stageAll() {
    await runGit(cwd.value, ["add", "-A"]);
    await refresh();
  }

  async function commit() {
    if (!commitMsg.value.trim()) return;
    await runGit(cwd.value, ["commit", "-m", commitMsg.value.trim()]);
    commitMsg.value = "";
    diff.value = "";
    diffFile.value = null;
    await refresh();
  }

  async function showDiff(path: string, isStagedFile: boolean) {
    diffFile.value = path;
    diffStaged.value = isStagedFile;
    try {
      const args = isStagedFile
        ? ["diff", "--cached", "--", path]
        : ["diff", "--", path];
      diff.value = await runGit(cwd.value, args);
    } catch {
      diff.value = "";
    }
  }

  function clearDiff() {
    diff.value = "";
    diffFile.value = null;
  }

  async function fetchAllDiff(staged: boolean): Promise<string> {
    const args = staged ? ["diff", "--cached"] : ["diff"];
    try {
      return await runGit(cwd.value, args);
    } catch {
      return "";
    }
  }

  async function gitInit() {
    if (!cwd.value) return;
    loading.value = true;
    error.value = null;
    try {
      await invoke<GitOutput>("run_git", { cwd: cwd.value, args: ["init"] });
      await refresh();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : "git init failed";
    } finally {
      loading.value = false;
    }
  }

  return {
    cwd, branch, staged, unstaged, untracked,
    diff, diffFile, diffStaged,
    loading, error, commitMsg,
    setCwd, refresh, stageFile, unstageFile, stageAll, commit, showDiff, clearDiff, fetchAllDiff, gitInit,
  };
});
