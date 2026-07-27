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

import {
  newHistory,
  fromStored,
  splice,
  bytesOf,
  offsetSpaceChanged,
  reanchor,
} from "/history.mjs";
import { makeSaver } from "/saver.mjs";
import { opfsAvailable, requestPersistence, load, save, clear } from "/opfs.mjs";
import { model as graphModel, render as renderGraph } from "/graph.mjs";
import { render as renderEditor } from "/editor.mjs";

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
const graphViewEl = document.getElementById("graphview");
const editorViewEl = document.getElementById("editorview");
const viewBtns = Array.from(document.querySelectorAll(".viewbtn"));

let ws = null;
let nextId = 1;
const pending = new Map();          // id -> resolve
const pendingFull = new Set();      // ids whose caller wants the whole envelope
let selected = null;                // selected endpoint display
let currentTap = null;              // active tap id
let lastState = { nodes: [], taps: [] };
let decoder = new TextDecoder("utf-8", { fatal: false });

let instanceNonce = null;           // daemon per-boot nonce (§11.8); history reset key
let opfsOk = opfsAvailable();       // false → memory-only fallback
let persistStatus = "unavailable";  // persisted | best-effort | unavailable
let storageError = null;            // last persistence failure, shown in the badge
let history = null;                 // current console's ConsoleHistory (history.mjs)
let historyKey = null;              // OPFS key for the current console — set with `history`
let historyEpoch = 0;               // the tap's offset-space epoch (§11.8), stored with it
let saveTimer = null;               // debounced persist handle
let selectGen = 0;                  // selectConsole re-entrancy generation (see below)
let view = "console";               // console | graph | editor (§17, §15.35)
let lastDump = { node: [], edge: [] };
let lastPorts = [];

// At most one OPFS write in flight per key, newest snapshot wins: `createWritable()`
// truncates, so two overlapping full-buffer rewrites of one console's scrollback let the
// second truncate into the middle of the first. Failures are surfaced, not swallowed.
const saver = makeSaver(save, (err) => storageFailed(err));

// `WebSocket.send` on a closed socket does not throw — it discards — so an
// unguarded call leaves its promise pending forever and leaks the id. The view and
// editor controls are live from the first paint, before `onopen` and after a drop,
// so both senders check first and settle with a refusal the caller can render.
function socketReady() {
  return ws !== null && ws.readyState === WebSocket.OPEN;
}

function rpc(method, params) {
  if (!socketReady()) return Promise.resolve(null);
  return new Promise((resolve) => {
    const id = nextId++;
    pending.set(id, resolve);
    ws.send(JSON.stringify({ jsonrpc: "2.0", id, method, params: params || null }));
  });
}

// The editor needs the daemon's *words*, not just "it failed": a structural refusal
// names the rule that was broken (§4/§11), and that is the only precise thing the
// operator gets. `rpc` keeps its null-on-error shape for the console paths that only
// branch on success; this one hands back the whole envelope.
function rpcFull(method, params) {
  if (!socketReady()) {
    return Promise.resolve({
      error: { code: 0, message: "not connected to the daemon — reload to reconnect" },
    });
  }
  return new Promise((resolve) => {
    const id = nextId++;
    pending.set(id, resolve);
    pendingFull.add(id);
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
    // Settle everything still in flight: an unsettled promise is a caller parked
    // forever, and `pending` would grow for the rest of the page's life.
    for (const [id, cb] of pending) {
      cb(pendingFull.delete(id) ? { error: { code: 0, message: "connection closed" } } : null);
    }
    pending.clear();
  };
  ws.onmessage = (ev) => onMessage(ev.data);
}

async function refreshState() {
  const st = await rpc("state", null);
  if (st) { lastState = st; renderConsoles(); }
  if (view !== "console") await refreshGraph();
}

// Topology comes from `dump` (configuration) and status from `state` (observed) —
// the §15.8 split, kept in the client too. `ports` is only needed by the editor's
// serial palette entry, so it is fetched with the rest rather than on every tick.
async function refreshGraph() {
  const dump = await rpc("dump", null);
  if (dump) lastDump = dump;
  if (view === "editor") {
    const p = await rpc("ports", null);
    if (p && p.ports) lastPorts = p.ports;
  }
  renderView();
}

function renderView() {
  graphViewEl.hidden = view !== "graph";
  editorViewEl.hidden = view !== "editor";
  // The console view is the pane's original chrome; hide it as one unit.
  for (const el of [termEl, sendForm, document.getElementById("pane-head")]) {
    el.hidden = view !== "console";
  }
  for (const b of viewBtns) b.classList.toggle("active", b.dataset.view === view);
  if (view === "graph") renderGraph(graphViewEl, graphModel(lastDump, lastState));
  if (view === "editor") {
    renderEditor(editorViewEl, {
      dump: lastDump,
      ports: lastPorts,
      rpc: rpcFull,
      confirm: (t) => window.confirm(t),
      refresh: () => refreshGraph(),
    });
  }
}

// Refetch `dump` at most once a second while the graph view is live, so a 5 Hz
// state stream costs one extra RPC per second rather than five.
let topologyAt = 0;
function refreshTopology() {
  const now = Date.now();
  if (now - topologyAt < 1000) {
    renderView();
    return;
  }
  topologyAt = now;
  refreshGraph();
}

function selectView(next) {
  // Assigning `location.hash` fires `hashchange`, whose listener calls back in
  // here; without this guard one click issues its RPCs twice and rebuilds the
  // editor's DOM under itself.
  if (view === next) return;
  view = next;
  location.hash = next === "console" ? "" : `#${next}`;
  if (next === "console") {
    renderView();
    // `#term` was `display:none` while another view showed, which zeroes its
    // scroll offset and makes every follow-the-tail check fail. Re-anchor it, or
    // the terminal never auto-scrolls again for the rest of the session.
    termEl.scrollTop = termEl.scrollHeight;
  } else {
    refreshGraph();
  }
}

for (const b of viewBtns) b.onclick = () => selectView(b.dataset.view);
window.addEventListener("hashchange", () => {
  const h = location.hash.replace("#", "");
  selectView(h === "graph" || h === "editor" ? h : "console");
});

function onMessage(text) {
  let msg;
  try { msg = JSON.parse(text); } catch { return; }
  if (msg.id !== undefined && (msg.result !== undefined || msg.error !== undefined)) {
    const cb = pending.get(msg.id);
    if (cb) {
      pending.delete(msg.id);
      const full = pendingFull.delete(msg.id);
      cb(full ? msg : (msg.error ? null : msg.result));
    }
    return;
  }
  // id-less notification
  switch (msg.method) {
    // Status indicators are live off `subscribe` (§17): a node flipping to
    // `waiting` repaints the graph page without a poll.
    case "state":
      lastState = msg.params;
      renderConsoles();
      // Repaint the graph live — but from a *fresh* `dump`, throttled. The status
      // indicators come from `state`, the topology does not, so repainting on a
      // state tick alone would animate a stale graph: a node another client added
      // would stay invisible while the page visibly refreshed around it.
      if (view === "graph") refreshTopology();
      break;
    case "lock":
      renderConsoles();
      if (view === "graph") renderView();
      break;
    case "tap.data": onTapData(msg.params); break;
    case "tap.closed": onTapClosed(msg.params); break;
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
  if (storageError) {
    storageEl.textContent = "history: memory only (write failed)";
    storageEl.title = storageError;
    return;
  }
  if (!opfsOk) { storageEl.textContent = "history: memory only"; return; }
  storageEl.textContent = `history: OPFS (${persistStatus})`;
}

// A failed persist drops us to memory-only *visibly* — in the badge, in the terminal,
// and in the console log. Origin storage is evictable and quota-limited, so honesty
// about best-effort persistence beats pretending (§15.32).
function storageFailed(err) {
  opfsOk = false;
  storageError = String((err && err.message) || err);
  console.error("serial_nexus: console history persistence failed", err);
  renderStorageBadge();
  appendMarker("— history persistence failed; scrollback is memory-only from here —\n");
}

// The OPFS key isolates history per daemon and per boot: the web origin (a stable
// host:port, §15.32) stands in for the socket path, plus the endpoint and the daemon
// instance nonce so a restart never splices across reset offsets.
function keyFor(display) {
  return `${location.host}::${display}::${instanceNonce ?? "unknown"}`;
}

// Selecting a console spans three awaits, and clicks do not queue: a second click can
// land in any of the gaps. A generation counter captured at entry and re-checked after
// every await makes a superseded continuation abandon its work — otherwise the loser
// leaks its daemon-side tap (it never reaches the `tap.close`) and, worse, pairs its own
// bytes with the winner's storage key, where `save()` truncates and one console's
// scrollback overwrites another's. `historyKey` and `history` are therefore adopted in
// one synchronous step and never exist as a mismatched pair.
async function selectConsole(display) {
  const gen = ++selectGen;
  flushSave();                       // persist the outgoing console under its own key

  // Drop everything the previous console owned, synchronously. Clearing `currentTap`
  // before the await means an overlapping selection neither closes it twice nor mistakes
  // a stale tap's bytes for the new console's.
  const closing = currentTap;
  currentTap = null;
  history = null;
  historyKey = null;
  selected = display;
  decoder = new TextDecoder("utf-8", { fatal: false });
  termEl.textContent = "";
  renderConsoles();

  if (closing !== null) {
    await rpc("tap.close", { tap: closing });
    if (gen !== selectGen) return;
  }

  // Restore persisted scrollback (if any) before the ring replay, so the frontier trims
  // the ring's overlap and the terminal shows history-then-ring-then-live contiguously.
  const key = keyFor(display);
  let stored = null;
  if (opfsOk) { try { stored = await load(key); } catch { stored = null; } }
  if (gen !== selectGen) return;

  historyKey = key;
  history = stored ? fromStored(stored.bytes, stored.endOffset) : newHistory();
  if (stored) {
    appendMarker(`— stored history (${stored.bytes.length} bytes) —\n`);
    appendText(decoder.decode(stored.bytes, { stream: true }));
  }

  const res = await rpc("tap.open", { endpoint: display, replay: true });
  if (gen !== selectGen) {
    // A newer selection won while this open was in flight: close the tap we just
    // opened rather than leaking it daemon-side, and touch no shared state.
    if (res && res.tap !== undefined && res.tap !== null) rpc("tap.close", { tap: res.tap });
    return;
  }
  if (res) {
    currentTap = res.tap;
    historyEpoch = res.epoch ?? 0;
    // The tap's stream begins at res.from_offset; if we restored nothing, anchor the
    // history there so the first live chunk is not mistaken for offset 0.
    if (!stored) history.frontier = res.from_offset;
    // The endpoint's offset space restarts whenever the daemon rebuilds its hub —
    // `load --replace`, `remove-node`, `add-node` — while `instance` (per boot) does
    // not change, so stored scrollback would otherwise reject every new chunk as
    // already-seen and the console would freeze. The daemon now *says* which space it
    // is talking about (§15.38), so this is a comparison rather than the guess it used
    // to be: guessing from `from_offset` alone called every ordinary reload a restart
    // and duplicated the ring into stored scrollback each time.
    else if (offsetSpaceChanged(stored.epoch, historyEpoch)) {
      reanchor(history, res.from_offset);
      appendMarker("\n— the daemon's graph was reconfigured; offsets restarted —\n");
    }
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
  // The daemon lost bytes at its own producer→hub hop and says so (§5/§15.32). The
  // offset space stays contiguous by design, so a silent splice here would conceal a
  // real hole — show it instead.
  if (params.gap_before) appendMarker(`\n— ${params.gap_before} bytes lost (daemon feed) —\n`);
  const fresh = splice(history, params.offset ?? history.frontier ?? 0, bytes);
  if (fresh.length) {
    appendText(decoder.decode(fresh, { stream: true }));
    scheduleSave();
  }
}

// The daemon detached this tap because its endpoint left the graph — `load --replace`,
// `remove-node`, or `teardown` (§10). Without this the stream simply stops, which is
// indistinguishable from a quiet console: the operator watches a dead pane believing
// it is live. Say so, flush what we have, and drop the tap so the next selection
// re-opens cleanly rather than closing an id the daemon has already retired.
function onTapClosed(params) {
  if (!params || params.tap !== currentTap) return;
  flushSave();
  currentTap = null;
  appendMarker(`\n— console detached: ${params.reason || "endpoint gone"} —\n`);
}

function scheduleSave() {
  if (!opfsOk || !historyKey || saveTimer) return;
  // Debounce: snapshot the capped buffer at most every second, not per chunk.
  saveTimer = setTimeout(() => { saveTimer = null; flushSave(); }, 1000);
}

function flushSave() {
  if (saveTimer) { clearTimeout(saveTimer); saveTimer = null; }
  if (!opfsOk || !historyKey || !history || history.frontier === null) return;
  // Serialized per key by the saver: overlapping full-buffer rewrites of one console
  // would truncate each other (WEB-5). Errors arrive at `storageFailed`.
  saver.save(historyKey, bytesOf(history), history.frontier, historyEpoch);
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

const initialHash = location.hash.replace("#", "");
if (initialHash === "graph" || initialHash === "editor") view = initialHash;
renderView();
connect();
