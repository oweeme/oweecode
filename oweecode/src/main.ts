import { createApp } from "vue";
import App from "./App.vue";

// A failure while starting must show up on screen instead of leaving a black window.
function showStartupError(e: unknown) {
  const box = document.getElementById("app") ?? document.body;
  box.innerHTML = `<pre style="margin:0;padding:16px;color:#f85149;background:#141416;font:12px monospace;white-space:pre-wrap;height:100vh;overflow:auto">OweeCode could not start:\n\n${String((e as Error)?.stack ?? e)}</pre>`;
}

try {
  const app = createApp(App);
  app.config.errorHandler = (err) => { console.error(err); };
  app.mount("#app");
} catch (e) { showStartupError(e); }
window.addEventListener("error", (ev) => { if (document.getElementById("owee-splash")) showStartupError(ev.error ?? ev.message); });
