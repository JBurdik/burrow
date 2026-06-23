import { ref, computed, onMounted, onBeforeUnmount } from "vue";

export const AUTO_REFRESH_INTERVALS = [5, 10, 30, 60, 0] as const; // 0 = off

export function useAutoRefresh(callback: () => void, storageKey = "burrow-git-refresh-interval") {
  const stored = parseInt(localStorage.getItem(storageKey) ?? "30");
  const currentInterval = ref(AUTO_REFRESH_INTERVALS.includes(stored as any) ? stored : 30);
  const nextRefreshIn = ref(0);

  const isRunning = computed(() => currentInterval.value > 0);

  let tickId: number | undefined;

  function stop() {
    if (tickId !== undefined) { clearInterval(tickId); tickId = undefined; }
    nextRefreshIn.value = 0;
  }

  function start() {
    stop();
    if (currentInterval.value === 0) return;
    nextRefreshIn.value = currentInterval.value;
    tickId = window.setInterval(() => {
      nextRefreshIn.value--;
      if (nextRefreshIn.value <= 0) {
        callback();
        nextRefreshIn.value = currentInterval.value;
      }
    }, 1000);
  }

  function setRefreshInterval(n: number) {
    currentInterval.value = n;
    localStorage.setItem(storageKey, String(n));
    start();
  }

  function toggle() {
    const idx = AUTO_REFRESH_INTERVALS.indexOf(currentInterval.value as any);
    const next = AUTO_REFRESH_INTERVALS[(idx + 1) % AUTO_REFRESH_INTERVALS.length];
    setRefreshInterval(next);
  }

  onMounted(start);
  onBeforeUnmount(stop);

  return { currentInterval, isRunning, nextRefreshIn, setRefreshInterval, toggle };
}
