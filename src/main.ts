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
const summariesEl = () => document.querySelector("#request-summaries");
const logEl = () => document.querySelector<HTMLTextAreaElement>("#bridge-log");
const clearRecordsBtn = () =>
  document.querySelector<HTMLButtonElement>("#clear-records");
const wizardEl = () => document.querySelector<HTMLElement>("#wizard");
const wizardPrereqsEl = () => document.querySelector("#wizard-prereqs");
const wizardWarningEl = () => document.querySelector("#wizard-warning");
const wizardErrorEl = () => document.querySelector<HTMLElement>("#wizard-error");
const wizardAutostartEl = () =>
  document.querySelector<HTMLInputElement>("#wizard-autostart");
const completeWizardBtn = () =>
  document.querySelector<HTMLButtonElement>("#complete-wizard");
const autostartEl = () => document.querySelector<HTMLInputElement>("#autostart");
const autostartWarningEl = () => document.querySelector("#autostart-warning");

type CredentialStatusView = {
  cursor_api_key_saved: boolean;
  agent_cli_logged_in: boolean;
};

type RequestSummary = {
  time: string;
  method: string;
  status: number;
  remote_addr: string;
  path: string;
};

type SetupStatus = {
  agent_cli_present: boolean;
  has_bridge_token: boolean;
  can_complete: boolean;
  completed: boolean;
  autostart_enabled: boolean;
  move_folder_warning: string;
};

function renderSetup(status: SetupStatus) {
  const wizard = wizardEl();
  if (wizard) wizard.hidden = status.completed;
  const prereqs = wizardPrereqsEl();
  if (prereqs) {
    const cli = status.agent_cli_present
      ? "已检测到 Agent CLI"
      : "未找到 Agent CLI";
    const token = status.has_bridge_token
      ? "已有 Bridge Token"
      : "尚无 Bridge Token（请先启动 Bridge）";
    prereqs.textContent = `${cli} · ${token}`;
  }
  const warn = wizardWarningEl();
  if (warn) warn.textContent = status.move_folder_warning;
  const settingsWarn = autostartWarningEl();
  if (settingsWarn) settingsWarn.textContent = status.move_folder_warning;
  const complete = completeWizardBtn();
  if (complete) complete.disabled = !status.can_complete;
  const autostart = autostartEl();
  if (autostart && document.activeElement !== autostart) {
    autostart.checked = status.autostart_enabled;
  }
  const wizardAuto = wizardAutostartEl();
  if (wizardAuto && !status.completed && document.activeElement !== wizardAuto) {
    if (!wizardAuto.dataset.touched) {
      wizardAuto.checked = true;
    }
  }
}

async function refreshSetup() {
  try {
    renderSetup(await invoke<SetupStatus>("wizard_status"));
    const error = wizardErrorEl();
    if (error) {
      error.hidden = true;
      error.textContent = "";
    }
  } catch (err) {
    const wizard = wizardEl();
    if (wizard) wizard.hidden = false;
    const error = wizardErrorEl();
    if (error) {
      error.hidden = false;
      error.textContent = String(err);
    }
  }
}

async function completeWizard() {
  const enable = wizardAutostartEl()?.checked ?? true;
  try {
    renderSetup(
      await invoke<SetupStatus>("complete_wizard", {
        enableAutostart: enable,
      }),
    );
  } catch (err) {
    const error = wizardErrorEl();
    if (error) {
      error.hidden = false;
      error.textContent = String(err);
    }
    await refreshSetup();
  }
}

async function toggleAutostart() {
  const enabled = autostartEl()?.checked ?? false;
  try {
    renderSetup(await invoke<SetupStatus>("set_autostart", { enabled }));
  } catch (err) {
    const error = errorEl();
    if (error) {
      error.hidden = false;
      error.textContent = String(err);
    }
    await refreshSetup();
  }
}

function formatSummaryTime(value: string) {
  const millis = Number(value);
  if (!Number.isFinite(millis)) return value;
  return new Date(millis).toLocaleString();
}

function renderSummaries(items: RequestSummary[]) {
  const el = summariesEl();
  if (!el) return;
  el.replaceChildren();
  if (items.length === 0) {
    const empty = document.createElement("li");
    empty.textContent = "暂无 Request Summary";
    el.append(empty);
    return;
  }
  for (const item of [...items].reverse()) {
    const li = document.createElement("li");
    li.textContent = `${formatSummaryTime(item.time)} ${item.method} ${item.status} ${item.remote_addr} ${item.path}`;
    el.append(li);
  }
}

async function refreshRecords() {
  try {
    renderSummaries(await invoke<RequestSummary[]>("request_summaries"));
  } catch {
    renderSummaries([]);
  }
  const log = logEl();
  if (log) {
    try {
      log.value = await invoke<string>("bridge_log");
    } catch {
      log.value = "";
    }
  }
}

async function clearRecords() {
  try {
    await invoke("clear_bridge_records");
  } catch (err) {
    const error = errorEl();
    if (error) {
      error.hidden = false;
      error.textContent = String(err);
    }
  }
  await refreshRecords();
}

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
  await refreshRecords();
  await refreshSetup();
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
  await refreshSetup();
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
  clearRecordsBtn()?.addEventListener("click", () => {
    void clearRecords();
  });
  completeWizardBtn()?.addEventListener("click", () => {
    void completeWizard();
  });
  wizardAutostartEl()?.addEventListener("change", () => {
    const box = wizardAutostartEl();
    if (box) box.dataset.touched = "1";
  });
  autostartEl()?.addEventListener("change", () => {
    void toggleAutostart();
  });
  await listen<BridgeStatusView>("bridge-status", (event) => {
    render(event.payload);
    void refreshCallerConfig(event.payload.running);
    void refreshCredentials();
    void refreshRecords();
    void refreshSetup();
  });
  window.setInterval(() => {
    void refreshRecords();
  }, 2000);
  await refresh();
});
