import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type OperatorHealth =
  | { kind: "agent_cli_missing" }
  | { kind: "stopped" }
  | { kind: "running"; bound_port: number };

type BridgeStatusView = {
  running: boolean;
  bound_port: number | null;
  preferred_port: number;
  error: string | null;
  agent_cli_present: boolean;
  health: OperatorHealth;
  start_enabled: boolean;
  start_block_reason: string | null;
  bridge_mode: "ask" | "agent" | "plan";
  bridge_workspace: string;
};

const healthEl = () => document.querySelector("#health");
const errorEl = () => document.querySelector<HTMLElement>("#error");
const boundEl = () => document.querySelector("#bound-port");
const preferredEl = () =>
  document.querySelector<HTMLInputElement>("#preferred-port");
const bridgeModeEl = () =>
  document.querySelector<HTMLSelectElement>("#bridge-mode");
const bridgeWorkspaceEl = () =>
  document.querySelector<HTMLInputElement>("#bridge-workspace");
const pickBridgeWorkspaceBtn = () =>
  document.querySelector<HTMLButtonElement>("#pick-bridge-workspace");
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
const cliBlockEl = () => document.querySelector<HTMLElement>("#cli-block");
const redetectCliBtn = () =>
  document.querySelector<HTMLButtonElement>("#redetect-cli");
const shellEl = () => document.querySelector<HTMLElement>("#shell");
const sidebarEl = () => document.querySelector<HTMLElement>("#sidebar");
const releaseBannerEl = () =>
  document.querySelector<HTMLElement>("#release-banner");
const releaseMessageEl = () => document.querySelector("#release-message");
const releaseDownloadEl = () =>
  document.querySelector<HTMLAnchorElement>("#release-download");
const dismissReleaseBtn = () =>
  document.querySelector<HTMLButtonElement>("#dismiss-release");
const preferencesIdleEl = () =>
  document.querySelector<HTMLElement>("#preferences-idle");

let releaseIgnored = false;
let selected: SidebarItem | null = null;
let wizardBusy = false;
let wizardStickyError = "";

const WIZARD_IDLE_LABEL = "完成向导";
const WIZARD_BUSY_LABEL = "正在完成向导…";

type UpdatePrompt = {
  latest_version: string;
  download_url: string;
};

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
  autostart_offered: boolean;
  move_folder_warning: string;
};

type SidebarItem =
  | "bridge"
  | "credentials"
  | "caller"
  | "records"
  | "autostart"
  | "preferences";

type ConsoleShellView = {
  mode: "wizard" | "sidebar";
  nav: { id: SidebarItem; label: string }[];
  selected: SidebarItem | null;
  pane: SidebarItem | null;
  preferences_shows_update: boolean;
  setup: SetupStatus;
  update_prompt: UpdatePrompt | null;
};

function renderSetup(status: SetupStatus) {
  const prereqs = wizardPrereqsEl();
  if (prereqs) {
    const cli = status.agent_cli_present
      ? "已检测到 Agent CLI"
      : "未找到 Agent CLI";
    const token = status.has_bridge_token
      ? "已有 Bridge Token"
      : "尚无 Bridge Token";
    prereqs.textContent = `${cli} · ${token}`;
  }
  const warn = wizardWarningEl();
  if (warn) warn.textContent = status.move_folder_warning;
  const settingsWarn = autostartWarningEl();
  if (settingsWarn) settingsWarn.textContent = status.move_folder_warning;
  const complete = completeWizardBtn();
  if (complete) {
    complete.disabled = wizardBusy || !status.can_complete;
    complete.textContent = wizardBusy ? WIZARD_BUSY_LABEL : WIZARD_IDLE_LABEL;
    complete.setAttribute("aria-busy", wizardBusy ? "true" : "false");
  }
  const error = wizardErrorEl();
  if (error) {
    if (wizardStickyError) {
      error.hidden = false;
      error.textContent = wizardStickyError;
    } else if (!wizardBusy) {
      error.hidden = true;
      error.textContent = "";
    }
  }
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

function renderNav(view: ConsoleShellView) {
  const nav = sidebarEl();
  if (!nav) return;
  nav.replaceChildren();
  for (const item of view.nav) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = item.label;
    if (item.id === view.selected) {
      button.setAttribute("aria-current", "page");
    }
    button.addEventListener("click", () => {
      selected = item.id;
      void refreshShell();
    });
    nav.append(button);
  }
}

function renderPanes(view: ConsoleShellView) {
  const activeId = `pane-${view.pane ?? ""}`;
  const panes = document.querySelectorAll<HTMLElement>("[id^='pane-']");
  for (const pane of panes) {
    pane.hidden = pane.id !== activeId;
  }
  const active = document.getElementById(activeId);
  const error = errorEl();
  if (active && error && error.parentElement !== active) {
    error.hidden = true;
    error.textContent = "";
    active.prepend(error);
  }
}

function renderShell(view: ConsoleShellView) {
  const stayOnWizard = view.mode === "wizard" || wizardBusy || !!wizardStickyError;
  const wizard = wizardEl();
  if (wizard) wizard.hidden = !stayOnWizard;
  const shell = shellEl();
  if (shell) shell.hidden = stayOnWizard;
  if (!stayOnWizard) {
    renderNav(view);
    renderPanes(view);
  }
  renderRelease(view);
}

function showWizardOnly() {
  const wizard = wizardEl();
  if (wizard) wizard.hidden = false;
  const shell = shellEl();
  if (shell) shell.hidden = true;
}

async function refreshShell() {
  const view = await invoke<ConsoleShellView>("console_shell", {
    selected,
    releaseIgnored,
  });
  selected = view.selected;
  renderSetup(view.setup);
  renderShell(view);
}

async function refreshSetup() {
  try {
    await refreshShell();
  } catch (err) {
    showWizardOnly();
    wizardStickyError = String(err);
    const error = wizardErrorEl();
    if (error) {
      error.hidden = false;
      error.textContent = wizardStickyError;
    }
  }
}

async function completeWizard() {
  if (wizardBusy) return;
  const enable = wizardAutostartEl()?.checked ?? true;
  wizardBusy = true;
  wizardStickyError = "";
  const complete = completeWizardBtn();
  if (complete) {
    complete.disabled = true;
    complete.textContent = WIZARD_BUSY_LABEL;
    complete.setAttribute("aria-busy", "true");
  }
  const error = wizardErrorEl();
  if (error) {
    error.hidden = true;
    error.textContent = "";
  }
  try {
    await invoke<SetupStatus>("complete_wizard", {
      enableAutostart: enable,
    });
    wizardBusy = false;
    wizardStickyError = "";
    await refreshShell();
  } catch (err) {
    wizardBusy = false;
    wizardStickyError = String(err);
    showWizardOnly();
    if (error) {
      error.hidden = false;
      error.textContent = wizardStickyError;
    }
    if (complete) {
      complete.disabled = false;
      complete.textContent = WIZARD_IDLE_LABEL;
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
    ? "Agent CLI 已登录，可不贴 Cursor API Key"
    : "Agent CLI 未登录";
  el.textContent = `${key} · ${login}`;
}

function healthLabel(status: BridgeStatusView) {
  if (status.health.kind === "running") {
    return `运行中 · 127.0.0.1:${status.health.bound_port}`;
  }
  if (status.health.kind === "agent_cli_missing") {
    return "未找到 Agent CLI";
  }
  return "未启动";
}

function render(status: BridgeStatusView) {
  const health = healthEl();
  const error = errorEl();
  const bound = boundEl();
  const preferred = preferredEl();
  if (health) {
    health.textContent = healthLabel(status);
  }
  const block = cliBlockEl();
  if (block) {
    if (status.start_block_reason && !status.running) {
      block.hidden = false;
      block.textContent = status.start_block_reason;
    } else {
      block.hidden = true;
      block.textContent = "";
    }
  }
  if (bound) {
    bound.textContent = status.bound_port ? String(status.bound_port) : "—";
  }
  if (preferred && document.activeElement !== preferred) {
    preferred.value = String(status.preferred_port);
  }
  const mode = bridgeModeEl();
  if (mode && document.activeElement !== mode) {
    mode.value = status.bridge_mode;
  }
  const workspace = bridgeWorkspaceEl();
  if (workspace && document.activeElement !== workspace) {
    workspace.value = status.bridge_workspace;
  }
  if (error) {
    const message =
      status.error && status.error !== status.start_block_reason
        ? status.error
        : "";
    if (message) {
      error.hidden = false;
      error.textContent = message;
    } else {
      error.hidden = true;
      error.textContent = "";
    }
  }
  const start = startBtn();
  const stop = stopBtn();
  const copy = copyConfigBtn();
  if (start) start.disabled = !status.start_enabled;
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
    area.value = await invoke<string>("caller_config_display");
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

function renderRelease(view: ConsoleShellView) {
  const banner = releaseBannerEl();
  const idle = preferencesIdleEl();
  if (view.preferences_shows_update && view.update_prompt) {
    if (banner) {
      banner.hidden = false;
      const message = releaseMessageEl();
      if (message) {
        message.textContent = `发现新 Release ${view.update_prompt.latest_version}。请自行下载，Console 不会替换正在运行的 exe。`;
      }
      const link = releaseDownloadEl();
      if (link) link.href = view.update_prompt.download_url;
    }
    if (idle) idle.hidden = true;
    return;
  }
  if (banner) banner.hidden = true;
  if (idle) idle.hidden = false;
}

async function refreshRelease() {
  try {
    await invoke("release_check");
    await refreshShell();
  } catch {
    // Preferences can stay idle until a later check succeeds.
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
    const status = await invoke<BridgeStatusView>("bridge_status").catch(
      () => null,
    );
    if (status) {
      render({ ...status, error: String(err) });
    }
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

async function redetectAgentCli() {
  try {
    const status = await invoke<BridgeStatusView>("redetect_agent_cli");
    render(status);
    await refreshCallerConfig(status.running);
    await refreshSetup();
  } catch (err) {
    const error = errorEl();
    if (error) {
      error.hidden = false;
      error.textContent = String(err);
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
    try {
      const text = await invoke<string>("caller_config");
      await navigator.clipboard.writeText(text);
      const area = callerConfigEl();
      if (area) {
        area.value = await invoke<string>("caller_config_display");
      }
    } catch (err) {
      const error = errorEl();
      if (error) {
        error.hidden = false;
        error.textContent = String(err);
      }
    }
  });
  rotateBtn()?.addEventListener("click", () => {
    void rotateToken();
  });
  saveKeyBtn()?.addEventListener("click", () => {
    void saveCursorApiKey();
  });
  redetectCliBtn()?.addEventListener("click", () => {
    void redetectAgentCli();
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
  preferredEl()?.addEventListener("change", () => {
    const preferred = Number(preferredEl()?.value || 8765);
    void invoke<BridgeStatusView>("set_preferred_port", {
      preferredPort: preferred,
    }).then(render);
  });
  bridgeModeEl()?.addEventListener("change", () => {
    const bridgeMode = bridgeModeEl()?.value || "agent";
    void invoke<BridgeStatusView>("set_bridge_mode", {
      bridgeMode,
    }).then(render);
  });
  bridgeWorkspaceEl()?.addEventListener("change", () => {
    const bridgeWorkspace = bridgeWorkspaceEl()?.value || "";
    void invoke<BridgeStatusView>("set_bridge_workspace", {
      bridgeWorkspace,
    }).then(render);
  });
  pickBridgeWorkspaceBtn()?.addEventListener("click", () => {
    void invoke<BridgeStatusView>("pick_bridge_workspace").then(render);
  });
  dismissReleaseBtn()?.addEventListener("click", () => {
    releaseIgnored = true;
    void refreshShell();
  });
  await listen("console-opened", () => {
    selected = null;
    void refreshShell();
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
  void refreshRelease();
});
