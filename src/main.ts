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
const copyConfigBtn = () =>
  document.querySelector<HTMLButtonElement>("#copy-config");
const rotateBtn = () =>
  document.querySelector<HTMLButtonElement>("#rotate-token");
const saveKeyBtn = () =>
  document.querySelector<HTMLButtonElement>("#save-key");
const cursorApiKeyEl = () =>
  document.querySelector<HTMLInputElement>("#cursor-api-key");
const credentialStatusEl = () =>
  document.querySelector("#credential-status");
const callerConfigEl = () =>
  document.querySelector<HTMLTextAreaElement>("#caller-config");

type CredentialStatusView = {
  cursor_api_key_saved: boolean;
  agent_cli_logged_in: boolean;
};

function renderCredentials(status: CredentialStatusView) {
  const el = credentialStatusEl();
  if (!el) return;
  const key = status.cursor_api_key_saved
    ? "Cursor API Key 已保存在凭据库"
    : "未保存 Cursor API Key";
  const login = status.agent_cli_logged_in
    ? "Agent CLI 已登录，可不贴 Key"
    : "Agent CLI 未登录";
  el.textContent = `${key} · ${login}`;
}

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
  const copy = copyConfigBtn();
  if (start) start.disabled = status.running;
  if (stop) stop.disabled = !status.running;
  if (copy) copy.disabled = !status.bound_port;
  if (!status.bound_port) {
    const area = callerConfigEl();
    if (area) area.value = "";
  }
}

async function refreshCallerConfig(running: boolean) {
  const area = callerConfigEl();
  if (!area) return;
  if (!running) {
    area.value = "";
    return;
  }
  try {
    area.value = await invoke<string>("caller_config");
  } catch {
    area.value = "";
  }
}

async function refreshCredentials() {
  try {
    renderCredentials(await invoke<CredentialStatusView>("credential_status"));
  } catch (err) {
    const el = credentialStatusEl();
    if (el) el.textContent = String(err);
  }
}

async function saveCursorApiKey() {
  const key = cursorApiKeyEl()?.value.trim() ?? "";
  try {
    const status = await invoke<CredentialStatusView>("save_cursor_api_key", {
      key,
    });
    renderCredentials(status);
    const input = cursorApiKeyEl();
    if (input) input.value = "";
  } catch (err) {
    const el = credentialStatusEl();
    if (el) el.textContent = String(err);
  }
}

async function refresh() {
  const status = await invoke<BridgeStatusView>("bridge_status");
  render(status);
  await refreshCallerConfig(status.running);
  await refreshCredentials();
}

async function start() {
  const preferred = Number(preferredEl()?.value || 8765);
  try {
    const status = await invoke<BridgeStatusView>("start_bridge", {
      preferredPort: preferred,
    });
    render(status);
    await refreshCallerConfig(status.running);
  } catch (err) {
    render({
      running: false,
      bound_port: null,
      preferred_port: preferred,
      error: String(err),
    });
    await refreshCallerConfig(false);
  }
}

async function stop() {
  const status = await invoke<BridgeStatusView>("stop_bridge");
  render(status);
  await refreshCallerConfig(status.running);
}

async function rotateToken() {
  try {
    const status = await invoke<BridgeStatusView>("rotate_bridge_token");
    render(status);
    await refreshCallerConfig(status.running);
  } catch (err) {
    const status = await invoke<BridgeStatusView>("bridge_status").catch(
      () => null,
    );
    if (status) {
      render({ ...status, error: String(err) });
      await refreshCallerConfig(status.running);
    }
  }
}

window.addEventListener("DOMContentLoaded", async () => {
  startBtn()?.addEventListener("click", () => {
    void start();
  });
  stopBtn()?.addEventListener("click", () => {
    void stop();
  });
  copyConfigBtn()?.addEventListener("click", async () => {
    const text = callerConfigEl()?.value.trim();
    if (text) {
      await navigator.clipboard.writeText(text);
    }
  });
  rotateBtn()?.addEventListener("click", () => {
    void rotateToken();
  });
  saveKeyBtn()?.addEventListener("click", () => {
    void saveCursorApiKey();
  });
  await listen<BridgeStatusView>("bridge-status", (event) => {
    render(event.payload);
    void refreshCallerConfig(event.payload.running);
    void refreshCredentials();
  });
  await refresh();
});
