// serial_nexus web console client (design §17). A pure browser client of the
// server's WebSocket, which is itself a filtering proxy of the daemon's JSON-RPC
// surface (§10). The per-session token rides the same-origin cookie the bootstrap
// URL set (§15.29), so this code never sees it. The layout is the contract; this
// rendering iterates freely (§15.16).
//
// Scrollback beyond the daemon's replay ring lives here, in the browser (§11.9): each
// console's hostward stream is folded by monotonic byte offset (§11.8) into a capped
// history persisted in the Origin Private File System, keyed by the web origin, the
// endpoint, and the daemon `instance` nonce — so a reload trims the ring overlap exactly
// and a daemon restart starts fresh. The splice/retention math is history.mjs (unit
// tested); OPFS I/O is opfs.mjs; both degrade to memory-only where OPFS is absent.
// (ES modules are always strict, so no "use strict" pragma is needed.)

import { newHistory, fromStored, splice, bytesOf } from "/history.mjs";
import { opfsAvailable, requestPersistence, load, save, clear } from "/opfs.mjs";

const consolesEl = document.getElementById("consoles");
const connEl = document.getElementById("conn");
const termEl = document.getElementById("term");
const titleEl = document.getElementById("pane-title");
const lockEl = document.getElementById("pane-lock");
const dropsEl = document.getElementById("pane-drops");
const storageEl = document.getElementById("pane-storage");
const exportBtn = document.getElementById("exportbtn");
const clearBtn = document.getElementById("clearbtn");
const sendForm = document.getElementById("sendform");
const sendLine = document.getElementById("sendline");
const sendBtn = document.getElementById("sendbtn");

let ws = null;
let nextId = 1;
const pending = new Map();          // id -> resolve
let selected = null;                // selected endpoint display
let currentTap = null;              // active tap id
let lastState = { nodes: [], taps: [] };
let decoder = new TextDecoder("utf-8", { fatal: false });

let instanceNonce = null;           // daemon per-boot nonce (§11.8); history reset key
let opfsOk = opfsAvailable();       // false → memory-only fallback
let persistStatus = "unavailable";  // persisted | best-effort | unavailable
let history = null;                 // current console's ConsoleHistory (history.mjs)
let historyKey = null;              // OPFS key for the current console
let saveTimer = null;               // debounced persist handle

function rpc(method, params) {
  return new Promise((resolve) => {
    const id = nextId++;
    pending.set(id, resolve);
    ws.send(JSON.stringify({ jsonrpc: "2.0", id, method, params: params || null }));
  });
}

function connect() {
  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  ws = new WebSocket(`${proto}//${location.host}/ws`);
  ws.onopen = async () => {
    connEl.textContent = "connected";
    connEl.className = "connected";
    await rpc("subscribe", null);   // stream state / lock / tap.data
    const info = await rpc("info", null);
    if (info) instanceNonce = info.instance;
    if (opfsOk) persistStatus = await requestPersistence();
    renderStorageBadge();
    refreshState();
  };
  ws.onclose = () => {
    connEl.textContent = "disconnected — reload to reconnect";
    connEl.className = "disconnected";
    sendLine.disabled = sendBtn.disabled = true;
    flushSave();
  };
  ws.onmessage = (ev) => onMessage(ev.data);
}

async function refreshState() {
  const st = await rpc("state", null);
  if (st) { lastState = st; renderConsoles(); }
}

function onMessage(text) {
  let msg;
  try { msg = JSON.parse(text); } catch { return; }
  if (msg.id !== undefined && (msg.result !== undefined || msg.error !== undefined)) {
    const cb = pending.get(msg.id);
    if (cb) { pending.delete(msg.id); cb(msg.error ? null : msg.result); }
    return;
  }
  // id-less notification
  switch (msg.method) {
    case "state": lastState = msg.params; renderConsoles(); break;
    case "lock": renderConsoles(); break;
    case "tap.data": onTapData(msg.params); break;
  }
}

// Every host-facing endpoint is a console: a serial node's default endpoint, or a
// codec/leg channel. Derive the list from `state` (§17 left rail).
function endpointsFromState(st) {
  const out = [];
  for (const n of st.nodes || []) {
    if (n.lock) out.push({ display: n.name, node: n, lock: n.lock });
    for (const [ch, cv] of Object.entries(n.channels || {})) {
      if (cv.lock) out.push({ display: `${n.name}/${ch}`, node: n, lock: cv.lock });
    }
  }
  return out;
}

function renderConsoles() {
  const eps = endpointsFromState(lastState);
  consolesEl.innerHTML = "";
  for (const ep of eps) {
    const li = document.createElement("li");
    li.className = ep.display === selected ? "console selected" : "console";
    const name = document.createElement("span");
    name.className = "cname";
    name.textContent = ep.display;
    li.appendChild(name);
    if (ep.lock && ep.lock.holder) {
      const badge = document.createElement("span");
      badge.className = "lockbadge";
      badge.textContent = `🔒 ${ep.lock.holder}`;
      li.appendChild(badge);
    }
    const waiters = ep.lock && ep.lock.waiters ? ep.lock.waiters.length : 0;
    if (waiters > 0) {
      const w = document.createElement("span");
      w.className = "waiters";
      w.textContent = `+${waiters}`;
      li.appendChild(w);
    }
    li.onclick = () => selectConsole(ep.display);
    consolesEl.appendChild(li);
  }
  updateHead();
}

function updateHead() {
  titleEl.textContent = selected || "select a console";
  const ep = endpointsFromState(lastState).find((e) => e.display === selected);
  lockEl.textContent = ep && ep.lock && ep.lock.holder ? `locked by ${ep.lock.holder}` : "";
  const tap = (lastState.taps || []).find((t) => t.tap === currentTap);
  const dropped = tap ? (tap.dropped || 0) + (tap.feed_dropped || 0) : 0;
  dropsEl.textContent = dropped > 0 ? `⚠ ${dropped} tap bytes dropped` : "";
  sendLine.disabled = sendBtn.disabled = !selected;
  exportBtn.disabled = clearBtn.disabled = !selected;
}

function renderStorageBadge() {
  if (!opfsOk) { storageEl.textContent = "history: memory only"; return; }
  storageEl.textContent = `history: OPFS (${persistStatus})`;
}

// The OPFS key isolates history per daemon and per boot: the web origin (a stable
// host:port, §15.32) stands in for the socket path, plus the endpoint and the daemon
// instance nonce so a restart never splices across reset offsets.
function keyFor(display) {
  return `${location.host}::${display}::${instanceNonce ?? "unknown"}`;
}

async function selectConsole(display) {
  flushSave();
  if (currentTap !== null) { await rpc("tap.close", { tap: currentTap }); currentTap = null; }
  selected = display;
  decoder = new TextDecoder("utf-8", { fatal: false });
  termEl.textContent = "";
  historyKey = keyFor(display);

  // Restore persisted scrollback (if any) before the ring replay, so the frontier trims
  // the ring's overlap and the terminal shows history-then-ring-then-live contiguously.
  let stored = null;
  if (opfsOk) { try { stored = await load(historyKey); } catch { stored = null; } }
  if (stored) {
    history = fromStored(stored.bytes, stored.endOffset);
    appendMarker(`— stored history (${stored.bytes.length} bytes) —\n`);
    appendText(decoder.decode(stored.bytes, { stream: true }));
  } else {
    history = newHistory();
  }

  const res = await rpc("tap.open", { endpoint: display, replay: true });
  if (res) {
    currentTap = res.tap;
    // The tap's stream begins at res.from_offset; if we restored nothing, anchor the
    // history there so the first live chunk is not mistaken for offset 0.
    if (!stored) history.frontier = res.from_offset;
    if (res.replay_bytes > 0) appendMarker(`— replay (${res.replay_bytes} bytes) —\n`);
    else if (!stored) appendMarker("— no history (set replay_ring to keep scrollback) —\n");
  }
  renderConsoles();
}

function onTapData(params) {
  if (!params || params.tap !== currentTap || !history) return;
  const bin = atob(params.data);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  // Fold by offset: overlap the ring re-sent (or already-stored bytes) is trimmed, so a
  // reconnect never double-renders or double-stores. Only the fresh tail is shown.
  const fresh = splice(history, params.offset ?? history.frontier ?? 0, bytes);
  if (fresh.length) {
    appendText(decoder.decode(fresh, { stream: true }));
    scheduleSave();
  }
}

function scheduleSave() {
  if (!opfsOk || !historyKey || saveTimer) return;
  // Debounce: snapshot the capped buffer at most every second, not per chunk.
  saveTimer = setTimeout(() => { saveTimer = null; flushSave(); }, 1000);
}

function flushSave() {
  if (saveTimer) { clearTimeout(saveTimer); saveTimer = null; }
  if (!opfsOk || !historyKey || !history || history.frontier === null) return;
  const key = historyKey;
  save(key, bytesOf(history), history.frontier).catch(() => {
    // A storage error drops us to best-effort visibly rather than silently.
    opfsOk = false;
    renderStorageBadge();
  });
}

function appendText(s) {
  const atBottom = termEl.scrollTop + termEl.clientHeight >= termEl.scrollHeight - 4;
  termEl.appendChild(document.createTextNode(s));
  if (atBottom) termEl.scrollTop = termEl.scrollHeight;
}

function appendMarker(s) {
  const span = document.createElement("span");
  span.className = "marker";
  span.textContent = s;
  termEl.appendChild(span);
  termEl.scrollTop = termEl.scrollHeight;
}

exportBtn.onclick = () => {
  if (!history) return;
  const blob = new Blob([bytesOf(history)], { type: "application/octet-stream" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `${(selected || "console").replace(/[^A-Za-z0-9._-]/g, "_")}.log`;
  a.click();
  URL.revokeObjectURL(url);
};

clearBtn.onclick = async () => {
  if (!selected) return;
  if (!confirm(`Clear stored scrollback for ${selected}?`)) return;
  if (opfsOk && historyKey) { try { await clear(historyKey); } catch { /* ignore */ } }
  // Keep the live frontier so the ongoing stream is not re-duplicated after a clear.
  const frontier = history ? history.frontier : null;
  history = fromStored(new Uint8Array(0), frontier ?? 0);
  termEl.textContent = "";
  appendMarker("— history cleared —\n");
};

// Persist a final snapshot when the tab is hidden or closed (a reload otherwise loses the
// last debounce window).
window.addEventListener("visibilitychange", () => { if (document.hidden) flushSave(); });
window.addEventListener("pagehide", flushSave);

sendForm.onsubmit = async (e) => {
  e.preventDefault();
  if (!selected) return;
  const line = sendLine.value;
  sendLine.value = "";
  const res = await rpc("send", { endpoint: selected, line, steal: false });
  if (res === null) {
    // Locked (or another error): offer an explicit steal, never automatic (§17).
    const ep = endpointsFromState(lastState).find((x) => x.display === selected);
    const holder = ep && ep.lock ? ep.lock.holder : "someone";
    if (confirm(`${selected} is locked by ${holder}. Steal the lock and send?`)) {
      await rpc("send", { endpoint: selected, line, steal: true });
    }
  }
};

connect();
