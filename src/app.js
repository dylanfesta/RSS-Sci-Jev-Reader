"use strict";

const state = {
  auth: {
    has_saved_key: false,
    is_unlocked: false,
    masked_key: null,
    active_provider_id: "openrouter",
    active_provider_name: "OpenRouter",
    requires_key: true,
    portal_url: "https://openrouter.ai/keys",
    unlocked_providers: [],
  },
  counts: {
    active_entries: 0,
    archived_entries: 0,
    total_remembered_entries: 0,
    feeds: 0,
    queries: 0,
  },
  settings: {
    ai_provider: "openrouter",
    provider_api_url: "https://openrouter.ai/api/alpha/decisions",
    provider_wire_format: "systemone",
    jev_model: "typesafe/jev-1.14",
    max_entries_per_feed: "25",
    custom_provider_name: "",
    custom_base_url: "",
    custom_portal_url: "",
  },
  exportTomlText: "",
  exportSuggestedFilename: "smart_rss_config.toml",
  importFlow: null,
  providers: [],
  activeProvider: null,
  workspace: {
    data_dir: "~/SmartRSSReader",
    skills_provisioned: true,
    files: [],
  },
  feeds: [],
  queries: [],
  activeEntries: [],
  archivedEntries: [],
  testerEntries: [],
  feedStats: {
    active: {},
    archive: {},
  },
  journalFilters: {
    active: { mode: "all", selectedIds: new Set(), searchQuery: "" },
    archive: { mode: "all", selectedIds: new Set(), searchQuery: "" },
  },
};

async function openExternalUrl(url) {
  const target = (url || "").trim();
  if (!target || (!target.startsWith("http://") && !target.startsWith("https://"))) {
    showToast("Please enter a valid http:// or https:// URL first.", "error");
    return;
  }
  if (getTauriInvoke()) {
    try {
      await apiFetch("/api/open-url", {
        method: "POST",
        body: JSON.stringify({ url: target }),
      });
      return;
    } catch (_e) {
      // Fallback below
    }
  }
  window.open(target, "_blank", "noopener,noreferrer");
}

function escapeHtml(value) {
  if (value === null || value === undefined) return "";
  return String(value)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

// WebKit2GTK on Linux passes <optgroup label>, <option> text, and native title="" tooltips
// into GTK Pango markup in popup menus/tooltips; replacing raw '&' prevents Gtk-WARNING spam.
function escapeGtkLabel(value) {
  if (value === null || value === undefined) return "";
  return escapeHtml(String(value).replace(/\s*&\s*/g, " and "));
}

function showToast(message, type = "info") {
  const container = document.getElementById("toast-container");
  if (!container) return;
  const el = document.createElement("div");
  el.className = `toast ${type}`;
  el.textContent = message;
  container.appendChild(el);
  setTimeout(() => {
    el.remove();
  }, 4200);
}

function getTauriInvoke() {
  if (
    window.__TAURI_INTERNALS__ &&
    typeof window.__TAURI_INTERNALS__.invoke === "function"
  ) {
    return window.__TAURI_INTERNALS__.invoke;
  }
  if (
    window.__TAURI__ &&
    window.__TAURI__.core &&
    typeof window.__TAURI__.core.invoke === "function"
  ) {
    return window.__TAURI__.core.invoke;
  }
  return null;
}

async function apiFetch(path, options = {}) {
  const invoke = getTauriInvoke();
  const method = (options.method || "GET").toUpperCase();
  let bodyObj = null;
  if (options.body) {
    try {
      bodyObj = typeof options.body === "string" ? JSON.parse(options.body) : options.body;
    } catch (_e) {
      bodyObj = null;
    }
  }

  if (invoke) {
    try {
      return await invoke("api_dispatch", {
        method,
        path,
        body: bodyObj,
      });
    } catch (err) {
      const msg = typeof err === "string" ? err : (err && (err.message || err.detail)) || String(err);
      throw new Error(msg);
    }
  }

  // Fallback to loopback HTTP server (e.g. http://127.0.0.1:8765)
  const headers = { "Content-Type": "application/json", ...(options.headers || {}) };
  const response = await fetch(path, { ...options, headers });
  let data = null;
  try {
    data = await response.json();
  } catch (_err) {
    data = null;
  }
  if (!response.ok) {
    const detail = (data && (data.detail || data.message)) || `Request failed (${response.status})`;
    throw new Error(detail);
  }
  return data;
}

// --- Navigation Tabs ---

function switchTab(tabId) {
  document.querySelectorAll(".nav-tab").forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.tab === tabId);
  });
  document.querySelectorAll(".tab-panel").forEach((panel) => {
    panel.classList.toggle("active", panel.id === tabId);
  });
}

// --- Header & Counts ---

function updateCountsUI(counts) {
  if (!counts) return;
  state.counts = { ...state.counts, ...counts };
  document.getElementById("stat-total-remembered").textContent = state.counts.total_remembered_entries;
  document.getElementById("stat-active-count").textContent = state.counts.active_entries;
  document.getElementById("stat-archived-count").textContent = state.counts.archived_entries;
  document.getElementById("nav-badge-active").textContent = state.counts.active_entries;
  document.getElementById("nav-badge-archived").textContent = state.counts.archived_entries;
  document.getElementById("nav-badge-feeds").textContent = state.counts.feeds;
  document.getElementById("nav-badge-queries").textContent = state.counts.queries;
}

// --- AI Skills & Workspace Info Frame ---

function updateWorkspaceUI(ws) {
  if (!ws) return;
  state.workspace = { ...state.workspace, ...ws };
  const dir = state.workspace.data_dir || "~/SmartRSSReader";
  const summaryPath = document.getElementById("info-summary-folder-path");
  const fullPath = document.getElementById("info-full-folder-path");
  const newFolderInput = document.getElementById("new-data-folder-input");
  const settingsDirInput = document.getElementById("setting-data-dir");
  const statusPill = document.getElementById("info-skills-status-pill");

  if (summaryPath) summaryPath.textContent = dir;
  if (fullPath) fullPath.textContent = dir;
  if (newFolderInput && !newFolderInput.value) newFolderInput.value = dir;
  if (settingsDirInput) settingsDirInput.value = dir;
  if (statusPill) {
    statusPill.textContent = state.workspace.skills_provisioned
      ? "✓ AI Skills Provisioned"
      : "⚠ Needs Re-sync";
  }
}

async function loadWorkspaceInfo() {
  try {
    const data = await apiFetch("/api/workspace");
    updateWorkspaceUI(data);
  } catch (_err) {
    // Ignore if endpoint unavailable
  }
}

function initAiInfoFrame() {
  const details = document.getElementById("ai-skills-info-frame");
  const toggleBtn = document.getElementById("btn-toggle-ai-info");
  const closeBtn = document.getElementById("btn-close-info-frame");
  if (!details || !toggleBtn) return;

  // On first launch (or installation), automatically expand the Info / AI Skills frame
  const onboardingSeen = localStorage.getItem("smart_rss_onboarding_seen");
  if (!onboardingSeen) {
    details.open = true;
    localStorage.setItem("smart_rss_onboarding_seen", "1");
  }

  function syncExpandedState() {
    toggleBtn.setAttribute("aria-expanded", details.open ? "true" : "false");
  }
  syncExpandedState();

  details.addEventListener("toggle", syncExpandedState);

  toggleBtn.addEventListener("click", () => {
    details.open = !details.open;
    syncExpandedState();
  });

  if (closeBtn) {
    closeBtn.addEventListener("click", () => {
      details.open = false;
      syncExpandedState();
    });
  }

  // Copy Folder Path
  const copyPathBtn = document.getElementById("btn-copy-folder-path");
  if (copyPathBtn) {
    copyPathBtn.addEventListener("click", async () => {
      const pathText = state.workspace.data_dir || "";
      try {
        await navigator.clipboard.writeText(pathText);
        showToast(`Copied folder path: ${pathText}`, "success");
      } catch (_err) {
        showToast(pathText, "info");
      }
    });
  }

  // Open Folder in OS File Manager
  async function openDataFolderInOS() {
    try {
      await apiFetch("/api/workspace/open", { method: "POST" });
      showToast("Opened Data & AI Skills Folder.", "info");
    } catch (err) {
      showToast(err.message, "error");
    }
  }

  const openFolderBtn = document.getElementById("btn-open-data-folder");
  if (openFolderBtn) openFolderBtn.addEventListener("click", openDataFolderInOS);

  const settingsOpenFolderBtn = document.getElementById("btn-settings-open-folder");
  if (settingsOpenFolderBtn) settingsOpenFolderBtn.addEventListener("click", openDataFolderInOS);

  // Re-sync AI Skills
  const resyncBtn = document.getElementById("btn-resync-skills");
  if (resyncBtn) {
    resyncBtn.addEventListener("click", async () => {
      try {
        const data = await apiFetch("/api/workspace/resync", { method: "POST" });
        updateWorkspaceUI(data);
        showToast("Re-synced AGENTS.md, .agents/skills/*, and ./smart-rss in Data Folder!", "success");
      } catch (err) {
        showToast(err.message, "error");
      }
    });
  }

  // Toggle Change Folder Form
  const changeFolderForm = document.getElementById("change-folder-form");
  const toggleChangeBtn = document.getElementById("btn-toggle-change-folder");
  const cancelChangeBtn = document.getElementById("btn-cancel-change-folder");

  if (toggleChangeBtn && changeFolderForm) {
    toggleChangeBtn.addEventListener("click", () => {
      document.getElementById("new-data-folder-input").value = state.workspace.data_dir || "";
      changeFolderForm.classList.toggle("hidden");
    });
  }
  if (cancelChangeBtn && changeFolderForm) {
    cancelChangeBtn.addEventListener("click", () => {
      changeFolderForm.classList.add("hidden");
    });
  }

  async function changeDataFolder(newDir) {
    if (!newDir) return;
    try {
      const data = await apiFetch("/api/workspace", {
        method: "PUT",
        body: JSON.stringify({ data_dir: newDir }),
      });
      updateWorkspaceUI(data);
      if (changeFolderForm) changeFolderForm.classList.add("hidden");
      await Promise.all([
        checkAuthStatus(false),
        loadSettings(),
        loadFeeds(),
        loadQueries(),
        loadActiveEntries(),
      ]);
      showToast(`Switched Data & AI Skills Folder to ${data.data_dir}`, "success");
    } catch (err) {
      showToast(err.message, "error");
    }
  }

  if (changeFolderForm) {
    changeFolderForm.addEventListener("submit", async (e) => {
      e.preventDefault();
      const val = document.getElementById("new-data-folder-input").value.trim();
      await changeDataFolder(val);
    });
  }

  const settingsFolderForm = document.getElementById("settings-folder-form");
  if (settingsFolderForm) {
    settingsFolderForm.addEventListener("submit", async (e) => {
      e.preventDefault();
      const val = document.getElementById("setting-data-dir").value.trim();
      await changeDataFolder(val);
    });
  }

  // Copy prompt / command snippets
  document.body.addEventListener("click", async (e) => {
    const copyBtn = e.target.closest(".btn-copy-snippet");
    if (!copyBtn) return;
    const text = copyBtn.dataset.copy || "";
    try {
      await navigator.clipboard.writeText(text);
      showToast("Copied to clipboard!", "success");
    } catch (_err) {
      showToast(text, "info");
    }
  });
}

// --- Auth / Password Encryption Modal ---

function getSelectedRadioValue(name, fallback = "openrouter") {
  const checked = document.querySelector(`input[name="${name}"]:checked`);
  return checked ? checked.value : fallback;
}

function setSelectedRadioValue(name, value) {
  const radios = document.querySelectorAll(`input[name="${name}"]`);
  let matched = false;
  radios.forEach((r) => {
    const isMatch = r.value === value;
    r.checked = isMatch;
    if (isMatch) matched = true;
  });
  if (!matched && radios.length > 0) {
    radios[0].checked = true;
  }
}

function parseTagsInput(raw) {
  const seen = new Set();
  const out = [];
  for (const part of String(raw || "").split(",")) {
    const clean = part.trim();
    if (!clean) continue;
    const lower = clean.toLowerCase();
    if (!seen.has(lower)) {
      seen.add(lower);
      out.push(clean);
    }
  }
  return out;
}

function formatTagsInput(tags) {
  return Array.isArray(tags) ? tags.join(", ") : "";
}

function findProviderSpec(providerId) {
  return (
    state.providers.find((p) => p.id === providerId) ||
    state.providers[0] || {
      id: "openrouter",
      name: "OpenRouter (TypeSafe Jev)",
      api_url: "https://openrouter.ai/api/alpha/decisions",
      portal_url: "https://openrouter.ai/keys",
      default_model: "typesafe/jev-1.14",
      recommended_models: ["typesafe/jev-1.14", "typesafe/jev-1.13", "typesafe/jev-router"],
      requires_key: true,
      description: "Official OpenRouter Decisions API for TypeSafe Jev models.",
    }
  );
}

function getResolvedPortalUrl(providerId) {
  if (providerId === "custom") {
    return (
      state.settings.custom_portal_url ||
      (state.auth && state.auth.portal_url) ||
      ""
    ).trim();
  }
  const customOverride = (state.settings.custom_portal_url || "").trim();
  if (customOverride) return customOverride;
  const spec = findProviderSpec(providerId || state.settings.ai_provider || "openrouter");
  return (spec.portal_url || "").trim();
}

function syncSetupModalProvider(providerId, applyPresetDefaults = false) {
  const spec = findProviderSpec(providerId);
  const selectEl = document.getElementById("setup-provider-select");
  if (selectEl && selectEl.value !== spec.id) {
    selectEl.value = spec.id;
  }

  const customBox = document.getElementById("setup-custom-provider-box");
  if (customBox) {
    customBox.classList.toggle("hidden", spec.id !== "custom");
  }

  const apiUrlInp = document.getElementById("setup-provider-api-url");
  const modelInp = document.getElementById("setup-jev-model");
  if (applyPresetDefaults) {
    if (apiUrlInp && spec.id !== "custom") {
      apiUrlInp.value = spec.api_url;
    }
    if (modelInp) {
      modelInp.value = spec.default_model;
    }
    populateModelDatalist(spec.recommended_models || []);
  }

  const keyInput = document.getElementById("setup-apikey-input");
  const keyLabel = document.getElementById("setup-apikey-label-text");
  if (keyInput) {
    keyInput.required = spec.requires_key !== false;
    keyInput.placeholder =
      spec.requires_key === false
        ? "Optional for local/custom endpoint (leave blank if none)"
        : `Paste your ${spec.name} API key...`;
  }
  if (keyLabel) {
    keyLabel.textContent =
      spec.requires_key === false
        ? `${spec.name} API Key (Optional)`
        : `${spec.name} API Key`;
  }
  const portalBtn = document.getElementById("btn-auth-open-portal");
  if (portalBtn) {
    const label = spec.id === "custom" ? "Open Provider Link" : `Get ${spec.name.split(" (")[0]} Key`;
    portalBtn.textContent = `🔗 ${label} ↗`;
  }
}

function updateAuthUI() {
  const dot = document.getElementById("auth-status-dot");
  const txt = document.getElementById("auth-status-text");
  const btn = document.getElementById("btn-open-auth");
  const headerPortalBtn = document.getElementById("btn-header-provider-link");
  const provName = state.auth.active_provider_name || "OpenRouter";

  if (headerPortalBtn) {
    const portalUrl = state.auth.portal_url || getResolvedPortalUrl(state.auth.active_provider_id);
    const rawTitle = portalUrl
      ? `Open ${provName} Portal / API Keys (${portalUrl})`
      : `Configure ${provName} link in Settings`;
    headerPortalBtn.title = rawTitle.replace(/\s*&\s*/g, " and ");
    headerPortalBtn.textContent = `🔗 ${provName}`;
  }

  if (state.auth.requires_key === false && !state.auth.masked_key) {
    dot.className = "status-dot unlocked";
    txt.textContent = `${provName} (Local — No Key Required)`;
    btn.textContent = "Provider / Keys";
  } else if (state.auth.is_unlocked) {
    dot.className = "status-dot unlocked";
    txt.textContent = `${provName}: Unlocked (${state.auth.masked_key || "RAM"})`;
    btn.textContent = "Lock / Keys";
  } else if (state.auth.has_saved_key) {
    dot.className = "status-dot locked";
    txt.textContent = `${provName}: Key Locked`;
    btn.textContent = "Unlock Key";
  } else {
    dot.className = "status-dot locked";
    txt.textContent = `${provName}: No Key Set`;
    btn.textContent = "Set API Key";
  }
}

function setUnlockError(message = "") {
  const input = document.getElementById("unlock-password-input");
  const banner = document.getElementById("unlock-password-error");
  const textEl = document.getElementById("unlock-password-error-text");
  if (!banner) return;

  if (message) {
    if (textEl) textEl.textContent = message;
    banner.classList.remove("hidden");
    if (input) {
      input.setAttribute("aria-invalid", "true");
      input.setAttribute("aria-describedby", "unlock-password-error");
    }
  } else {
    banner.classList.add("hidden");
    if (input) {
      input.setAttribute("aria-invalid", "false");
      input.removeAttribute("aria-describedby");
    }
  }
}

function setSetupAuthError(message = "") {
  const banner = document.getElementById("setup-auth-error");
  const textEl = document.getElementById("setup-auth-error-text");
  if (!banner) return;

  if (message) {
    if (textEl) textEl.textContent = message;
    banner.classList.remove("hidden");
  } else {
    if (textEl) textEl.textContent = "";
    banner.classList.add("hidden");
  }
}

const passwordPeekTimers = new WeakMap();

function maskPasswordInput(input, btn) {
  if (!input || !btn) return;
  const timer = passwordPeekTimers.get(input);
  if (timer) {
    clearTimeout(timer);
    passwordPeekTimers.delete(input);
  }
  input.type = "password";
  btn.setAttribute("aria-pressed", "false");
  btn.setAttribute("aria-label", "Show password briefly");
}

function maskAllPeekPasswords() {
  document.querySelectorAll(".btn-password-peek").forEach((btn) => {
    const input = document.getElementById(btn.dataset.peekTarget || "");
    maskPasswordInput(input, btn);
  });
}

function initBriefPasswordPeek(durationMs = 1500) {
  document.querySelectorAll(".btn-password-peek").forEach((btn) => {
    btn.addEventListener("click", () => {
      const input = document.getElementById(btn.dataset.peekTarget || "");
      if (!input) return;

      if (input.type === "text") {
        maskPasswordInput(input, btn);
        return;
      }

      const prevTimer = passwordPeekTimers.get(input);
      if (prevTimer) clearTimeout(prevTimer);

      input.type = "text";
      btn.setAttribute("aria-pressed", "true");
      btn.setAttribute("aria-label", "Hide password");

      const timer = setTimeout(() => {
        maskPasswordInput(input, btn);
      }, durationMs);
      passwordPeekTimers.set(input, timer);
    });
  });
}

function openAuthModal(forceSetup = false) {
  const dialog = document.getElementById("auth-dialog");
  const unlockForm = document.getElementById("auth-unlock-form");
  const setupForm = document.getElementById("auth-setup-form");
  const unlockedPanel = document.getElementById("auth-unlocked-panel");
  const backBtn = document.getElementById("btn-back-to-unlock");

  setUnlockError("");
  setSetupAuthError("");
  maskAllPeekPasswords();

  unlockForm.classList.add("hidden");
  setupForm.classList.add("hidden");
  unlockedPanel.classList.add("hidden");

  const provName = state.auth.active_provider_name || "AI Provider";
  const activeProvId = state.auth.active_provider_id || state.settings.ai_provider || "openrouter";

  if (state.auth.is_unlocked && !forceSetup) {
    document.getElementById("auth-modal-title").textContent = `${provName} — Ready & Active`;
    const activeLabel = document.getElementById("unlocked-active-provider-label");
    if (activeLabel) activeLabel.textContent = provName;
    document.getElementById("unlocked-masked-key").textContent =
      state.auth.masked_key || (state.auth.requires_key === false ? "Local (No Key Needed)" : "Active");
    const unlockedList = document.getElementById("unlocked-providers-list-info");
    if (unlockedList) {
      const list = state.auth.unlocked_providers || [];
      unlockedList.textContent =
        list.length > 0 ? `Unlocked provider keys in memory: ${list.join(", ")}` : "";
    }
    unlockedPanel.classList.remove("hidden");
  } else if (state.auth.has_saved_key && !forceSetup) {
    document.getElementById("auth-modal-title").textContent = `Unlock Encrypted API Keys (${provName})`;
    document.getElementById("unlock-password-input").value = "";
    unlockForm.classList.remove("hidden");
  } else {
    document.getElementById("auth-modal-title").textContent = "Configure Jev Endpoint, Model & API Key";
    document.getElementById("setup-apikey-input").value = "";
    document.getElementById("setup-password-input").value = "";
    const apiUrlInp = document.getElementById("setup-provider-api-url");
    const modelInp = document.getElementById("setup-jev-model");
    const customNameInp = document.getElementById("setup-custom-name");
    const customPortalInp = document.getElementById("setup-custom-portal-url");
    const spec = findProviderSpec(activeProvId);
    if (apiUrlInp) {
      apiUrlInp.value =
        (state.activeProvider && state.activeProvider.api_url) ||
        state.settings.provider_api_url ||
        spec.api_url;
    }
    if (modelInp) {
      modelInp.value =
        (state.activeProvider && state.activeProvider.model) ||
        state.settings.jev_model ||
        spec.default_model;
    }
    if (customNameInp) customNameInp.value = state.settings.custom_provider_name || "";
    if (customPortalInp) customPortalInp.value = state.settings.custom_portal_url || "";
    syncSetupModalProvider(activeProvId, false);
    backBtn.classList.toggle("hidden", !state.auth.has_saved_key);
    setupForm.classList.remove("hidden");
  }

  if (!dialog.open) {
    dialog.showModal();
  }
  if (!unlockForm.classList.contains("hidden")) {
    document.getElementById("unlock-password-input")?.focus();
  } else if (!setupForm.classList.contains("hidden")) {
    document.getElementById("setup-apikey-input")?.focus();
  }
}

async function checkAuthStatus(promptIfLocked = false) {
  const data = await apiFetch("/api/auth/status");
  state.auth = { ...state.auth, ...data };
  updateAuthUI();
  if (promptIfLocked && !state.auth.is_unlocked && state.auth.requires_key !== false) {
    openAuthModal(false);
  }
}

// --- Ranked & Archived Entries Rendering ---

function renderEntryCard(entry, isArchived = false) {
  const pct = Number(entry.relevance_pct || 0);
  const rawScore = Number(entry.weighted_score || 0);
  const maxScore = Number(entry.max_possible_score || 0);
  let badgeTier = "low";
  if (entry.is_scored && pct < 0) badgeTier = "penalty";
  else if (entry.is_scored && pct >= 70) badgeTier = "high";
  else if (entry.is_scored && pct >= 40) badgeTier = "med";

  const scores = entry.query_scores || [];
  let breakdownHtml = "";

  if (!entry.is_scored || scores.length === 0) {
    breakdownHtml = `
      <div class="query-pills-row">
        <span class="query-score-pill"><em>Not yet evaluated by Jev</em></span>
      </div>
    `;
  } else {
    const rowsHtml = scores
      .map((qs) => {
        const norm = Number(qs.normalized_score || 0);
        const contribNum = Number(qs.weighted_contribution || 0);
        const contribStr = `${contribNum > 0 ? "+" : ""}${contribNum.toFixed(2)}`;
        const weightNum = Number(qs.weight || 0);
        const weightStr = `${weightNum > 0 ? "+" : ""}${weightNum}`;
        const isPenalty = weightNum < 0 && contribNum < -0.005;
        const isPositive = contribNum > 0.005 && weightNum > 0;

        let histBadge = "";
        if (qs.query_deleted_since_run) {
          histBadge = `<span class="hist-tag" title="This query was deleted after this paper was evaluated; showing snapshot from test run">deleted query snapshot</span>`;
        } else if (qs.query_modified_since_run) {
          histBadge = `<span class="hist-tag" title="This query was modified after this paper was evaluated; showing snapshot from test run">snapshot at test run</span>`;
        }

        const snapInstr = (qs.query_snapshot && qs.query_snapshot.instructions) || "";

        let outcomeHtml = "";
        if (qs.query_type === "noul") {
          const isYes = norm >= 0.5;
          const confVal =
            qs.confidence !== null && qs.confidence !== undefined
              ? Number(qs.confidence) * 100
              : Math.max(norm, 1 - norm) * 100;
          outcomeHtml = `
            <span class="verdict-pill ${isYes ? "yes" : "no"}">${isYes ? "Yes" : "No"}</span>
            <span class="outcome-detail">
              <strong>${confVal.toFixed(1)}% confidence</strong>
              <span class="muted-label">(P(Yes) = ${(norm * 100).toFixed(1)}%)</span>
            </span>
          `;
        } else {
          outcomeHtml = `
            <span class="verdict-pill score">Score:</span>
            <span class="outcome-detail">${escapeHtml(qs.raw_outcome_text || `${(norm * 100).toFixed(1)}%`)}</span>
          `;
        }

        return `
          <tr class="${!qs.enabled ? "row-disabled" : ""}">
            <td class="col-query-name" title="${escapeGtkLabel(snapInstr)}">
              <strong>${escapeHtml(qs.query_name)}</strong>
              ${histBadge}
            </td>
            <td class="col-raw-outcome">${outcomeHtml}</td>
            <td class="col-weight"><code>${weightStr}</code></td>
            <td class="col-weighted-val ${isPenalty ? "val-penalty" : isPositive ? "val-positive" : ""}">
              <strong>${contribStr}</strong>
            </td>
          </tr>
        `;
      })
      .join("");

    breakdownHtml = `
      <div class="jev-breakdown-wrap">
        <table class="jev-breakdown-table">
          <thead>
            <tr>
              <th>Jev Query (at test time)</th>
              <th>Raw Outcome &amp; Confidence</th>
              <th>Weight</th>
              <th>Weighted Value</th>
            </tr>
          </thead>
          <tbody>
            ${rowsHtml}
          </tbody>
        </table>
      </div>
    `;
  }

  const actionBtn = isArchived
    ? `<button type="button" class="btn btn-sm btn-secondary btn-unarchive-entry" data-id="${entry.id}">↩ Unarchive</button>`
    : `<button type="button" class="btn btn-sm btn-outline btn-archive-entry" data-id="${entry.id}">📥 Archive</button>`;

  return `
    <article class="entry-card" data-entry-id="${entry.id}">
      <div class="score-badge-col ${badgeTier}" title="Weighted sum: ${rawScore.toFixed(2)} / ${maxScore.toFixed(2)}">
        <div class="score-pct">${entry.is_scored ? `${pct.toFixed(1)}%` : "—"}</div>
        <div class="score-raw">${entry.is_scored ? `${rawScore.toFixed(2)} / ${maxScore.toFixed(2)}` : "Unscored"}</div>
      </div>

      <div class="entry-main">
        <div class="entry-meta-top">
          <span class="journal-pill">${escapeHtml(entry.journal_title || "Journal")}</span>
          ${entry.published_at ? `<span>• ${escapeHtml(entry.published_at)}</span>` : ""}
        </div>

        <h3 class="entry-title">
          ${
            entry.link
              ? `<a href="${escapeHtml(entry.link)}" class="external-article-link" target="_blank" rel="noopener noreferrer">${escapeHtml(entry.title)} ↗</a>`
              : escapeHtml(entry.title)
          }
        </h3>

        ${entry.authors ? `<div class="entry-authors">${escapeHtml(entry.authors)}</div>` : ""}

        ${breakdownHtml}

        ${
          entry.summary || entry.link
            ? `
          <div class="entry-footer-bar">
            ${
              entry.summary
                ? `<button type="button" class="entry-abstract-toggle" aria-expanded="false">▸ Show Abstract / Summary</button>`
                : ""
            }
            ${
              entry.link
                ? `<a href="${escapeHtml(entry.link)}" class="external-article-link entry-paper-link" target="_blank" rel="noopener noreferrer" title="${escapeHtml(entry.link)}">Open Paper ↗</a>`
                : ""
            }
          </div>
          ${
            entry.summary
              ? `<div class="entry-abstract-text hidden">${escapeHtml(entry.summary)}</div>`
              : ""
          }
        `
            : ""
        }
      </div>

      <div class="entry-actions">
        ${actionBtn}
        <button type="button" class="btn btn-xs btn-ghost btn-send-to-tester" data-id="${entry.id}" data-archived="${isArchived ? "1" : "0"}">
          🔬 Test in Jev
        </button>
      </div>
    </article>
  `;
}

function getJournalFilterConfig(scope) {
  const isArchive = scope === "archive";
  return {
    scope,
    filterState: state.journalFilters[scope],
    wrapper: document.getElementById(isArchive ? "archive-journal-multiselect" : "active-journal-multiselect"),
    triggerBtn: document.getElementById(isArchive ? "archive-feed-filter-btn" : "active-feed-filter-btn"),
    labelEl: document.getElementById(isArchive ? "archive-feed-filter-label" : "active-feed-filter-label"),
    panel: document.getElementById(isArchive ? "archive-feed-filter-panel" : "active-feed-filter-panel"),
    masterCb: document.getElementById(isArchive ? "archive-feed-master-cb" : "active-feed-master-cb"),
    selectAllBtn: document.getElementById(isArchive ? "archive-feed-select-all" : "active-feed-select-all"),
    selectNoneBtn: document.getElementById(isArchive ? "archive-feed-select-none" : "active-feed-select-none"),
    searchInput: document.getElementById(isArchive ? "archive-feed-option-search" : "active-feed-option-search"),
    optionsList: document.getElementById(isArchive ? "archive-feed-options-list" : "active-feed-options-list"),
    reloadEntries: isArchive ? loadArchivedEntries : loadActiveEntries,
    countProp: isArchive ? "archived_entries" : "active_entries",
  };
}

function normalizeJournalFilterState(scope) {
  const filterState = state.journalFilters[scope];
  const validIds = new Set(state.feeds.map((f) => Number(f.id)));
  if (filterState.mode === "subset") {
    for (const id of Array.from(filterState.selectedIds)) {
      if (!validIds.has(id)) {
        filterState.selectedIds.delete(id);
      }
    }
    if (state.feeds.length > 0 && filterState.selectedIds.size === state.feeds.length) {
      filterState.mode = "all";
      filterState.selectedIds.clear();
    } else if (filterState.selectedIds.size === 0) {
      filterState.mode = "none";
    }
  }
}

function getJournalFilterQueryParam(scope) {
  const filterState = state.journalFilters[scope];
  if (filterState.mode === "all") return null;
  if (filterState.mode === "none" || filterState.selectedIds.size === 0) return "none";
  return Array.from(filterState.selectedIds).join(",");
}

function syncJournalFilterSummaryUI(scope) {
  const cfg = getJournalFilterConfig(scope);
  if (!cfg.labelEl || !cfg.masterCb) return;

  const total = state.feeds.length;
  const fs = cfg.filterState;

  let selectedCount = 0;
  if (fs.mode === "all") {
    selectedCount = total;
  } else if (fs.mode === "none") {
    selectedCount = 0;
  } else {
    selectedCount = fs.selectedIds.size;
  }

  // Update master checkbox checked & indeterminate state
  if (total === 0) {
    cfg.masterCb.checked = fs.mode !== "none";
    cfg.masterCb.indeterminate = false;
  } else if (selectedCount === total) {
    cfg.masterCb.checked = true;
    cfg.masterCb.indeterminate = false;
  } else if (selectedCount === 0) {
    cfg.masterCb.checked = false;
    cfg.masterCb.indeterminate = false;
  } else {
    cfg.masterCb.checked = false;
    cfg.masterCb.indeterminate = true;
  }

  // Update trigger button summary text and visual active indicator
  const isFiltered = total > 0 && selectedCount !== total;
  if (cfg.triggerBtn) {
    cfg.triggerBtn.classList.toggle("has-active-filter", isFiltered);
  }

  if (total === 0) {
    cfg.labelEl.textContent = "All Journals / Feeds";
  } else if (selectedCount === total) {
    cfg.labelEl.textContent = `All Journals (${total})`;
  } else if (selectedCount === 0) {
    cfg.labelEl.textContent = "No Journals Selected (0)";
  } else if (selectedCount === 1) {
    const onlyId = Array.from(fs.selectedIds)[0];
    const feed = state.feeds.find((f) => Number(f.id) === onlyId);
    cfg.labelEl.textContent = feed ? feed.title : `1 of ${total} Journals`;
  } else {
    cfg.labelEl.textContent = `${selectedCount} of ${total} Journals`;
  }
}

function getArchiveBelowThreshold() {
  const inp = document.getElementById("bulk-threshold-input");
  const val = Number(inp ? inp.value : 35);
  return Number.isFinite(val) ? val : 35;
}

function renderJournalFilterOptions(scope) {
  const cfg = getJournalFilterConfig(scope);
  if (!cfg.optionsList) return;

  normalizeJournalFilterState(scope);
  syncJournalFilterSummaryUI(scope);

  if (state.feeds.length === 0) {
    cfg.optionsList.innerHTML = `<div class="journal-multiselect-empty">No RSS feeds configured yet.</div>`;
    return;
  }

  const q = (cfg.filterState.searchQuery || "").trim().toLowerCase();
  const matchingFeeds = q
    ? state.feeds.filter(
        (f) =>
          String(f.title || "").toLowerCase().includes(q) ||
          String(f.category || "").toLowerCase().includes(q) ||
          (Array.isArray(f.tags) && f.tags.some((t) => String(t).toLowerCase().includes(q)))
      )
    : state.feeds;

  if (matchingFeeds.length === 0) {
    cfg.optionsList.innerHTML = `<div class="journal-multiselect-empty">No matching journals for "${escapeHtml(
      cfg.filterState.searchQuery
    )}".</div>`;
    return;
  }

  const thresh = getArchiveBelowThreshold();
  const scopeStats = state.feedStats[scope] || {};

  cfg.optionsList.innerHTML = matchingFeeds
    .map((f) => {
      const fid = Number(f.id);
      const isChecked =
        cfg.filterState.mode === "all" ||
        (cfg.filterState.mode === "subset" && cfg.filterState.selectedIds.has(fid));

      const statObj = scopeStats[String(fid)];
      const totalCount = statObj ? Number(statObj.total || 0) : Number(f[cfg.countProp] || 0);
      const scoredPcts = statObj && Array.isArray(statObj.scored_pcts) ? statObj.scored_pcts : [];
      const aboveCount = scoredPcts.filter((pct) => Number(pct) >= thresh).length;
      const countTitle = `${aboveCount} paper${aboveCount === 1 ? "" : "s"} at or above ${thresh}% threshold out of ${totalCount} total`;

      const cbId = `${scope}-journal-opt-${fid}`;
      return `
        <div class="journal-option-row ${isChecked ? "is-checked" : ""}" data-feed-id="${fid}">
          <input
            type="checkbox"
            id="${cbId}"
            class="checkbox-custom journal-option-cb"
            value="${fid}"
            ${isChecked ? "checked" : ""}
          />
          <label for="${cbId}" class="journal-option-label">
            <span class="journal-option-title">${escapeHtml(f.title)}</span>
            <span class="journal-option-meta">
              ${f.category ? `<span class="journal-option-cat">${escapeHtml(f.category)}</span>` : ""}
              <span class="journal-option-count ${aboveCount > 0 ? "has-above" : ""}" title="${escapeGtkLabel(countTitle)}"><strong class="count-above">${aboveCount}</strong><span class="count-sep">/</span><span class="count-total">${totalCount}</span></span>
            </span>
          </label>
          <button
            type="button"
            class="btn-journal-only"
            data-only-id="${fid}"
            aria-label="Select only ${escapeHtml(f.title)}"
            title="Select only this journal"
          >
            Only
          </button>
        </div>
      `;
    })
    .join("");
}

function closeAllJournalDropdowns(exceptScope = null) {
  ["active", "archive"].forEach((s) => {
    if (s === exceptScope) return;
    const cfg = getJournalFilterConfig(s);
    if (cfg.panel && !cfg.panel.classList.contains("hidden")) {
      cfg.panel.classList.add("hidden");
      cfg.triggerBtn?.setAttribute("aria-expanded", "false");
    }
  });
}

function initJournalMultiselect(scope) {
  const cfg = getJournalFilterConfig(scope);
  if (!cfg.wrapper || !cfg.triggerBtn || !cfg.panel) return;

  cfg.triggerBtn.addEventListener("click", () => {
    const isOpen = !cfg.panel.classList.contains("hidden");
    if (isOpen) {
      cfg.panel.classList.add("hidden");
      cfg.triggerBtn.setAttribute("aria-expanded", "false");
    } else {
      closeAllJournalDropdowns(scope);
      cfg.panel.classList.remove("hidden");
      cfg.triggerBtn.setAttribute("aria-expanded", "true");
      renderJournalFilterOptions(scope);
      cfg.searchInput?.focus();
    }
  });

  function selectAllJournals() {
    cfg.filterState.mode = "all";
    cfg.filterState.selectedIds.clear();
    renderJournalFilterOptions(scope);
    cfg.reloadEntries();
  }

  function selectNoneJournals() {
    cfg.filterState.mode = "none";
    cfg.filterState.selectedIds.clear();
    renderJournalFilterOptions(scope);
    cfg.reloadEntries();
  }

  cfg.selectAllBtn?.addEventListener("click", selectAllJournals);
  cfg.selectNoneBtn?.addEventListener("click", selectNoneJournals);

  cfg.masterCb?.addEventListener("change", () => {
    if (cfg.masterCb.checked) {
      selectAllJournals();
    } else {
      selectNoneJournals();
    }
  });

  cfg.searchInput?.addEventListener("input", () => {
    cfg.filterState.searchQuery = cfg.searchInput.value;
    renderJournalFilterOptions(scope);
  });

  cfg.optionsList?.addEventListener("change", (e) => {
    const cb = e.target.closest(".journal-option-cb");
    if (!cb) return;
    const fid = Number(cb.value);
    const allIds = state.feeds.map((f) => Number(f.id));

    if (cfg.filterState.mode === "all") {
      // Switching from 'all' to 'subset'
      cfg.filterState.selectedIds = new Set(allIds);
      cfg.filterState.mode = "subset";
    } else if (cfg.filterState.mode === "none") {
      cfg.filterState.selectedIds = new Set();
      cfg.filterState.mode = "subset";
    }

    if (cb.checked) {
      cfg.filterState.selectedIds.add(fid);
    } else {
      cfg.filterState.selectedIds.delete(fid);
    }

    normalizeJournalFilterState(scope);
    syncJournalFilterSummaryUI(scope);
    const row = cb.closest(".journal-option-row");
    if (row) row.classList.toggle("is-checked", cb.checked);
    cfg.reloadEntries();
  });

  cfg.optionsList?.addEventListener("click", (e) => {
    const onlyBtn = e.target.closest(".btn-journal-only");
    if (!onlyBtn) return;
    const fid = Number(onlyBtn.dataset.onlyId);
    if (state.feeds.length === 1) {
      cfg.filterState.mode = "all";
      cfg.filterState.selectedIds.clear();
    } else {
      cfg.filterState.mode = "subset";
      cfg.filterState.selectedIds = new Set([fid]);
    }
    renderJournalFilterOptions(scope);
    cfg.reloadEntries();
  });
}

async function loadActiveEntries() {
  const search = document.getElementById("active-search-input").value.trim();
  const feedIdsParam = getJournalFilterQueryParam("active");
  const params = new URLSearchParams({ status: "active" });
  if (search) params.set("search", search);
  if (feedIdsParam !== null) params.set("feed_ids", feedIdsParam);

  const data = await apiFetch(`/api/entries?${params.toString()}`);
  state.activeEntries = data.entries || [];
  if (data.feed_stats) {
    state.feedStats.active = data.feed_stats;
    renderJournalFilterOptions("active");
  }
  updateCountsUI(data.counts);

  const container = document.getElementById("active-entries-list");
  if (state.activeEntries.length === 0) {
    if (feedIdsParam === "none") {
      container.innerHTML = `
        <div class="empty-state">
          <h3>No Journals Selected</h3>
          <p>Open the journal selector above and check one or more journals (or click <strong>Select all</strong>) to display active papers.</p>
        </div>
      `;
    } else if (feedIdsParam !== null || search) {
      container.innerHTML = `
        <div class="empty-state">
          <h3>No Matching Active Entries</h3>
          <p>No active papers match your current search or selected journal filters.</p>
        </div>
      `;
    } else {
      container.innerHTML = `
        <div class="empty-state">
          <h3>No Active RSS Entries</h3>
          <p>Click <strong>"↻ Update Feeds (New Entries Only)"</strong> above to fetch your scientific journal feeds and rank new papers with Jev.</p>
        </div>
      `;
    }
    return;
  }
  container.innerHTML = state.activeEntries.map((e) => renderEntryCard(e, false)).join("");
}

async function loadArchivedEntries() {
  const search = document.getElementById("archive-search-input").value.trim();
  const feedIdsParam = getJournalFilterQueryParam("archive");
  const params = new URLSearchParams({ status: "archived" });
  if (search) params.set("search", search);
  if (feedIdsParam !== null) params.set("feed_ids", feedIdsParam);

  const data = await apiFetch(`/api/entries?${params.toString()}`);
  state.archivedEntries = data.entries || [];
  if (data.feed_stats) {
    state.feedStats.archive = data.feed_stats;
    renderJournalFilterOptions("archive");
  }
  updateCountsUI(data.counts);

  const container = document.getElementById("archived-entries-list");
  if (state.archivedEntries.length === 0) {
    if (feedIdsParam === "none") {
      container.innerHTML = `
        <div class="empty-state">
          <h3>No Journals Selected</h3>
          <p>Open the journal selector above and check one or more journals (or click <strong>Select all</strong>) to view archived papers.</p>
        </div>
      `;
    } else if (feedIdsParam !== null || search) {
      container.innerHTML = `
        <div class="empty-state">
          <h3>No Matching Archived Entries</h3>
          <p>No archived papers match your current search or selected journal filters.</p>
        </div>
      `;
    } else {
      container.innerHTML = `
        <div class="empty-state">
          <h3>Archive is Empty</h3>
          <p>Papers you archive from the Ranked Feed appear here and are permanently remembered so they won't be re-scored on future updates.</p>
        </div>
      `;
    }
    return;
  }
  container.innerHTML = state.archivedEntries.map((e) => renderEntryCard(e, true)).join("");
}

// --- Expandable RSS Feeds List + Live Preview ---

function populateFeedDropdowns() {
  const testerFeedSelect = document.getElementById("tester-feed-select");
  const currentTesterVal = testerFeedSelect ? testerFeedSelect.value : "";

  const optionsHtml = state.feeds
    .map((f) => `<option value="${f.id}">${escapeGtkLabel(f.title)}</option>`)
    .join("");

  if (testerFeedSelect) {
    testerFeedSelect.innerHTML = state.feeds.length
      ? optionsHtml
      : `<option value="">No RSS feeds configured</option>`;
    if (currentTesterVal) testerFeedSelect.value = currentTesterVal;
  }

  renderJournalFilterOptions("active");
  renderJournalFilterOptions("archive");
}

function renderFeedPreviewHtml(previewData) {
  const items = previewData.entries || [];
  if (items.length === 0) {
    return `<p class="muted-label">Feed parsed successfully, but returned 0 entries.</p>`;
  }
  const rows = items
    .slice(0, 10)
    .map((item) => {
      const statusBadge = item.already_seen
        ? `<span class="badge badge-muted">Already in Memory</span>`
        : `<span class="badge">NEW Entry</span>`;
      return `
        <div class="preview-entry-item">
          <div>
            <div>
              ${statusBadge}
              <strong>${escapeHtml(item.title)}</strong>
            </div>
            <div class="muted-label">
              ${item.authors ? `${escapeHtml(item.authors)} · ` : ""}
              ${escapeHtml(item.published_at || "")}
            </div>
          </div>
          <button
            type="button"
            class="btn btn-xs btn-secondary btn-use-preview-entry"
            data-title="${escapeHtml(item.title)}"
            data-summary="${escapeHtml(item.summary || "")}"
            data-journal="${escapeHtml(item.journal_title || previewData.feed_title || "")}"
            data-authors="${escapeHtml(item.authors || "")}"
          >
            🔬 Test in Jev
          </button>
        </div>
      `;
    })
    .join("");

  return `
    <div>
      <div class="split-header-sm">
        <strong>Live Preview: ${escapeHtml(previewData.feed_title)} (${previewData.entry_count} entries parsed)</strong>
      </div>
      ${rows}
    </div>
  `;
}

function renderFeedsList() {
  const container = document.getElementById("feeds-expandable-list");
  if (state.feeds.length === 0) {
    container.innerHTML = `
      <div class="empty-state">
        <h3>No RSS Feeds Configured</h3>
        <p>Click <strong>"+ Add RSS Feed"</strong> to add your favorite scientific journal RSS links.</p>
      </div>
    `;
    return;
  }

  container.innerHTML = state.feeds
    .map((f) => {
      const tagsArr = Array.isArray(f.tags) ? f.tags : [];
      const tagsHtml =
        tagsArr.length > 0
          ? `<span class="feed-tags-wrap">${tagsArr
              .map((t) => `<span class="tag-pill">#${escapeHtml(t)}</span>`)
              .join("")}</span>`
          : "";
      return `
      <details class="expandable-item" data-feed-id="${f.id}">
        <summary>
          <div class="summary-left">
            <span class="expand-chevron" aria-hidden="true">▶</span>
            <input
              type="checkbox"
              class="feed-enable-toggle stop-summary-toggle"
              data-id="${f.id}"
              ${f.enabled ? "checked" : ""}
              title="Enable or disable this feed during updates"
            />
            <strong>${escapeHtml(f.title)}</strong>
            <span class="journal-pill">${escapeHtml(f.category || "General")}</span>
            ${tagsHtml}
            <code>${escapeHtml(f.url)}</code>
          </div>
          <div class="summary-right">
            <span class="muted-label">
              ${Number(f.active_entries || 0)} active · ${Number(f.archived_entries || 0)} archived
            </span>
            <button type="button" class="btn btn-xs btn-secondary btn-test-existing-feed" data-id="${f.id}">
              🔍 Test / Preview
            </button>
          </div>
        </summary>

        <div class="expandable-body">
          <form class="edit-feed-form" data-id="${f.id}">
            <div class="form-grid-4">
              <div class="field">
                <label>Journal / Feed Title</label>
                <input type="text" name="title" class="input" value="${escapeHtml(f.title)}" required />
              </div>
              <div class="field">
                <label>RSS / Atom URL</label>
                <input type="url" name="url" class="input" value="${escapeHtml(f.url)}" autocorrect="off" autocapitalize="none" spellcheck="false" required />
              </div>
              <div class="field">
                <label>Category</label>
                <input type="text" name="category" class="input" value="${escapeHtml(f.category || "General")}" />
              </div>
              <div class="field">
                <label>Tags (comma-separated)</label>
                <input type="text" name="tags" class="input" value="${escapeHtml(formatTagsInput(tagsArr))}" placeholder="e.g. preprint, computational" autocorrect="off" autocapitalize="none" spellcheck="false" />
              </div>
            </div>
            <div class="split-header-sm">
              <div class="muted-label">
                ${f.last_fetched_at ? `Last fetched: ${escapeHtml(f.last_fetched_at)}` : "Never fetched yet"}
                ${f.last_error ? ` · <span style="color:var(--danger)">Error: ${escapeHtml(f.last_error)}</span>` : ""}
              </div>
              <div class="form-actions">
                <button type="button" class="btn btn-sm btn-secondary btn-test-existing-feed" data-id="${f.id}">
                  🔍 Test / Preview
                </button>
                <button type="submit" class="btn btn-sm btn-primary">Save Changes</button>
                <button type="button" class="btn btn-sm btn-danger btn-delete-feed" data-id="${f.id}">Delete Feed</button>
              </div>
            </div>
          </form>
          <div id="feed-preview-box-${f.id}" class="feed-preview-box hidden"></div>
        </div>
      </details>
    `;
    })
    .join("");
}

async function loadFeeds() {
  const data = await apiFetch("/api/feeds");
  state.feeds = (data.feeds || []).slice().sort((a, b) =>
    String(a.title || "").localeCompare(String(b.title || ""), undefined, { sensitivity: "base" })
  );
  populateFeedDropdowns();
  renderFeedsList();
}

// --- Expandable Jev Queries List + Weights + Tester ---

function populateTesterQueriesDropdown() {
  const sel = document.getElementById("tester-query-select");
  const prevVal = sel.value;
  if (state.queries.length === 0) {
    sel.innerHTML = `<option value="">No Jev queries configured</option>`;
    return;
  }
  sel.innerHTML = state.queries
    .map(
      (q) =>
        `<option value="${q.id}">${escapeGtkLabel(q.name)} [${q.query_type === "noul" ? "Yes/No" : "Score"} · w=${q.weight}]</option>`
    )
    .join("");
  if (prevVal) sel.value = prevVal;
}

function renderScoreLevelsInputs(
  levels = ["Low relevance", "Moderate relevance", "High relevance"],
  { asPlaceholders = false, required = true } = {}
) {
  return levels
    .map((lvl, idx) => {
      const valAttr = asPlaceholders ? "" : `value="${escapeHtml(lvl)}"`;
      const placeholderText = asPlaceholders ? lvl : `Describe level ${idx + 1}...`;
      const reqAttr = required ? "required" : "";
      return `
      <div class="level-row">
        <span class="level-index">Level ${idx + 1}</span>
        <input type="text" class="input score-level-input" ${valAttr} placeholder="${escapeHtml(placeholderText)}" ${reqAttr} />
        <button type="button" class="btn btn-xs btn-ghost btn-remove-level" title="Remove level">✕</button>
      </div>
    `;
    })
    .join("");
}

function renderQueriesList() {
  const container = document.getElementById("queries-expandable-list");
  if (state.queries.length === 0) {
    container.innerHTML = `
      <div class="empty-state">
        <h3>No Jev Queries Configured</h3>
        <p>Click <strong>"+ Add Jev Query"</strong> to create Yes/No (<code>noul</code>) or Rubric Score (<code>score</code>) queries with custom weights.</p>
      </div>
    `;
    return;
  }

  container.innerHTML = state.queries
    .map((q) => {
      const isScore = q.query_type === "score";
      const trueDesc = !isScore && q.criteria ? q.criteria.true || "" : "";
      const falseDesc = !isScore && q.criteria ? q.criteria.false || "" : "";
      const hasSavedScoreLevels = isScore && Array.isArray(q.criteria) && q.criteria.length >= 2;
      const scoreLevels = hasSavedScoreLevels
        ? q.criteria
        : ["e.g. Not relevant", "e.g. Moderately relevant", "e.g. Highly relevant"];

      return `
        <details class="expandable-item" data-query-id="${q.id}">
          <summary>
            <div class="summary-left">
              <span class="expand-chevron" aria-hidden="true">▶</span>
              <input
                type="checkbox"
                class="query-enable-toggle stop-summary-toggle"
                data-id="${q.id}"
                ${q.enabled ? "checked" : ""}
                title="Include this query in weighted sum ranking"
              />
              <strong>${escapeHtml(q.name)}</strong>
              <span class="type-badge ${q.query_type}">
                ${q.query_type === "noul" ? "Yes/No (noul)" : "Score (rubric)"}
              </span>
            </div>

            <div class="summary-right">
              <label class="weight-inline-box stop-summary-toggle ${Number(q.weight) < 0 ? "penalty" : ""}" title="Weight in weighted sum (negative weights subtract from paper ranking)">
                <span>${Number(q.weight) < 0 ? "Penalty w:" : "Weight:"}</span>
                <input
                  type="number"
                  class="weight-inline-input query-quick-weight stop-summary-toggle"
                  data-id="${q.id}"
                  value="${q.weight}"
                  step="0.1"
                />
              </label>
              <button type="button" class="btn btn-xs btn-secondary btn-select-query-in-tester" data-id="${q.id}">
                🔬 Test
              </button>
            </div>
          </summary>

          <div class="expandable-body">
            <form class="edit-query-form" data-id="${q.id}">
              <div class="form-grid-3">
                <div class="field">
                  <label>Query Name</label>
                  <input type="text" name="name" class="input" value="${escapeHtml(q.name)}" required />
                </div>
                <div class="field">
                  <label>Query Type</label>
                  <select name="query_type" class="select edit-query-type-select">
                    <option value="noul" ${!isScore ? "selected" : ""}>Yes / No Probability (noul)</option>
                    <option value="score" ${isScore ? "selected" : ""}>Ordered Rubric Scale (score)</option>
                  </select>
                </div>
                <div class="field">
                  <label>Weight (w)</label>
                  <input type="number" name="weight" class="input" value="${q.weight}" step="0.1" required />
                </div>
              </div>

              <div class="field">
                <label>Instructions / Question for Jev</label>
                <textarea name="instructions" class="textarea" rows="2" required>${escapeHtml(q.instructions)}</textarea>
              </div>

              <div class="criteria-subbox edit-noul-box ${isScore ? "hidden" : ""}">
                <p class="criteria-hint">Optional semantic definitions for Yes (True) and No (False):</p>
                <div class="form-grid-2">
                  <div class="field">
                    <label>When True (Yes)</label>
                    <input type="text" name="true_desc" class="input" value="${escapeHtml(trueDesc)}" />
                  </div>
                  <div class="field">
                    <label>When False (No)</label>
                    <input type="text" name="false_desc" class="input" value="${escapeHtml(falseDesc)}" />
                  </div>
                </div>
              </div>

              <div class="criteria-subbox edit-score-box ${!isScore ? "hidden" : ""}">
                <div class="split-header-sm">
                  <p class="criteria-hint">Ordered Rubric Levels (Lowest <code>[0]</code> to Highest <code>[K-1]</code>):</p>
                  <button type="button" class="btn btn-xs btn-secondary btn-add-edit-level">+ Add Level</button>
                </div>
                <div class="levels-list edit-levels-container">
                  ${renderScoreLevelsInputs(scoreLevels, { asPlaceholders: !hasSavedScoreLevels, required: isScore })}
                </div>
              </div>

              <div class="form-actions">
                <button type="submit" class="btn btn-sm btn-primary">Save Query</button>
                <button type="button" class="btn btn-sm btn-danger btn-delete-query" data-id="${q.id}">Delete Query</button>
              </div>
            </form>
          </div>
        </details>
      `;
    })
    .join("");
}

async function loadQueries() {
  const data = await apiFetch("/api/queries");
  state.queries = data.queries || [];
  populateTesterQueriesDropdown();
  renderQueriesList();
}

// --- Interactive Single-Query Jev Tester ---

async function loadEntriesForTesterFeed() {
  const feedId = document.getElementById("tester-feed-select").value;
  if (!feedId) {
    showToast("Please select an RSS feed first.", "error");
    return;
  }
  const btn = document.getElementById("btn-tester-load-feed");
  const entrySelect = document.getElementById("tester-entry-select");
  btn.disabled = true;
  btn.textContent = "Fetching...";

  try {
    const data = await apiFetch("/api/feeds/test", {
      method: "POST",
      body: JSON.stringify({ feed_id: Number(feedId), max_entries: 25 }),
    });
    state.testerEntries = data.entries || [];
    if (state.testerEntries.length === 0) {
      entrySelect.innerHTML = `<option value="">No entries returned by feed</option>`;
      return;
    }
    entrySelect.innerHTML = state.testerEntries
      .map((e, idx) => `<option value="${idx}">${escapeGtkLabel(e.title)}</option>`)
      .join("");
    selectTesterEntryByIndex(0);
    showToast(`Loaded ${state.testerEntries.length} entries from ${data.feed_title}`, "success");
  } catch (err) {
    showToast(err.message, "error");
  } finally {
    btn.disabled = false;
    btn.textContent = "Fetch Entries";
  }
}

function selectTesterEntryByIndex(idx) {
  const entry = state.testerEntries[Number(idx)];
  if (!entry) return;
  document.getElementById("tester-entry-title").value = entry.title || "";
  document.getElementById("tester-entry-summary").value = entry.summary || "";
}

function renderTesterResult(res) {
  const box = document.getElementById("tester-result-box");
  box.classList.remove("hidden");

  const normPct = (Number(res.normalized_score) * 100).toFixed(1);
  const raw = res.raw_response || {};
  let barsHtml = "";

  if (res.query_type === "noul") {
    const yesProb = Number(raw.noul || res.normalized_score || 0) * 100;
    const noProb = Math.max(0, 100 - yesProb);
    barsHtml = `
      <div class="prob-bar-row">
        <div class="prob-bar-label"><span>Yes (True)</span><strong>${yesProb.toFixed(1)}%</strong></div>
        <div class="prob-bar-track"><div class="prob-bar-fill" style="width:${yesProb}%"></div></div>
      </div>
      <div class="prob-bar-row">
        <div class="prob-bar-label"><span>No (False)</span><strong>${noProb.toFixed(1)}%</strong></div>
        <div class="prob-bar-track"><div class="prob-bar-fill" style="width:${noProb}%;background:var(--text-faint)"></div></div>
      </div>
    `;
  } else if (res.query_type === "score" && Array.isArray(raw.legend)) {
    const probs = Array.isArray(raw.probabilities) ? raw.probabilities : [];
    barsHtml = raw.legend
      .map((label, i) => {
        const p = Number(probs[i] || 0) * 100;
        return `
          <div class="prob-bar-row">
            <div class="prob-bar-label">
              <span>Level ${i + 1}: ${escapeHtml(label)}</span>
              <strong>${p.toFixed(1)}%</strong>
            </div>
            <div class="prob-bar-track"><div class="prob-bar-fill" style="width:${p}%"></div></div>
          </div>
        `;
      })
      .join("");
  }

  box.innerHTML = `
    <div class="split-header-sm">
      <strong>Jev Result: ${escapeHtml(res.query_name)}</strong>
      <span class="badge">${escapeHtml(res.display_value)}</span>
    </div>
    <div class="muted-label" style="margin-bottom:0.5rem">
      Model: <code>${escapeHtml(res.model || state.settings.jev_model || "")}</code>
      ${res.api_url ? ` · Endpoint: <code>${escapeHtml(res.api_url)}</code>` : ""}
    </div>
    <div class="muted-label" style="margin-bottom:0.5rem">
      Normalized Score: <strong>${Number(res.normalized_score).toFixed(4)} (${normPct}%)</strong> ·
      Weight: <code>${res.weight}</code> ·
      Weighted Contribution: <strong>${Number(res.weighted_contribution).toFixed(4)}</strong>
      ${raw.confidence !== undefined && raw.confidence !== null ? ` · Confidence: <strong>${(Number(raw.confidence) * 100).toFixed(1)}%</strong>` : ""}
    </div>
    ${barsHtml}
    <details style="margin-top:0.75rem">
      <summary style="font-size:0.78rem;cursor:pointer;color:var(--text-muted)">Inspect Raw Jev JSON Payload &amp; Response</summary>
      <pre role="region" aria-label="Raw Jev JSON Payload and Response" tabindex="0" style="font-size:0.74rem;overflow-x:auto;background:var(--surface);padding:0.6rem;border-radius:4px;border:1px solid var(--border)"><code>${escapeHtml(
        JSON.stringify({ response: res.raw_response, request: res.request_preview }, null, 2)
      )}</code></pre>
    </details>
  `;
}

async function runSingleJevTest() {
  const title = document.getElementById("tester-entry-title").value.trim();
  const summary = document.getElementById("tester-entry-summary").value.trim();
  const queryId = document.getElementById("tester-query-select").value;

  if (!title) {
    showToast("Please pick an RSS entry (or enter a title) to test.", "error");
    return;
  }
  if (!queryId) {
    showToast("Please select a Jev query to test.", "error");
    return;
  }

  const btn = document.getElementById("btn-run-jev-test");
  btn.disabled = true;
  btn.textContent = "⏳ Querying OpenRouter Jev...";

  try {
    const selectedIdx = document.getElementById("tester-entry-select").value;
    const entryMeta = state.testerEntries[Number(selectedIdx)] || {};
    const res = await apiFetch("/api/queries/test", {
      method: "POST",
      body: JSON.stringify({
        query_id: Number(queryId),
        entry: {
          journal_title: entryMeta.journal_title || "",
          title,
          authors: entryMeta.authors || "",
          published_at: entryMeta.published_at || "",
          summary,
          link: entryMeta.link || "",
        },
      }),
    });
    renderTesterResult(res);
    showToast("Jev query evaluated!", "success");
  } catch (err) {
    showToast(err.message, "error");
    if (err.message.toLowerCase().includes("locked")) {
      openAuthModal(false);
    }
  } finally {
    btn.disabled = false;
    btn.textContent = "▶ Run Individual Jev Query";
  }
}

// --- Settings & Multi-Provider Decision Configuration ---

function populateProviderSelects() {
  const providers = state.providers || [];
  if (providers.length === 0) return;

  const groups = new Map();
  for (const p of providers) {
    const cat = p.group || "Decision Providers";
    if (!groups.has(cat)) groups.set(cat, []);
    groups.get(cat).push(p);
  }

  let html = "";
  for (const [cat, items] of groups.entries()) {
    html += `<optgroup label="${escapeGtkLabel(cat)}">`;
    for (const p of items) {
      html += `<option value="${escapeHtml(p.id)}">${escapeGtkLabel(p.name)}</option>`;
    }
    html += `</optgroup>`;
  }

  const settingSelect = document.getElementById("setting-ai-provider");
  const setupSelect = document.getElementById("setup-provider-select");
  if (settingSelect) settingSelect.innerHTML = html;
  if (setupSelect) setupSelect.innerHTML = html;
}

function populateModelDatalist(models = []) {
  const datalist = document.getElementById("provider-model-datalist");
  if (!datalist) return;
  datalist.innerHTML = models
    .map((m) => `<option value="${escapeHtml(m)}"></option>`)
    .join("");
}

function updateSettingsProviderPreview(providerId, applyPresetDefaults = false) {
  const spec = findProviderSpec(providerId);
  const descEl = document.getElementById("setting-provider-desc");
  const portalUrlEl = document.getElementById("setting-portal-url-display");
  const customMetaBox = document.getElementById("setting-custom-meta-box");
  const apiUrlInp = document.getElementById("setting-provider-api-url");
  const modelInp = document.getElementById("setting-jev-model");

  if (applyPresetDefaults) {
    if (apiUrlInp && spec.id !== "custom") {
      apiUrlInp.value = spec.api_url;
    }
    if (modelInp) {
      modelInp.value = spec.default_model;
    }
    setSelectedRadioValue("provider_wire_format", spec.wire_format || "systemone");
  }

  const customName = (document.getElementById("setting-custom-provider-name")?.value || "").trim();
  const customPortal = (document.getElementById("setting-custom-portal-url")?.value || "").trim();
  const activeUrl = (apiUrlInp?.value || spec.api_url || "").trim();

  if (descEl) {
    descEl.textContent = `${spec.description} Active URL: ${activeUrl}`;
  }
  if (portalUrlEl) {
    const link = customPortal || spec.portal_url || "(local / optional — set below)";
    const label = spec.id === "custom" && customName ? `${customName}: ${link}` : link;
    portalUrlEl.textContent = label;
  }
  if (customMetaBox) {
    const isCustomLike = spec.id === "custom" || spec.id === "litellm" || spec.id === "local-decider";
    customMetaBox.classList.toggle("hidden", !isCustomLike);
  }

  populateModelDatalist(spec.recommended_models || ["typesafe/jev-1.14"]);
}

async function loadSettings() {
  const data = await apiFetch("/api/settings");
  state.settings = { ...state.settings, ...(data.settings || {}) };
  if (Array.isArray(data.providers) && data.providers.length > 0) {
    state.providers = data.providers;
    populateProviderSelects();
  }
  if (data.active_provider) {
    state.activeProvider = data.active_provider;
  }
  updateCountsUI(data.counts);
  if (data.pruned_archived && data.pruned_archived > 0) {
    showToast(
      `Cleaned up ${data.pruned_archived} archived entry(s) older than 1 month.`,
      "info"
    );
  }
  const provId =
    (data.active_provider && data.active_provider.provider_id) ||
    state.settings.ai_provider ||
    "openrouter";
  const spec = findProviderSpec(provId);
  const settingSelect = document.getElementById("setting-ai-provider");
  if (settingSelect) settingSelect.value = spec.id;

  const wireFormat =
    (data.active_provider && data.active_provider.wire_format) ||
    state.settings.provider_wire_format ||
    spec.wire_format ||
    "systemone";
  setSelectedRadioValue("provider_wire_format", wireFormat);

  const apiUrlEl = document.getElementById("setting-provider-api-url");
  const modelEl = document.getElementById("setting-jev-model");
  const customNameEl = document.getElementById("setting-custom-provider-name");
  const customPortalEl = document.getElementById("setting-custom-portal-url");

  if (apiUrlEl) {
    apiUrlEl.value =
      (data.active_provider && data.active_provider.api_url) ||
      state.settings.provider_api_url ||
      spec.api_url;
  }
  if (modelEl) {
    modelEl.value =
      (data.active_provider && data.active_provider.model) ||
      state.settings.jev_model ||
      spec.default_model;
  }
  if (customNameEl) customNameEl.value = state.settings.custom_provider_name || "";
  if (customPortalEl) customPortalEl.value = state.settings.custom_portal_url || "";

  document.getElementById("setting-max-entries").value =
    state.settings.max_entries_per_feed || "25";

  updateSettingsProviderPreview(spec.id, false);
}

// --- TOML Export & Import with Interactive Conflict Resolution ---

async function openExportTomlDialog() {
  const dialog = document.getElementById("export-toml-dialog");
  if (!dialog) return;
  try {
    const data = await apiFetch("/api/config/export");
    state.exportTomlText = data.toml || "";
    state.exportSuggestedFilename = data.suggested_filename || "smart_rss_config.toml";

    const summaryEl = document.getElementById("export-toml-summary");
    if (summaryEl) {
      summaryEl.textContent = `Prepared ${data.feeds_count} RSS feed(s) (with categories & tags) and ${data.rules_count} Jev filter rule(s). API keys are never included.`;
    }
    const pathInp = document.getElementById("export-toml-path-input");
    if (pathInp) {
      pathInp.value = data.default_export_path || `${state.workspace.data_dir}/smart_rss_config.toml`;
    }
    const codeEl = document.getElementById("export-toml-preview-code");
    if (codeEl) {
      codeEl.textContent = state.exportTomlText;
    }

    if (!dialog.open) {
      dialog.showModal();
    }
  } catch (err) {
    showToast(err.message || "Failed to generate TOML export.", "error");
  }
}

async function downloadExportTomlViaBrowser() {
  const text = state.exportTomlText || "";
  const filename = state.exportSuggestedFilename || "smart_rss_config.toml";
  if (!text) return;

  if (typeof window.showSaveFilePicker === "function") {
    try {
      const handle = await window.showSaveFilePicker({
        suggestedName: filename,
        types: [
          {
            description: "TOML Configuration File",
            accept: { "application/toml": [".toml"], "text/plain": [".toml"] },
          },
        ],
      });
      const writable = await handle.createWritable();
      await writable.write(text);
      await writable.close();
      showToast(`Saved ${filename}!`, "success");
      return;
    } catch (err) {
      if (err && err.name === "AbortError") return;
    }
  }

  const blob = new Blob([text], { type: "application/toml;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
  showToast(`Triggered download of ${filename}. (If your desktop webview blocks downloads, use "Save to File" above.)`, "info");
}

function renderFeedComparisonHtml(feedObj, isIncoming) {
  const title = isIncoming ? feedObj.name : feedObj.title;
  const tags = Array.isArray(feedObj.tags) ? feedObj.tags : [];
  const tagsText = tags.length > 0 ? tags.map((t) => `#${t}`).join(", ") : "(none)";
  const enabledText = feedObj.enabled ? "Enabled" : "Disabled";
  return `
    <div><strong>Name:</strong> ${escapeHtml(title || "")}</div>
    <div><strong>URL:</strong> <code>${escapeHtml(feedObj.url || "")}</code></div>
    <div><strong>Category:</strong> ${escapeHtml(feedObj.category || "General")}</div>
    <div><strong>Tags:</strong> ${escapeHtml(tagsText)}</div>
    <div><strong>Status:</strong> ${escapeHtml(enabledText)}</div>
  `;
}

function renderRuleComparisonHtml(ruleObj) {
  const qtype = ruleObj.query_type || "noul";
  const weight = Number(ruleObj.weight ?? 1.0);
  const enabledText = ruleObj.enabled ? "Enabled" : "Disabled";
  let critHtml = "";
  if (qtype === "score") {
    const lvls = Array.isArray(ruleObj.levels)
      ? ruleObj.levels
      : Array.isArray(ruleObj.criteria)
        ? ruleObj.criteria
        : [];
    critHtml = `<div><strong>Levels (${lvls.length}):</strong> ${escapeHtml(lvls.join(" → "))}</div>`;
  } else {
    const t = ruleObj.true_description || (ruleObj.criteria && ruleObj.criteria.true) || "";
    const f = ruleObj.false_description || (ruleObj.criteria && ruleObj.criteria.false) || "";
    if (t) critHtml += `<div><strong>True:</strong> ${escapeHtml(t)}</div>`;
    if (f) critHtml += `<div><strong>False:</strong> ${escapeHtml(f)}</div>`;
  }
  return `
    <div><strong>Type:</strong> ${qtype === "noul" ? "Yes/No (noul)" : "Rubric (score)"}</div>
    <div><strong>Weight:</strong> <code>${weight > 0 ? "+" : ""}${weight}</code> · <strong>${escapeHtml(enabledText)}</strong></div>
    <div><strong>Instructions:</strong> ${escapeHtml(ruleObj.instructions || "")}</div>
    ${critHtml}
  `;
}

function renderCurrentImportConflict() {
  const flow = state.importFlow;
  if (!flow || flow.index >= flow.conflicts.length) return;

  const item = flow.conflicts[flow.index];
  const total = flow.conflicts.length;

  document.getElementById("conflict-progress-badge").textContent = `Conflict ${flow.index + 1} of ${total}`;
  document.getElementById("conflict-overview-text").textContent =
    `TOML contains ${flow.preview.new_feeds.length} new feed(s), ${flow.preview.new_rules.length} new filter rule(s), and ${total} existing item(s) that already exist in SmartRSSReader.`;

  const typeBadge = document.getElementById("conflict-item-type-badge");
  const nameEl = document.getElementById("conflict-item-name");
  const existingEl = document.getElementById("conflict-existing-details");
  const incomingEl = document.getElementById("conflict-incoming-details");

  if (item.kind === "feed") {
    typeBadge.className = "type-badge noul";
    typeBadge.textContent = "RSS Feed";
    nameEl.textContent = `${item.incoming.name} (${item.incoming.url})`;
    existingEl.innerHTML = renderFeedComparisonHtml(item.existing, false);
    incomingEl.innerHTML = renderFeedComparisonHtml(item.incoming, true);
  } else {
    typeBadge.className = "type-badge score";
    typeBadge.textContent = "Filter Rule";
    nameEl.textContent = item.incoming.name;
    existingEl.innerHTML = renderRuleComparisonHtml(item.existing);
    incomingEl.innerHTML = renderRuleComparisonHtml(item.incoming);
  }

  const noBtn = document.getElementById("btn-conflict-no");
  if (noBtn) noBtn.focus();
}

async function finishTomlImportFlow(overrideAll = false) {
  const flow = state.importFlow;
  if (!flow) return;
  const dialog = document.getElementById("import-conflict-dialog");
  state.importFlow = null;
  if (dialog && dialog.open) {
    dialog.close("applied");
  }

  try {
    const res = await apiFetch("/api/config/import/apply", {
      method: "POST",
      body: JSON.stringify({
        toml: flow.tomlText,
        override_all: overrideAll,
        override_feed_urls: Array.from(flow.overrideFeedUrls),
        override_rule_names: Array.from(flow.overrideRuleNames),
      }),
    });
    updateCountsUI(res.counts);
    await Promise.all([loadFeeds(), loadQueries(), loadActiveEntries(), loadSettings()]);
    showToast(
      `TOML imported: +${res.feeds_added} feeds (${res.feeds_overridden} overridden, ${res.feeds_skipped} kept) · +${res.rules_added} rules (${res.rules_overridden} overridden, ${res.rules_skipped} kept).`,
      "success"
    );
  } catch (err) {
    showToast(err.message || "Failed to apply TOML import.", "error");
  }
}

function cancelTomlImportFlow(silent = false) {
  const dialog = document.getElementById("import-conflict-dialog");
  const wasActive = Boolean(state.importFlow);
  state.importFlow = null;
  if (dialog && dialog.open) {
    dialog.close("cancelled");
  }
  if (wasActive && !silent) {
    showToast("TOML import cancelled — no changes were made.", "info");
  }
}

async function startTomlImportFlow(tomlText) {
  try {
    const preview = await apiFetch("/api/config/import/preview", {
      method: "POST",
      body: JSON.stringify({ toml: tomlText }),
    });

    const conflicts = [
      ...(preview.conflicting_feeds || []).map((c) => ({ kind: "feed", ...c })),
      ...(preview.conflicting_rules || []).map((c) => ({ kind: "rule", ...c })),
    ];

    state.importFlow = {
      tomlText,
      preview,
      conflicts,
      index: 0,
      overrideFeedUrls: new Set(),
      overrideRuleNames: new Set(),
    };

    if (conflicts.length === 0) {
      await finishTomlImportFlow(false);
      return;
    }

    renderCurrentImportConflict();
    const dialog = document.getElementById("import-conflict-dialog");
    if (dialog && !dialog.open) {
      dialog.showModal();
    }
  } catch (err) {
    state.importFlow = null;
    showToast(err.message || "Invalid TOML file.", "error");
  }
}

// --- Theme Toggle ---

function initThemeToggle() {
  const btn = document.getElementById("btn-theme-toggle");
  if (!btn) return;

  function applyTheme(theme) {
    document.documentElement.setAttribute("data-theme", theme);
    const meta = document.querySelector('meta[name="color-scheme"]');
    if (meta) meta.content = theme === "light" ? "light" : "dark";
    btn.textContent = theme === "dark" ? "🌙 Dark" : "☀️ Light";
  }

  const initial = document.documentElement.getAttribute("data-theme") || "dark";
  applyTheme(initial);

  btn.addEventListener("click", () => {
    const current = document.documentElement.getAttribute("data-theme") || "dark";
    const next = current === "dark" ? "light" : "dark";
    try {
      localStorage.setItem("smart_rss_theme", next);
    } catch (_e) {
      // Ignore storage errors
    }
    applyTheme(next);
  });

  window.addEventListener("storage", (e) => {
    if (e.key === "smart_rss_theme" && e.newValue) {
      applyTheme(e.newValue);
    }
  });
}

// --- Live SQLite Watcher Event Listener (Tauri) ---

function initTauriLiveSync() {
  const listenFn =
    (window.__TAURI__ && window.__TAURI__.event && window.__TAURI__.event.listen) ||
    null;

  if (typeof listenFn === "function") {
    listenFn("db-updated", async () => {
      await Promise.all([
        loadSettings(),
        loadFeeds(),
        loadQueries(),
        loadActiveEntries(),
        loadArchivedEntries(),
      ]);
      showToast("Live-synced updates from AI Agent / SQLite!", "info");
    });
  }
}

// --- Event Listeners Setup ---

function initEventListeners() {
  initThemeToggle();
  initAiInfoFrame();
  initTauriLiveSync();

  // Prevent summary toggle when clicking inline checkboxes/inputs inside <summary>
  document.body.addEventListener("click", (e) => {
    const abstractToggle = e.target.closest(".entry-abstract-toggle");
    if (abstractToggle) {
      const card = abstractToggle.closest(".entry-main");
      const textEl = card && card.querySelector(".entry-abstract-text");
      if (textEl) {
        const isNowOpen = textEl.classList.toggle("hidden") === false;
        abstractToggle.setAttribute("aria-expanded", isNowOpen ? "true" : "false");
        abstractToggle.textContent = isNowOpen
          ? "▾ Hide Abstract / Summary"
          : "▸ Show Abstract / Summary";
      }
      return;
    }
    if (e.target.closest(".stop-summary-toggle")) {
      e.stopPropagation();
    }
    const extLink = e.target.closest("a.external-article-link");
    if (extLink && getTauriInvoke()) {
      const href = extLink.getAttribute("href");
      if (href && (href.startsWith("http://") || href.startsWith("https://"))) {
        e.preventDefault();
        apiFetch("/api/open-url", {
          method: "POST",
          body: JSON.stringify({ url: href }),
        }).catch(() => {
          window.open(href, "_blank", "noopener,noreferrer");
        });
      }
    }
  });

  // Navigation tabs
  document.querySelectorAll(".nav-tab").forEach((btn) => {
    btn.addEventListener("click", () => {
      closeAllJournalDropdowns();
      const target = btn.dataset.tab;
      switchTab(target);
      if (target === "tab-ranked") loadActiveEntries();
      if (target === "tab-archive") loadArchivedEntries();
      if (target === "tab-feeds") loadFeeds();
      if (target === "tab-queries") loadQueries();
    });
  });

  // Auth & generic modal controls
  initBriefPasswordPeek(1500);
  const authDialog = document.getElementById("auth-dialog");
  if (authDialog) {
    authDialog.addEventListener("close", () => {
      maskAllPeekPasswords();
      authDialog.returnValue = "";
    });
  }
  const exportDialog = document.getElementById("export-toml-dialog");
  if (exportDialog) {
    exportDialog.addEventListener("close", () => {
      exportDialog.returnValue = "";
    });
  }
  const conflictDialog = document.getElementById("import-conflict-dialog");
  if (conflictDialog) {
    conflictDialog.addEventListener("cancel", (e) => {
      e.preventDefault();
      cancelTomlImportFlow(false);
    });
    conflictDialog.addEventListener("close", () => {
      conflictDialog.returnValue = "";
    });
  }

  if (!("closedBy" in HTMLDialogElement.prototype)) {
    const isOutsideDialog = (e, dlg) => {
      const r = dlg.getBoundingClientRect();
      return e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom;
    };
    document.querySelectorAll('dialog[closedby="any"]').forEach((dlg) => {
      let pressedOutside = false;
      dlg.addEventListener("pointerdown", (e) => {
        pressedOutside = isOutsideDialog(e, dlg);
      });
      dlg.addEventListener("pointerup", (e) => {
        if (pressedOutside && isOutsideDialog(e, dlg)) {
          dlg.close();
        }
      });
    });
  }

  // TOML Export & Import Triggers + Dialog Controls
  document.querySelectorAll(".btn-export-toml").forEach((btn) => {
    btn.addEventListener("click", openExportTomlDialog);
  });

  const tomlFileInput = document.getElementById("toml-import-file-input");
  document.querySelectorAll(".btn-import-toml").forEach((btn) => {
    btn.addEventListener("click", () => {
      if (tomlFileInput) {
        tomlFileInput.value = "";
        tomlFileInput.click();
      }
    });
  });

  if (tomlFileInput) {
    tomlFileInput.addEventListener("change", async () => {
      const file = tomlFileInput.files && tomlFileInput.files[0];
      tomlFileInput.value = "";
      if (!file) return;
      try {
        const text = await file.text();
        await startTomlImportFlow(text);
      } catch (err) {
        showToast(err.message || "Could not read selected TOML file.", "error");
      }
    });
  }

  document.getElementById("btn-close-export-dialog")?.addEventListener("click", () => {
    document.getElementById("export-toml-dialog")?.close();
  });

  document.getElementById("export-toml-path-form")?.addEventListener("submit", async (e) => {
    e.preventDefault();
    const pathInp = document.getElementById("export-toml-path-input");
    const targetPath = (pathInp ? pathInp.value : "").trim();
    if (!targetPath) return;
    try {
      const res = await apiFetch("/api/config/export-file", {
        method: "POST",
        body: JSON.stringify({ path: targetPath }),
      });
      showToast(
        `Exported ${res.feeds_count} feed(s) & ${res.rules_count} rule(s) to ${res.path}`,
        "success"
      );
      document.getElementById("export-toml-dialog")?.close();
    } catch (err) {
      showToast(err.message || "Failed to save TOML file.", "error");
    }
  });

  document.getElementById("btn-download-toml-blob")?.addEventListener("click", downloadExportTomlViaBrowser);

  document.getElementById("btn-copy-toml-text")?.addEventListener("click", async () => {
    if (!state.exportTomlText) return;
    try {
      await navigator.clipboard.writeText(state.exportTomlText);
      showToast("Copied TOML configuration to clipboard!", "success");
    } catch (_err) {
      showToast("Could not access clipboard; you can select text in the preview below.", "info");
    }
  });

  document.getElementById("btn-conflict-yes")?.addEventListener("click", async () => {
    const flow = state.importFlow;
    if (!flow) return;
    const current = flow.conflicts[flow.index];
    if (current) {
      if (current.kind === "feed") flow.overrideFeedUrls.add(current.key);
      else flow.overrideRuleNames.add(current.key);
    }
    flow.index += 1;
    if (flow.index >= flow.conflicts.length) {
      await finishTomlImportFlow(false);
    } else {
      renderCurrentImportConflict();
    }
  });

  document.getElementById("btn-conflict-no")?.addEventListener("click", async () => {
    const flow = state.importFlow;
    if (!flow) return;
    flow.index += 1;
    if (flow.index >= flow.conflicts.length) {
      await finishTomlImportFlow(false);
    } else {
      renderCurrentImportConflict();
    }
  });

  document.getElementById("btn-conflict-override-all")?.addEventListener("click", async () => {
    await finishTomlImportFlow(true);
  });

  document.getElementById("btn-conflict-cancel")?.addEventListener("click", () => {
    cancelTomlImportFlow(false);
  });

  document.getElementById("btn-close-conflict-dialog")?.addEventListener("click", () => {
    cancelTomlImportFlow(false);
  });

  document.getElementById("btn-open-auth").addEventListener("click", () => openAuthModal(false));
  document.getElementById("btn-close-auth").addEventListener("click", () => {
    document.getElementById("auth-dialog").close();
  });
  document.getElementById("btn-switch-to-setup").addEventListener("click", () => openAuthModal(true));
  document.getElementById("btn-replace-key").addEventListener("click", () => openAuthModal(true));
  document.getElementById("btn-back-to-unlock").addEventListener("click", () => openAuthModal(false));

  const unlockPwdInput = document.getElementById("unlock-password-input");
  if (unlockPwdInput) {
    unlockPwdInput.addEventListener("input", () => {
      if (unlockPwdInput.getAttribute("aria-invalid") === "true") {
        setUnlockError("");
      }
    });
  }

  document.getElementById("auth-unlock-form").addEventListener("submit", async (e) => {
    e.preventDefault();
    const pwdInput = document.getElementById("unlock-password-input");
    const password = pwdInput.value;
    setUnlockError("");
    maskAllPeekPasswords();
    try {
      const data = await apiFetch("/api/auth/unlock", {
        method: "POST",
        body: JSON.stringify({ password }),
      });
      state.auth = { ...state.auth, ...data };
      updateAuthUI();
      document.getElementById("auth-dialog").close();
      showToast(
        `${state.auth.active_provider_name || "AI Provider"} API key unlocked in memory!`,
        "success"
      );
    } catch (err) {
      pwdInput.value = "";
      setUnlockError(err.message || "Incorrect password.");
      pwdInput.focus();
    }
  });

  const setupProvSelect = document.getElementById("setup-provider-select");
  if (setupProvSelect) {
    setupProvSelect.addEventListener("change", () => {
      syncSetupModalProvider(setupProvSelect.value, true);
    });
  }

  const authOpenPortalBtn = document.getElementById("btn-auth-open-portal");
  if (authOpenPortalBtn) {
    authOpenPortalBtn.addEventListener("click", () => {
      const selId = document.getElementById("setup-provider-select")?.value || state.settings.ai_provider || "openrouter";
      const customLink = (document.getElementById("setup-custom-portal-url")?.value || "").trim();
      const url = customLink || getResolvedPortalUrl(selId);
      openExternalUrl(url);
    });
  }

  const unlockedOpenPortalBtn = document.getElementById("btn-unlocked-open-portal");
  if (unlockedOpenPortalBtn) {
    unlockedOpenPortalBtn.addEventListener("click", () => {
      const url = state.auth.portal_url || getResolvedPortalUrl(state.auth.active_provider_id);
      openExternalUrl(url);
    });
  }

  const headerPortalBtn = document.getElementById("btn-header-provider-link");
  if (headerPortalBtn) {
    headerPortalBtn.addEventListener("click", () => {
      const url = state.auth.portal_url || getResolvedPortalUrl(state.auth.active_provider_id);
      if (url) {
        openExternalUrl(url);
      } else {
        switchTab("tab-settings");
        showToast("Set a General Link for your Custom Provider in Settings.", "info");
      }
    });
  }

  document.getElementById("auth-setup-form").addEventListener("submit", async (e) => {
    e.preventDefault();
    const provider_id =
      document.getElementById("setup-provider-select")?.value ||
      state.settings.ai_provider ||
      "openrouter";
    const spec = findProviderSpec(provider_id);
    const provider_api_url = (document.getElementById("setup-provider-api-url")?.value || "").trim();
    const jev_model = (document.getElementById("setup-jev-model")?.value || "").trim();
    const api_key = document.getElementById("setup-apikey-input").value.trim();
    const password = document.getElementById("setup-password-input").value;
    const custom_provider_name = (document.getElementById("setup-custom-name")?.value || "").trim();
    const custom_portal_url = (document.getElementById("setup-custom-portal-url")?.value || "").trim();

    const payload = {
      provider_id,
      provider_api_url,
      provider_wire_format: spec.wire_format || "systemone",
      jev_model,
      api_key,
      password,
    };
    if (provider_id === "custom" || custom_provider_name) {
      payload.custom_provider_name = custom_provider_name;
    }
    if (provider_id === "custom" || custom_portal_url) {
      payload.custom_portal_url = custom_portal_url;
    }

    setSetupAuthError("");
    try {
      const data = await apiFetch("/api/auth/setup", {
        method: "POST",
        body: JSON.stringify(payload),
      });
      state.auth = { ...state.auth, ...data };
      updateAuthUI();
      await loadSettings();
      document.getElementById("auth-dialog").close();
      showToast(
        `${state.auth.active_provider_name || "Provider"} configured & unlocked for this session!`,
        "success"
      );
    } catch (err) {
      setSetupAuthError(err.message || "Failed to save encrypted API key.");
    }
  });

  document.getElementById("btn-lock-session").addEventListener("click", async () => {
    const data = await apiFetch("/api/auth/lock", { method: "POST" });
    state.auth = { ...state.auth, ...data };
    updateAuthUI();
    document.getElementById("auth-dialog").close();
    showToast("API key cleared from memory.", "info");
  });

  // Refresh feeds (New Only) & Score Unscored Active
  async function triggerFeedRefresh(evaluateUnscoredActive = false) {
    const btn = evaluateUnscoredActive
      ? document.getElementById("btn-score-unscored")
      : document.getElementById("btn-refresh-feeds");
    const origText = btn.textContent;
    btn.disabled = true;
    btn.textContent = "⏳ Updating & Ranking with Jev...";

    try {
      const res = await apiFetch("/api/feeds/refresh", {
        method: "POST",
        body: JSON.stringify({ evaluate_unscored_active: evaluateUnscoredActive }),
      });
      updateCountsUI(res.counts);
      const banner = document.getElementById("refresh-summary-banner");
      banner.classList.remove("hidden");
      banner.textContent = `Fetched ${res.feeds_fetched} feeds: ${res.newly_inserted} new entries discovered, ${res.skipped_known} remembered entries skipped, ${res.entries_evaluated} evaluated by Jev.`;

      if (res.evaluation_errors && res.evaluation_errors.length > 0) {
        showToast(res.evaluation_errors[0].error, "error");
      } else {
        showToast(`Feed update complete (${res.newly_inserted} new, ${res.skipped_known} skipped).`, "success");
      }
      await loadActiveEntries();
      await loadFeeds();
    } catch (err) {
      showToast(err.message, "error");
    } finally {
      btn.disabled = false;
      btn.textContent = origText;
    }
  }

  document.getElementById("btn-refresh-feeds").addEventListener("click", () => triggerFeedRefresh(false));
  document.getElementById("btn-score-unscored").addEventListener("click", () => triggerFeedRefresh(true));

  // Reset All Entries (Active + Archived)
  const resetAllBtn = document.getElementById("btn-reset-all");
  if (resetAllBtn) {
    resetAllBtn.addEventListener("click", async () => {
      const total = Number(state.counts.total_remembered_entries || 0);
      const confirmed = window.confirm(
        `Are you sure you want to RESET ALL ${total} RSS entries (both active and archived)?\n\n` +
          `This will wipe your entire RSS memory blank so all articles will be re-read and re-evaluated with Jev on the next update.`
      );
      if (!confirmed) return;

      resetAllBtn.disabled = true;
      try {
        const res = await apiFetch("/api/entries/reset-all", { method: "POST" });
        updateCountsUI(res.counts);
        const banner = document.getElementById("refresh-summary-banner");
        if (banner) {
          banner.classList.add("hidden");
          banner.textContent = "";
        }
        await Promise.all([loadActiveEntries(), loadArchivedEntries(), loadFeeds()]);
        showToast(
          `Reset complete: removed ${res.deleted_count} RSS entries. Ready to re-fetch and re-score from scratch.`,
          "info"
        );
      } catch (err) {
        showToast(err.message, "error");
      } finally {
        resetAllBtn.disabled = false;
      }
    });
  }

  // Search & Journal Multi-Select Filters
  document.getElementById("active-search-input").addEventListener("input", () => loadActiveEntries());
  document.getElementById("archive-search-input").addEventListener("input", () => loadArchivedEntries());
  initJournalMultiselect("active");
  initJournalMultiselect("archive");

  document.addEventListener("pointerdown", (e) => {
    ["active", "archive"].forEach((scope) => {
      const cfg = getJournalFilterConfig(scope);
      if (cfg.panel && !cfg.panel.classList.contains("hidden") && cfg.wrapper && !cfg.wrapper.contains(e.target)) {
        cfg.panel.classList.add("hidden");
        cfg.triggerBtn?.setAttribute("aria-expanded", "false");
      }
    });
  });

  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      ["active", "archive"].forEach((scope) => {
        const cfg = getJournalFilterConfig(scope);
        if (cfg.panel && !cfg.panel.classList.contains("hidden")) {
          cfg.panel.classList.add("hidden");
          cfg.triggerBtn?.setAttribute("aria-expanded", "false");
          cfg.triggerBtn?.focus();
        }
      });
    }
  });

  // Bulk Archive Below Threshold
  const bulkThresholdInput = document.getElementById("bulk-threshold-input");
  if (bulkThresholdInput) {
    bulkThresholdInput.addEventListener("input", () => {
      renderJournalFilterOptions("active");
      renderJournalFilterOptions("archive");
    });
  }

  document.getElementById("btn-archive-below").addEventListener("click", async () => {
    const thresh = Number(document.getElementById("bulk-threshold-input").value || 0);
    const toArchive = state.activeEntries
      .filter((e) => e.is_scored && Number(e.relevance_pct) < thresh)
      .map((e) => e.id);
    if (toArchive.length === 0) {
      showToast(`No scored active entries below ${thresh}%.`, "info");
      return;
    }
    const res = await apiFetch("/api/entries/archive-bulk", {
      method: "POST",
      body: JSON.stringify({ entry_ids: toArchive }),
    });
    showToast(`Archived ${res.archived_count} entries below ${thresh}%.`, "success");
    await Promise.all([loadActiveEntries(), loadFeeds()]);
  });

  // Delegated click handlers for Active & Archived entry lists
  document.body.addEventListener("click", async (e) => {
    const archiveBtn = e.target.closest(".btn-archive-entry");
    if (archiveBtn) {
      const id = Number(archiveBtn.dataset.id);
      await apiFetch(`/api/entries/${id}/archive`, { method: "POST" });
      showToast("Entry moved to Archive.", "info");
      await Promise.all([loadActiveEntries(), loadFeeds()]);
      return;
    }

    const unarchiveBtn = e.target.closest(".btn-unarchive-entry");
    if (unarchiveBtn) {
      const id = Number(unarchiveBtn.dataset.id);
      await apiFetch(`/api/entries/${id}/unarchive`, { method: "POST" });
      showToast("Entry restored to Ranked Feed.", "info");
      await Promise.all([loadArchivedEntries(), loadFeeds()]);
      return;
    }

    const sendTesterBtn = e.target.closest(".btn-send-to-tester");
    if (sendTesterBtn) {
      const id = Number(sendTesterBtn.dataset.id);
      const isArch = sendTesterBtn.dataset.archived === "1";
      const list = isArch ? state.archivedEntries : state.activeEntries;
      const entry = list.find((item) => item.id === id);
      if (entry) {
        switchTab("tab-queries");
        document.getElementById("tester-entry-title").value = entry.title || "";
        document.getElementById("tester-entry-summary").value = entry.summary || "";
        showToast("Loaded entry into Individual Jev Query Tester.", "info");
      }
      return;
    }

    const previewEntryTesterBtn = e.target.closest(".btn-use-preview-entry");
    if (previewEntryTesterBtn) {
      switchTab("tab-queries");
      document.getElementById("tester-entry-title").value = previewEntryTesterBtn.dataset.title || "";
      document.getElementById("tester-entry-summary").value = previewEntryTesterBtn.dataset.summary || "";
      showToast("Loaded preview entry into Individual Jev Query Tester.", "info");
      return;
    }
  });

  // --- RSS Feeds Tab Controls ---
  const newFeedForm = document.getElementById("new-feed-form");
  document.getElementById("btn-toggle-new-feed").addEventListener("click", () => {
    newFeedForm.classList.toggle("hidden");
  });
  document.getElementById("btn-cancel-new-feed").addEventListener("click", () => {
    newFeedForm.classList.add("hidden");
  });

  document.getElementById("btn-preview-new-feed").addEventListener("click", async () => {
    const url = document.getElementById("new-feed-url").value.trim();
    if (!url) {
      showToast("Enter a feed URL to test.", "error");
      return;
    }
    const box = document.getElementById("new-feed-preview-box");
    box.classList.remove("hidden");
    box.innerHTML = `<p class="muted-label">⏳ Fetching and parsing RSS feed...</p>`;
    try {
      const data = await apiFetch("/api/feeds/test", {
        method: "POST",
        body: JSON.stringify({ url, max_entries: 10 }),
      });
      const titleInput = document.getElementById("new-feed-title");
      if (!titleInput.value.trim() && data.feed_title) {
        titleInput.value = data.feed_title;
      }
      box.innerHTML = renderFeedPreviewHtml(data);
    } catch (err) {
      box.innerHTML = `<p style="color:var(--danger)">${escapeHtml(err.message)}</p>`;
    }
  });

  newFeedForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    const title = document.getElementById("new-feed-title").value.trim();
    const url = document.getElementById("new-feed-url").value.trim();
    const category = document.getElementById("new-feed-category").value.trim() || "General";
    const tags = parseTagsInput(document.getElementById("new-feed-tags")?.value || "");
    try {
      await apiFetch("/api/feeds", {
        method: "POST",
        body: JSON.stringify({ title, url, category, tags, enabled: true }),
      });
      newFeedForm.reset();
      newFeedForm.classList.add("hidden");
      document.getElementById("new-feed-preview-box").classList.add("hidden");
      await loadFeeds();
      await loadSettings();
      showToast("Added RSS feed!", "success");
    } catch (err) {
      showToast(err.message, "error");
    }
  });

  // Delegated events inside Expandable Feeds List
  const feedsListEl = document.getElementById("feeds-expandable-list");
  feedsListEl.addEventListener("change", async (e) => {
    if (e.target.classList.contains("feed-enable-toggle")) {
      const id = Number(e.target.dataset.id);
      const enabled = e.target.checked;
      await apiFetch(`/api/feeds/${id}`, {
        method: "PUT",
        body: JSON.stringify({ enabled }),
      });
      await loadFeeds();
      showToast(`Feed ${enabled ? "enabled" : "disabled"}.`, "info");
    }
  });

  feedsListEl.addEventListener("click", async (e) => {
    const testBtn = e.target.closest(".btn-test-existing-feed");
    if (testBtn) {
      e.preventDefault();
      e.stopPropagation();
      const id = Number(testBtn.dataset.id);
      const detailsEl = testBtn.closest("details.expandable-item");
      if (detailsEl) detailsEl.open = true;
      const urlInput = detailsEl ? detailsEl.querySelector('input[name="url"]') : null;
      const currentUrl = urlInput ? urlInput.value.trim() : null;
      const box = document.getElementById(`feed-preview-box-${id}`);
      box.classList.remove("hidden");
      box.innerHTML = `<p class="muted-label">⏳ Fetching live RSS feed preview...</p>`;
      try {
        const data = await apiFetch("/api/feeds/test", {
          method: "POST",
          body: JSON.stringify({ feed_id: id, url: currentUrl || undefined, max_entries: 10 }),
        });
        box.innerHTML = renderFeedPreviewHtml(data);
      } catch (err) {
        box.innerHTML = `<p style="color:var(--danger)">${escapeHtml(err.message)}</p>`;
      }
      return;
    }

    const delBtn = e.target.closest(".btn-delete-feed");
    if (delBtn) {
      e.preventDefault();
      const id = Number(delBtn.dataset.id);
      await apiFetch(`/api/feeds/${id}`, { method: "DELETE" });
      await loadFeeds();
      await loadSettings();
      showToast("Feed removed.", "info");
    }
  });

  feedsListEl.addEventListener("submit", async (e) => {
    const form = e.target.closest(".edit-feed-form");
    if (!form) return;
    e.preventDefault();
    const id = Number(form.dataset.id);
    const fd = new FormData(form);
    try {
      await apiFetch(`/api/feeds/${id}`, {
        method: "PUT",
        body: JSON.stringify({
          title: fd.get("title"),
          url: fd.get("url"),
          category: fd.get("category"),
          tags: parseTagsInput(fd.get("tags") || ""),
        }),
      });
      await loadFeeds();
      showToast("Feed updated!", "success");
    } catch (err) {
      showToast(err.message, "error");
    }
  });

  // --- Jev Queries Tab Controls ---
  const newQueryForm = document.getElementById("new-query-form");
  const newQueryTypeSelect = document.getElementById("new-query-type");
  const newLevelsList = document.getElementById("new-query-levels-list");
  const DEFAULT_NEW_SCORE_LEVEL_PLACEHOLDERS = [
    "e.g. No mention or unrelated to the target methodology",
    "e.g. Briefly mentions or uses standard off-the-shelf methods",
    "e.g. Primary focus introducing or deeply applying the methodology",
  ];

  function syncNewQueryTypeUI() {
    const isScore = newQueryTypeSelect.value === "score";
    document.getElementById("new-query-noul-criteria").classList.toggle("hidden", isScore);
    document.getElementById("new-query-score-criteria").classList.toggle("hidden", !isScore);
    newLevelsList.querySelectorAll(".score-level-input").forEach((inp) => {
      inp.required = isScore;
    });
  }

  function resetNewScoreLevels() {
    const isScore = newQueryTypeSelect.value === "score";
    newLevelsList.innerHTML = renderScoreLevelsInputs(DEFAULT_NEW_SCORE_LEVEL_PLACEHOLDERS, {
      asPlaceholders: true,
      required: isScore,
    });
  }

  resetNewScoreLevels();
  syncNewQueryTypeUI();

  document.getElementById("btn-toggle-new-query").addEventListener("click", () => {
    newQueryForm.classList.toggle("hidden");
  });
  document.getElementById("btn-cancel-new-query").addEventListener("click", () => {
    newQueryForm.classList.add("hidden");
  });

  newQueryTypeSelect.addEventListener("change", syncNewQueryTypeUI);

  document.getElementById("btn-add-new-score-level").addEventListener("click", () => {
    const count = newLevelsList.querySelectorAll(".level-row").length;
    const isScore = newQueryTypeSelect.value === "score";
    const wrapper = document.createElement("div");
    wrapper.className = "level-row";
    wrapper.innerHTML = `
      <span class="level-index">Level ${count + 1}</span>
      <input type="text" class="input score-level-input" placeholder="Describe level ${count + 1}..." ${isScore ? "required" : ""} />
      <button type="button" class="btn btn-xs btn-ghost btn-remove-level">✕</button>
    `;
    newLevelsList.appendChild(wrapper);
  });

  newQueryForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    const name = document.getElementById("new-query-name").value.trim();
    const query_type = newQueryTypeSelect.value;
    const weight = Number(document.getElementById("new-query-weight").value || 1.0);
    const instructions = document.getElementById("new-query-instructions").value.trim();

    let criteria = {};
    if (query_type === "score") {
      criteria = Array.from(newLevelsList.querySelectorAll(".score-level-input"))
        .map((inp) => inp.value.trim())
        .filter(Boolean);
      if (criteria.length < 2) {
        showToast("Please fill in at least 2 rubric levels.", "error");
        return;
      }
    } else {
      const t = document.getElementById("new-query-true-desc").value.trim();
      const f = document.getElementById("new-query-false-desc").value.trim();
      if (t || f) criteria = { true: t, false: f };
    }

    try {
      await apiFetch("/api/queries", {
        method: "POST",
        body: JSON.stringify({ name, query_type, instructions, criteria, weight, enabled: true }),
      });
      newQueryForm.reset();
      resetNewScoreLevels();
      syncNewQueryTypeUI();
      newQueryForm.classList.add("hidden");
      await loadQueries();
      await loadActiveEntries();
      await loadSettings();
      showToast("Created Jev query!", "success");
    } catch (err) {
      showToast(err.message, "error");
    }
  });

  // Delegated events for Expandable Queries List
  const queriesListEl = document.getElementById("queries-expandable-list");

  queriesListEl.addEventListener("change", async (e) => {
    if (e.target.classList.contains("query-enable-toggle")) {
      const id = Number(e.target.dataset.id);
      const enabled = e.target.checked;
      await apiFetch(`/api/queries/${id}`, {
        method: "PUT",
        body: JSON.stringify({ enabled }),
      });
      await loadQueries();
      await loadActiveEntries();
      showToast(`Query ${enabled ? "enabled" : "disabled"} — rankings updated.`, "info");
      return;
    }

    if (e.target.classList.contains("query-quick-weight")) {
      const id = Number(e.target.dataset.id);
      const weight = Number(e.target.value || 0);
      await apiFetch(`/api/queries/${id}`, {
        method: "PUT",
        body: JSON.stringify({ weight }),
      });
      await loadQueries();
      await loadActiveEntries();
      showToast(`Updated weight to ${weight} — rankings recalculated!`, "success");
      return;
    }

    if (e.target.classList.contains("edit-query-type-select")) {
      const form = e.target.closest(".edit-query-form");
      const isScore = e.target.value === "score";
      form.querySelector(".edit-noul-box").classList.toggle("hidden", isScore);
      form.querySelector(".edit-score-box").classList.toggle("hidden", !isScore);
      form.querySelectorAll(".edit-score-box .score-level-input").forEach((inp) => {
        inp.required = isScore;
      });
    }
  });

  queriesListEl.addEventListener("click", async (e) => {
    const selectTesterBtn = e.target.closest(".btn-select-query-in-tester");
    if (selectTesterBtn) {
      e.preventDefault();
      e.stopPropagation();
      const qid = selectTesterBtn.dataset.id;
      document.getElementById("tester-query-select").value = qid;
      showToast("Selected query in Individual Jev Tester.", "info");
      return;
    }

    const addLevelBtn = e.target.closest(".btn-add-edit-level");
    if (addLevelBtn) {
      const container = addLevelBtn.closest(".edit-score-box").querySelector(".edit-levels-container");
      const count = container.querySelectorAll(".level-row").length;
      const row = document.createElement("div");
      row.className = "level-row";
      row.innerHTML = `
        <span class="level-index">Level ${count + 1}</span>
        <input type="text" class="input score-level-input" placeholder="Describe level ${count + 1}..." required />
        <button type="button" class="btn btn-xs btn-ghost btn-remove-level">✕</button>
      `;
      container.appendChild(row);
      return;
    }

    const delQueryBtn = e.target.closest(".btn-delete-query");
    if (delQueryBtn) {
      const id = Number(delQueryBtn.dataset.id);
      await apiFetch(`/api/queries/${id}`, { method: "DELETE" });
      await loadQueries();
      await loadActiveEntries();
      await loadSettings();
      showToast("Deleted Jev query.", "info");
    }
  });

  document.body.addEventListener("click", (e) => {
    const removeLvlBtn = e.target.closest(".btn-remove-level");
    if (removeLvlBtn) {
      const list = removeLvlBtn.closest(".levels-list");
      if (list.querySelectorAll(".level-row").length <= 2) {
        showToast("A score query requires at least 2 rubric levels.", "error");
        return;
      }
      removeLvlBtn.closest(".level-row").remove();
      list.querySelectorAll(".level-row").forEach((r, i) => {
        r.querySelector(".level-index").textContent = `Level ${i + 1}`;
      });
    }
  });

  queriesListEl.addEventListener("submit", async (e) => {
    const form = e.target.closest(".edit-query-form");
    if (!form) return;
    e.preventDefault();
    const id = Number(form.dataset.id);
    const fd = new FormData(form);
    const query_type = fd.get("query_type");
    let criteria = {};

    if (query_type === "score") {
      criteria = Array.from(form.querySelectorAll(".score-level-input"))
        .map((inp) => inp.value.trim())
        .filter(Boolean);
    } else {
      const t = (fd.get("true_desc") || "").trim();
      const f = (fd.get("false_desc") || "").trim();
      if (t || f) criteria = { true: t, false: f };
    }

    try {
      await apiFetch(`/api/queries/${id}`, {
        method: "PUT",
        body: JSON.stringify({
          name: fd.get("name"),
          query_type,
          weight: Number(fd.get("weight") || 1.0),
          instructions: fd.get("instructions"),
          criteria,
        }),
      });
      await loadQueries();
      await loadActiveEntries();
      showToast("Saved Jev query & updated rankings!", "success");
    } catch (err) {
      showToast(err.message, "error");
    }
  });

  // --- Single-Query Jev Tester Controls ---
  document.getElementById("btn-tester-load-feed").addEventListener("click", loadEntriesForTesterFeed);
  document.getElementById("tester-feed-select").addEventListener("change", loadEntriesForTesterFeed);
  document.getElementById("tester-entry-select").addEventListener("change", (e) => {
    selectTesterEntryByIndex(e.target.value);
  });
  document.getElementById("btn-run-jev-test").addEventListener("click", runSingleJevTest);

  // --- Settings Form & Provider Controls ---
  const settingProvSelect = document.getElementById("setting-ai-provider");
  if (settingProvSelect) {
    settingProvSelect.addEventListener("change", () => {
      updateSettingsProviderPreview(settingProvSelect.value, true);
    });
  }

  ["setting-provider-api-url", "setting-custom-provider-name", "setting-custom-portal-url"].forEach(
    (id) => {
      const inp = document.getElementById(id);
      if (inp) {
        inp.addEventListener("input", () => {
          const provId = document.getElementById("setting-ai-provider")?.value || "openrouter";
          updateSettingsProviderPreview(provId, false);
        });
      }
    }
  );

  const settingOpenPortalBtn = document.getElementById("btn-setting-open-portal");
  if (settingOpenPortalBtn) {
    settingOpenPortalBtn.addEventListener("click", () => {
      const provId = document.getElementById("setting-ai-provider")?.value || "openrouter";
      const customPortal = (document.getElementById("setting-custom-portal-url")?.value || "").trim();
      const spec = findProviderSpec(provId);
      const url = customPortal || spec.portal_url || "";
      openExternalUrl(url);
    });
  }

  const openCustomPortalBtn = document.getElementById("btn-open-custom-portal-url");
  if (openCustomPortalBtn) {
    openCustomPortalBtn.addEventListener("click", () => {
      const url = (document.getElementById("setting-custom-portal-url")?.value || "").trim();
      openExternalUrl(url);
    });
  }

  const manageKeyBtn = document.getElementById("btn-settings-manage-key");
  if (manageKeyBtn) {
    manageKeyBtn.addEventListener("click", () => {
      const provId = document.getElementById("setting-ai-provider")?.value || state.settings.ai_provider || "openrouter";
      syncSetupModalProvider(provId, false);
      openAuthModal(true);
    });
  }

  const fetchModelsBtn = document.getElementById("btn-fetch-provider-models");
  if (fetchModelsBtn) {
    fetchModelsBtn.addEventListener("click", async () => {
      const provider_id = document.getElementById("setting-ai-provider")?.value || "openrouter";
      const provider_api_url = (document.getElementById("setting-provider-api-url")?.value || "").trim();
      const origText = fetchModelsBtn.textContent;
      fetchModelsBtn.disabled = true;
      fetchModelsBtn.textContent = "⏳ Fetching...";
      try {
        const res = await apiFetch("/api/providers/models", {
          method: "POST",
          body: JSON.stringify({ provider_id, provider_api_url: provider_api_url || undefined }),
        });
        const models = res.models || [];
        populateModelDatalist(models);
        if (models.length > 0 && !document.getElementById("setting-jev-model").value.trim()) {
          document.getElementById("setting-jev-model").value = models[0];
        }
        showToast(
          `Loaded ${models.length} decision model(s) (${res.source === "live_endpoint" ? "live /models API" : "curated catalog"}).`,
          "success"
        );
      } catch (err) {
        showToast(err.message, "error");
      } finally {
        fetchModelsBtn.disabled = false;
        fetchModelsBtn.textContent = origText;
      }
    });
  }

  document.getElementById("settings-form").addEventListener("submit", async (e) => {
    e.preventDefault();
    const ai_provider = document.getElementById("setting-ai-provider")?.value || "openrouter";
    const provider_api_url = (document.getElementById("setting-provider-api-url")?.value || "").trim();
    const provider_wire_format = getSelectedRadioValue("provider_wire_format", "systemone");
    const jev_model = document.getElementById("setting-jev-model").value.trim();
    const custom_provider_name = (document.getElementById("setting-custom-provider-name")?.value || "").trim();
    const custom_portal_url = (document.getElementById("setting-custom-portal-url")?.value || "").trim();
    const max_entries_per_feed = Number(document.getElementById("setting-max-entries").value || 25);

    try {
      await apiFetch("/api/settings", {
        method: "PUT",
        body: JSON.stringify({
          ai_provider,
          provider_api_url,
          provider_wire_format,
          jev_model,
          custom_provider_name,
          custom_portal_url,
          max_entries_per_feed,
        }),
      });
      await Promise.all([loadSettings(), checkAuthStatus(false)]);
      showToast("Decision Provider, URL & Model saved!", "success");
    } catch (err) {
      showToast(err.message, "error");
    }
  });
}

async function initApp() {
  initEventListeners();
  await loadSettings();
  await Promise.all([
    loadWorkspaceInfo(),
    checkAuthStatus(true),
    loadFeeds(),
    loadQueries(),
    loadActiveEntries(),
  ]);
}

document.addEventListener("DOMContentLoaded", initApp);
