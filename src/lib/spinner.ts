import { ref } from "vue";

const FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
export const spinnerFrame = ref(FRAMES[0]);

let idx = 0;
setInterval(() => {
  spinnerFrame.value = FRAMES[++idx % FRAMES.length];
}, 80);
