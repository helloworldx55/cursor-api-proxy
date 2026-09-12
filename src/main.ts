import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type BridgeStatusView = {
  running: boolean;
  bound_port: number | null;
  preferred_port: number;
  error: string | null;
};

const healthEl = () => document.querySelector("#health");
const errorEl = () => document.querySelector<HTMLElement>("#error");
const boundEl = () => document.querySelector("#bound-port");
const preferredEl = () =>
  document.querySelector<HTMLInputElement>("#preferred-port");
const startBtn = () => document.querySelector<HTMLButtonElement>("#start");
const stopBtn = () => document.querySelector<HTMLButtonElement>("#stop");
const copyBtn = () => document.querySelector<HTMLButtonElement>("#copy-bound");

function render(status: BridgeStatusView) {
  const health = healthEl();
  const error = errorEl();
  const bound = boundEl();
  const preferred = preferredEl();
  if (health) {
    health.textContent = status.running
      ? `运行中 · 127.0.0.1:${status.bound_port}`
      : "未启动";
  }
  if (bound) {
    bound.textContent = status.bound_port ? String(status.bound_port) : "—";
  }
  if (preferred && document.activeElement !== preferred) {
    preferred.value = String(status.preferred_port);
  }
  if (error) {
    if (status.error) {
      error.hidden = false;
      error.textContent = status.error;
    } else {
      error.hidden = true;
      error.textContent = "";
    }
  }
  const start = startBtn();
  const stop = stopBtn();
  const copy = copyBtn();
  if (start) start.disabled = status.running;
  if (stop) stop.disabled = !status.running;
  if (copy) copy.disabled = !status.bound_port;
}

async function refresh() {
  const status = await invoke<BridgeStatusView>("bridge_status");
  render(status);
}

async function start() {
  const preferred = Number(preferredEl()?.value || 8765);
  try {
    const status = await invoke<BridgeStatusView>("start_bridge", {
      preferredPort: preferred,
    });
    render(status);
  } catch (err) {
    render({
      running: false,
      bound_port: null,
      preferred_port: preferred,
      error: String(err),
    });
  }
}

async function stop() {
  const status = await invoke<BridgeStatusView>("stop_bridge");
  render(status);
}

window.addEventListener("DOMContentLoaded", async () => {
  startBtn()?.addEventListener("click", () => {
    void start();
  });
  stopBtn()?.addEventListener("click", () => {
    void stop();
  });
  copyBtn()?.addEventListener("click", async () => {
    const port = boundEl()?.textContent?.trim();
    if (port && port !== "—") {
      await navigator.clipboard.writeText(port);
    }
  });
  await listen<BridgeStatusView>("bridge-status", (event) => {
    render(event.payload);
  });
  await refresh();
});
