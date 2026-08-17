// serial_nexus web console client (design §17). A pure browser client of the
// server's WebSocket, which is itself a filtering proxy of the daemon's JSON-RPC
// surface (§10). The per-session token rides the same-origin `HttpOnly` cookie the
// bootstrap URL set (§15.29), so this code never sees it — and since review WEBS-1
// that cookie is scoped `Path=/ws`, which means the browser attaches it to the
// upgrade below and to nothing else; the assets travel on a separate, capability-free
// cookie. Nothing here changes as a result, and nothing here may start carrying the
// token by hand: a credential this file can read is a credential a DOM-injection slip
// can exfiltrate. The layout is the contract; this rendering iterates freely (§15.16).
//
// Scrollback beyond the daemon's replay ring lives here, in the browser (§15.32): each
// console's hostward stream is folded by the monotonic byte offset every `tap.data`
// carries (§10) into a capped history persisted in the Origin Private File System, keyed
// by the web origin, the endpoint, and the daemon `instance` nonce — so a reload trims
// the ring overlap exactly and a daemon restart starts fresh, the previous boot's records
// being swept once per connection so "fresh" does not mean "forever" (§15.32's per-console
// cap is a cap on what is *retained*, not on what accumulates). The splice/retention math
// is history.mjs (unit tested); OPFS I/O is opfs.mjs; both degrade to memory-only where
// OPFS is absent.
//
// What reaches the *screen* is a bounded window onto that log, not the log: the rendered
// scrollback is capped (review HIST-2 — an unbounded `<pre>` does not merely grow, it
// makes the console fall further behind its device the longer it stays attached), and the
// bytes are interpreted through the minimal ANSI subset §17 sanctions (ansi.mjs, review
// UIR-1) instead of being poured into a Text node as literal escape litter. Neither
// touches the byte log: `export` and the OPFS record stay the raw device stream.
// (ES modules are always strict, so no "use strict" pragma is needed.)

import {
  newHistory,
  fromStored,
  splice,
  bytesOf,
  noteGap,
  offsetSpaceChanged,
  reanchor,
} from "/history.mjs";
import { makeSaver } from "/saver.mjs";
import {
  opfsAvailable,
  requestPersistence,
  load,
  save,
  clear,
  pruneForeign,
} from "/opfs.mjs";
import { model as graphModel, render as renderGraph } from "/graph.mjs";
import { render as renderEditor } from "/editor.mjs";
import { newAnsiState, parseAnsi } from "/ansi.mjs";

const consolesEl = document.getElementById("consoles");
const connEl = document.getElementById("conn");
const provenanceEl = document.getElementById("provenance");
const termEl = document.getElementById("term");
const titleEl = document.getElementById("pane-title");
const lockEl = document.getElementById("pane-lock");
const dropsEl = document.getElementById("pane-drops");
const unknownEl = document.getElementById("pane-unknown");
const storageEl = document.getElementById("pane-storage");
const exportBtn = document.getElementById("exportbtn");
const clearBtn = document.getElementById("clearbtn");
const sendForm = document.getElementById("sendform");
const sendLine = document.getElementById("sendline");
const sendBtn = document.getElementById("sendbtn");
/// The standing steal preference (§15.66), replacing the per-line `confirm()`. Its
/// default lives in `index.html` as the `checked` attribute rather than being assigned
/// here, so it holds from the first paint instead of from whenever this module finishes
/// loading — and so a reader of the markup can see what the console does by default.
const stealBox = document.getElementById("stealbox");
const graphViewEl = document.getElementById("graphview");
const editorViewEl = document.getElementById("editorview");
const viewBtns = Array.from(document.querySelectorAll(".viewbtn"));

let ws = null;
let nextId = 1;
const pending = new Map();          // id -> resolve
const pendingFull = new Set();      // ids whose caller wants the whole envelope
let selected = null;                // selected endpoint display
let currentTap = null;              // active tap id
/// The endpoint's producer→hub loss at the moment `currentTap` was opened — adopted with
/// it, and meaningless without it (§10, TAP-1b). See `updateHead`.
let feedBaseline = 0;
let lastState = { nodes: [], taps: [] };
let decoder = new TextDecoder("utf-8", { fatal: false });

let instanceNonce = null;           // daemon per-boot nonce (§10 `info`); history reset key
let opfsOk = opfsAvailable();       // false → memory-only fallback
let persistStatus = "unavailable";  // persisted | best-effort | unavailable
let storageError = null;            // last persistence failure, shown in the badge
let history = null;                 // current console's ConsoleHistory (history.mjs)
let historyKey = null;              // OPFS key for the current console — set with `history`
let saveTimer = null;               // debounced persist handle
let selectGen = 0;                  // selectConsole re-entrancy generation (see below)
let view = "console";               // console | graph | editor (§17, §15.35)
let lastDump = { node: [], edge: [] };
let lastPorts = [];
/// The terminal's render queue and its one-shot flush flag (§15.67 — the reasoning lives
/// with `scheduleRender`, far below). Declared here, with the rest of the module state,
/// because `resetTerminal` discards the queue and is itself declared above that code: a
/// `let` in its own section would put both inside a temporal dead zone that only a
/// future caller would discover.
let renderQueue = [];
let renderScheduled = false;

/// `AppError::Locked`'s code, from `serial-nexus-rpc`'s single error registry (§16.8): the one
/// `send` refusal that means contention, and therefore the only one that may offer a
/// steal. Everything else is a different refusal wearing a different code, and saying
/// "locked" over it is how the console told an operator a false story (WEBUI-1).
const APP_LOCKED = -32003;

/// The node-status glyphs, matching the graph page's `.gstatus` vocabulary (graph.mjs)
/// so the rail and the graph read alike (DEVR-2). The CSS classes are shared, in
/// app.css; only the three glyphs are restated here, because graph.mjs's copy is private
/// to its own renderer.
const STATUS_GLYPH = { active: "●", waiting: "◌", faulted: "✕" };

/// §17: "Taps open lazily per viewed console and close on blur after a grace interval."
/// The grace is deliberately long. Holding a tap open costs a bounded queue and some
/// base64; dropping one costs a re-open that may have to announce a hole, since
/// §15.38's `epoch`/`from_offset` splice is lossless only as far as the endpoint's ring
/// reaches. A minute covers an operator reading a datasheet in another tab, and an
/// operator who left for the day stops costing the daemon anything (DEVR-5).
const TAP_GRACE_MS = 60_000;

/// The rendered scrollback cap, in characters. Deliberately smaller than the *retained*
/// buffer (`history.cap`, 16 MiB by default — §15.32): a byte in a `Uint8Array` costs
/// nothing to keep, while a character in a laid-out `<pre>` costs throughput on every
/// chunk that follows it, forever. 256 KiB is the size at which the HIST-2 measurement
/// held flat (121–144 KiB/s, against 34.9 → 14.4 KiB/s decaying with an unbounded pane).
/// `export` still hands back the whole retained buffer; the screen is a window onto it.
const TERM_CHAR_CAP = 256 * 1024;

/// The largest node one flush may commit before it starts a new one (§15.67).
///
/// `commitNode` enforces `TERM_CHAR_CAP` by dropping whole committed nodes, and it never
/// drops the last one — a cap smaller than one node has to shrink the screen, not blank
/// it. Before batching, a node was one notification's worth of output (single-digit KB),
/// so that exception was invisible. Batching makes a node one *flush*'s worth, which under
/// a backlogged main thread is however much arrived while the previous flush ran — so
/// without this the rendered scrollback's bound would have quietly become "the cap, or one
/// flush, whichever is larger". Splitting at 32 KiB keeps the trim granular for a handful
/// of extra nodes, and restores the bound to `TERM_CHAR_CAP + TERM_BLOCK_CAP`.
const TERM_BLOCK_CAP = 32 * 1024;

/// How far back a carriage return can reach. Only the incomplete last line is editable,
/// and a line this long is committed as it stands so an unterminated multi-megabyte
/// "line" cannot make every chunk rewrite a megabyte of DOM. Stated rather than hidden:
/// a `\r` cannot rewind past a segment this froze — which no progress bar comes near.
const TERM_LINE_CAP = 4096;

// At most one OPFS write in flight per key, newest snapshot wins: `createWritable()`
// truncates, so two overlapping full-buffer rewrites of one console's scrollback let the
// second truncate into the middle of the first. Failures are surfaced, not swallowed.
// The delete rides the same queue, so a "clear" cannot be undone by a snapshot that was
// already inside `createWritable()` when the operator asked for it (HISTC-2).
const saver = makeSaver(save, (err) => storageFailed(err), clear);

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

// What the connection badge says when the socket closes, from the CloseEvent the server
// took the trouble to send (§17, review WEB-4: "a dead pane must never look live", and it
// must say *why*). The bridge codes the two endings it causes — 1001 with a reason when
// the daemon went away, 1009 when its WebSocket size caps refused what this page sent —
// and until now both arrived here and were discarded, so a refused frame, a dead daemon
// and a pulled network cable produced the same eight words.
//
// Anything else, including a bare 1005 ("no status received", which is what an abortive
// close leaves), falls through to the generic line: an ending we cannot explain must not
// be narrated as one we can.
function disconnectMessage(ev) {
  const tail = "reload to reconnect";
  if (ev && ev.code === 1009) {
    return `disconnected — the server refused an oversized message; ${tail}`;
  }
  if (ev && ev.code === 1001 && ev.reason) {
    return `disconnected — ${ev.reason}; ${tail}`;
  }
  return `disconnected — ${tail}`;
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
    // Provenance (plan §17.3): name this client and the daemon behind it. `info` is
    // the daemon's own answer, so an operator looking at a console can tell which
    // build is serving it — and, after §15.40, that the two halves are the same
    // generation. A daemon that did not answer leaves the client name standing alone
    // rather than claiming a version it does not have.
    if (provenanceEl) {
      provenanceEl.textContent = info?.daemon_version
        ? `serial-nexus-web → serial-nexus-daemon ${info.daemon_version}`
        : "serial-nexus-web";
    }
    if (opfsOk) persistStatus = await requestPersistence();
    // Reclaim the records of previous daemon boots. `keyFor` folds in the per-boot
    // `instance` nonce, so without this every restart orphans one capped record per
    // console — forever, since `clear` only ever touched the selected console — until
    // the origin's quota is reached and persistence dies for good (HIST-3). Only with a
    // nonce in hand: with `keyFor` falling back to "unknown" the sweep would delete the
    // record it is about to write. Best-effort by construction, like the storage it
    // manages.
    if (opfsOk && instanceNonce !== null && instanceNonce !== undefined) {
      pruneForeign(`${location.host}::`, instanceNonce).catch(() => {});
    }
    renderStorageBadge();
    refreshState();
  };
  ws.onclose = (ev) => {
    connEl.textContent = disconnectMessage(ev);
    connEl.className = "disconnected";
    sendLine.disabled = sendBtn.disabled = true;
    // The daemon dropped every tap with the connection; a grace timer still counting
    // down would `tap.close` an id that no longer exists on a socket that is gone.
    if (tapGraceTimer !== null) {
      clearTimeout(tapGraceTimer);
      tapGraceTimer = null;
    }
    currentTap = null;
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
  // Leaving the console pane leaves no console being viewed, which §17's lazy-tap clause
  // covers as squarely as backgrounding the tab does (DEVR-5); returning to it re-opens.
  noteWatchChange();
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

// ---- the left rail, reconciled rather than rebuilt (§15.65) -------------------------
//
// This function used to open with `consolesEl.innerHTML = ""` and rebuild every row,
// re-attaching a fresh `li.onclick` to each. It is driven by `state`, and the daemon
// publishes a state snapshot **every 200 ms, unconditionally, whether or not anything
// changed** (§10) — so on a completely idle graph the rail was destroyed and rebuilt five
// times a second, forever.
//
// The cost was not the DOM work. It was that a click whose `mousedown` landed on a row
// that was destroyed before `mouseup` produces **no `click` event on that row at all**:
// the browser dispatches to the nearest common ancestor of the two targets, and with the
// pressed element detached there is no handler to reach. The operator pressed the console
// name, nothing whatsoever happened, and they pressed again — which is what "it takes a
// few seconds to show a console after clicking it" actually was. Measured against this
// page with a synthetic press of a human's duration: 0/20 lost at 0 ms of dwell, **9/20
// at 60 ms, 10/20 at 100 ms, 16/20 at 150 ms**. The median latency of a click that *did*
// land was 9–18 ms, so nothing here was ever slow; roughly half of the operator's clicks
// simply never arrived.
//
// **Nothing in the tree could see it**, and that is structural rather than unlucky:
// `ui-tests/tests/fixture.mjs` selects with Playwright's `.click()`, which presses and
// releases in the same tick (0 ms dwell — measured lossless) *and* auto-retries when an
// element goes detached. The gate was forgiving in exactly the two ways a mouse is not.
//
// So rows are now **keyed by display address and patched in place**: an `<li>` for a
// console that still exists is the same `<li>` it was a tick ago, and only the fields
// that actually changed are written. A row is created when a console appears and removed
// when it leaves; nothing else is touched. `endpointsFromState` still runs per tick — it
// walks a handful of nodes and allocates a small array, which is not what this was about.
const railRows = new Map(); // display -> { li, dot, name, lock, waiters }

// One delegated listener, registered once, instead of a handler per row.
//
// Belt and braces rather than the fix: with rows reconciled the `<li>` survives, so a
// per-row handler would work too. But a listener on the container cannot be detached by
// any future rebuild of its children, which is precisely the failure mode above — and it
// is one listener instead of one per console. `closest` walks up from whatever the click
// landed on (the glyph, the name, a badge).
consolesEl.addEventListener("click", (ev) => {
  const li = ev.target instanceof Element ? ev.target.closest("li[data-display]") : null;
  if (li && consolesEl.contains(li)) selectConsole(li.dataset.display);
});

/// Set `prop` on `node` only when it differs. Assigning `textContent` replaces the child
/// text node whether or not the string changed, and assigning `className` invalidates
/// style whether or not the class list changed; on a surface repainted five times a
/// second that is the difference between a stable DOM and a churning one.
function setIfChanged(node, prop, value) {
  if (node[prop] !== value) node[prop] = value;
}

/// Build the fixed skeleton of one rail row. The badges are created lazily by
/// [`patchRow`], because most consoles have neither.
function newRow(display) {
  const li = document.createElement("li");
  li.dataset.display = display;
  const dot = document.createElement("span");
  li.appendChild(dot);
  const name = document.createElement("span");
  name.className = "cname";
  name.textContent = display;
  li.appendChild(name);
  return { li, dot, name, lock: null, waiters: null };
}

/// Update one row to match `ep`, touching only what moved.
function patchRow(row, ep, isSelected) {
  setIfChanged(row.li, "className", isSelected ? "console selected" : "console");
  // §17's rail is "display address, node status, lock holder and waiter count", and
  // the status was the one it never rendered (DEVR-2) — so on a lost device every
  // field the rail showed was byte-identical to before and its only live signal was
  // nothing at all. Same glyphs and classes as the graph page, so the two views read
  // alike; `reason` rides along as the tooltip, which is where "device … lost" becomes
  // visible without leaving the console.
  const status = (ep.node && ep.node.status) || "unknown";
  setIfChanged(row.dot, "className", `gstatus ${status}`);
  setIfChanged(row.dot, "textContent", STATUS_GLYPH[status] || "?");
  setIfChanged(row.dot, "title", (ep.node && ep.node.reason) || status);

  const holder = ep.lock && ep.lock.holder ? `🔒 ${ep.lock.holder}` : null;
  if (holder === null) {
    if (row.lock) {
      row.lock.remove();
      row.lock = null;
    }
  } else {
    if (!row.lock) {
      row.lock = document.createElement("span");
      row.lock.className = "lockbadge";
      // Before the waiter count, not merely at the end. A row that gained waiters while
      // the lock was free and *then* gained a holder would otherwise carry its badges in
      // the opposite order from a row built holder-first — an ordering the rebuild-per-
      // tick version could not produce, and which is exactly the class of drift keying
      // rows introduces. `insertBefore(x, null)` appends, so the no-waiters case needs no
      // branch of its own.
      row.li.insertBefore(row.lock, row.waiters);
    }
    setIfChanged(row.lock, "textContent", holder);
  }

  const waiters = ep.lock && ep.lock.waiters ? ep.lock.waiters.length : 0;
  if (waiters === 0) {
    if (row.waiters) {
      row.waiters.remove();
      row.waiters = null;
    }
  } else {
    if (!row.waiters) {
      row.waiters = document.createElement("span");
      row.waiters.className = "waiters";
      row.li.appendChild(row.waiters);
    }
    setIfChanged(row.waiters, "textContent", `+${waiters}`);
  }
}

function renderConsoles() {
  const eps = endpointsFromState(lastState);
  // The selected endpoint can leave the graph under the operator's feet — `remove-node`,
  // a cascade, `load --replace`. Nothing used to notice: `selected` stayed set and the
  // send box stayed enabled, so the next `send` came back `-32602` and the console
  // reported it as a lock conflict with a steal offer for a lock that did not exist
  // (WEBUI-1). Drop the selection; `tap.closed` has already said why in the pane.
  if (selected !== null && !eps.some((e) => e.display === selected)) selected = null;

  // Walk the desired list against the live children, moving a row only when it is out of
  // order. A rail whose contents did not change performs zero DOM mutations.
  let cursor = consolesEl.firstChild;
  const live = new Set();
  for (const ep of eps) {
    live.add(ep.display);
    let row = railRows.get(ep.display);
    if (!row) {
      row = newRow(ep.display);
      railRows.set(ep.display, row);
    }
    patchRow(row, ep, ep.display === selected);
    if (row.li === cursor) {
      cursor = cursor.nextSibling;
    } else {
      consolesEl.insertBefore(row.li, cursor);
    }
  }
  // Whatever is left past the cursor is a console that has gone.
  while (cursor) {
    const next = cursor.nextSibling;
    cursor.remove();
    cursor = next;
  }
  for (const display of railRows.keys()) {
    if (!live.has(display)) railRows.delete(display);
  }
  updateHead();
}

function updateHead() {
  titleEl.textContent = selected || "select a console";
  const ep = endpointsFromState(lastState).find((e) => e.display === selected);
  lockEl.textContent = ep && ep.lock && ep.lock.holder ? `locked by ${ep.lock.holder}` : "";
  const tap = (lastState.taps || []).find((t) => t.tap === currentTap);
  // §17: "the tab's own tap drop counter is shown when nonzero". `state` reports the
  // endpoint hub's *lifetime* producer→hub loss, so adding it raw charged every later tab
  // for everything the endpoint had ever lost — including bytes that went missing before
  // the tab existed (37-WEBC-4). `tap.open` answers with that figure precisely so a client
  // can separate the two, and the difference is what this tap was actually told about
  // through `gap_before`. Clamped, because a hub rebuilt beneath us restarts its counter.
  const feedSinceOpen = tap ? Math.max(0, (tap.feed_dropped || 0) - feedBaseline) : 0;
  const dropped = tap ? (tap.dropped || 0) + feedSinceOpen : 0;
  dropsEl.textContent = dropped > 0 ? `⚠ ${dropped} tap bytes dropped` : "";
  renderUnknownBadge();
  // The send box follows the *tap*, not merely the selection. An endpoint whose tap the
  // daemon retired (`tap.closed`), or that the grace timer released while nobody was
  // watching, is not somewhere a line can be typed with any confidence (WEBUI-1,
  // DEVR-5) — and a box that stays live over a gone endpoint is what produced the false
  // steal offer in the first place.
  sendLine.disabled = sendBtn.disabled = !(selected && currentTap !== null);
  // Export and clear act on the *stored* scrollback, which outlives the tap.
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

// ---- the in-flight selection, named rather than inferred (review 37-WEBC-1/2/6) -----
//
// A selection tears the previous console's state down synchronously and adopts the new
// console's over three awaits. Everything that could run in those gaps used to read the
// torn-down state as "nothing is happening on this pane": the grace timer's resume saw
// `currentTap === null` and opened a second tap in the same generation, leaking the first
// daemon-side for the page's life (37-WEBC-2); the clear button saw `historyKey === null`
// and deleted nothing, while the resuming selection re-rendered — and, at its next flush,
// re-wrote — the record the operator had just confirmed destroying (37-WEBC-1). Those
// nulls are legitimate *inside* this window and only inside it, so the window gets a name
// instead of being inferred from them.
//
// A depth rather than a flag: two rapid clicks put two continuations in flight, and a
// boolean the first one cleared would open the window under the second.
let selectionDepth = 0;

function selectionInFlight() {
  return selectionDepth > 0;
}

/// Bumped by every confirmed clear, with the key it destroyed. A selection parked on an
/// await compares the counter it captured on entry, which is how it learns that the
/// record it is holding is one the operator has already thrown away (37-WEBC-1).
let clearSeq = 0;
let clearedKey = null;

// Selecting a console spans three awaits, and clicks do not queue: a second click can
// land in any of the gaps. A generation counter captured at entry and re-checked after
// every await makes a superseded continuation abandon its work — otherwise the loser
// leaks its daemon-side tap (it never reaches the `tap.close`) and, worse, pairs its own
// bytes with the winner's storage key, where `save()` truncates and one console's
// scrollback overwrites another's. `historyKey` and `history` are therefore adopted in
// one synchronous step and never exist as a mismatched pair — and the offset-space
// `epoch` lives *inside* the history object for the same reason: as a third module
// global it lagged the other two by an await (and stayed stale for good if `tap.open`
// failed), so a flush in that window stamped the previous console's epoch onto this
// console's record and the next selection declared a false restart (HIST-4).
async function selectConsole(display) {
  const gen = ++selectGen;
  // Captured before the first await, so a clear confirmed anywhere in this continuation —
  // including while the dialog held the renderer — is visible to the restore below.
  const clearAt = clearSeq;
  selectionDepth += 1;
  try {
    await adoptConsole(display, gen, clearAt);
  } finally {
    // In `finally`, because every early return here is a *superseded* selection: leaving
    // the window open would freeze the pane's resume path for the rest of the session.
    selectionDepth -= 1;
  }
}

async function adoptConsole(display, gen, clearAt) {
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
  resetTerminal();
  renderConsoles();

  if (closing !== null) {
    await rpc("tap.close", { tap: closing });
    if (gen !== selectGen) return;
  }

  // Restore persisted scrollback (if any) before the ring replay, so the frontier trims
  // the ring's overlap and the terminal shows history-then-ring-then-live contiguously.
  const key = keyFor(display);
  let stored = null;
  if (opfsOk) {
    // Read *through* the saver's queue rather than past it. `flushSave` above may have
    // just enqueued a snapshot for this very key — re-selecting the console that is
    // already selected is an ordinary click — and `load` goes straight to storage, so the
    // restore could return the record that snapshot is about to replace and come back
    // with a frontier lagging the stream by a debounce window, which a talkative console
    // then reads as a ring gap that never happened (37-WEBC-6).
    await saver.settled(key);
    if (gen !== selectGen) return;
    try { stored = await load(key); } catch { stored = null; }
  }
  if (gen !== selectGen) return;
  // The operator confirmed a clear for this console while this continuation was parked
  // (`confirm` blocks the renderer, so the window cannot close itself): the record is
  // deleted, and restoring the copy in hand would put it back on screen and, at the next
  // flush, back in storage (37-WEBC-1).
  if (clearSeq !== clearAt && clearedKey === key) stored = null;

  historyKey = key;
  history = stored
    ? fromStored(stored.bytes, stored.endOffset, stored.epoch)
    : newHistory();
  if (stored) {
    appendMarker(`— stored history (${stored.bytes.length} bytes) —\n`);
    // Only the tail can survive the rendered-scrollback cap, so only the tail is parsed:
    // interpreting 16 MiB of restored history to throw all but the last 256 KiB of it
    // away is work with no output (HIST-2). The decoder is fresh here, so a cut through
    // a multi-byte character costs one replacement character at the seam and nothing
    // downstream — and `history` still holds every stored byte, which is what `export`
    // hands back.
    const shown =
      stored.bytes.length > TERM_CHAR_CAP
        ? stored.bytes.subarray(stored.bytes.length - TERM_CHAR_CAP)
        : stored.bytes;
    writeTerminal(decoder.decode(shown, { stream: true }));
  }

  await openTapAndAnchor(display, gen, !!stored);
  if (gen === selectGen) renderConsoles();
}

// Open the tap for `display` and anchor the history against the offset space the daemon
// answers with. Shared by a fresh selection and by the grace timer's resume (DEVR-5), so
// the epoch comparison, the ring-gap marker and the replay notice live on exactly one
// path — a second copy is how the two would come to disagree about what a reconnect
// means. `hadBytes` says whether `history` already holds an anchored log: a fresh
// selection passes whether storage supplied one, a resume passes whether the console has
// been streaming.
async function openTapAndAnchor(display, gen, hadBytes) {
  // The open is part of the in-flight-selection window: until the reply lands
  // `currentTap` is null, and anything that reads that as "this console has no tap" —
  // the grace timer's resume, above all — would start a second open in the same
  // generation, whose loser then streams to nobody for the page's life (37-WEBC-2).
  selectionDepth += 1;
  try {
    await openTap(display, gen, hadBytes);
  } finally {
    selectionDepth -= 1;
  }
}

async function openTap(display, gen, hadBytes) {
  const res = await rpc("tap.open", { endpoint: display, replay: true });
  if (gen !== selectGen) {
    // A newer selection won while this open was in flight: close the tap we just
    // opened rather than leaking it daemon-side, and touch no shared state.
    if (res && res.tap !== undefined && res.tap !== null) rpc("tap.close", { tap: res.tap });
    return;
  }
  if (!res) return;
  currentTap = res.tap;
  // Adopted with the tap, in the same synchronous step: the endpoint's producer→hub loss
  // as of this open is the baseline the badge measures this tab's own drops against
  // (37-WEBC-4, `updateHead`). A hub that never lost anything reports zero, which is the
  // ordinary case and costs the subtraction nothing.
  feedBaseline = res.feed_dropped ?? 0;
  const epoch = res.epoch ?? 0;
  // `history.epoch` still holds what storage said — it was adopted with the key and
  // the buffer — so this comparison has to happen before the live epoch is written in.
  const restarted = hadBytes ? offsetSpaceChanged(history.epoch, epoch) : false;
  history.epoch = epoch;
  // The tap's stream begins at res.from_offset; if we restored nothing, anchor the
  // history there so the first live chunk is not mistaken for offset 0.
  if (!hadBytes) {
    history.frontier = res.from_offset;
  } else if (restarted) {
    // The endpoint's offset space restarts whenever the daemon rebuilds its hub —
    // `load --replace`, `remove-node`, `add-node` — while `instance` (per boot) does
    // not change, so stored scrollback would otherwise reject every new chunk as
    // already-seen and the console would freeze. The daemon now *says* which space it
    // is talking about (§15.38), so this is a comparison rather than the guess it used
    // to be: guessing from `from_offset` alone called every ordinary reload a restart
    // and duplicated the ring into stored scrollback each time.
    reanchor(history, res.from_offset);
    appendMarker("\n— the daemon's graph was reconfigured; offsets restarted —\n");
  } else {
    // Same offset space, but the stream starts past where we left off: the daemon's
    // replay ring rotated over the bytes between, while this tab was closed, looking at
    // another console, or released by the grace timer. With the default 64 KiB ring that
    // is the ordinary outcome for a talkative console, and the `— replay (N bytes) —`
    // marker below positively suggests continuity — so say what is missing first
    // (HIST-1).
    markOffsetGap(noteGap(history, res.from_offset));
  }
  if (res.replay_bytes > 0) appendMarker(`— replay (${res.replay_bytes} bytes) —\n`);
  else if (!hadBytes) appendMarker("— no history (set replay_ring to keep scrollback) —\n");
  // The console may have stopped being watched while this open was in flight — or may
  // never have been watched, if the operator picked a console from the rail while the
  // graph page was showing. Either way the tap gets its grace interval rather than being
  // opened and shut in the same breath.
  noteWatchChange();
}

// ---- §17's lazy taps: "close on blur after a grace interval" (DEVR-5) ---------------
//
// The client opened lazily from the first version and never released a tap for any
// reason short of selecting another console, a daemon-side `tap.closed`, or the link
// dropping. Two reachable cases went uncovered by that: a backgrounded tab, and — the
// one that fails even a charitable reading of "on blur" — switching to the graph or
// editor page, which hides the console pane as a unit while its tap keeps streaming with
// no console on screen at all.
let tapGraceTimer = null;

function consoleIsWatched() {
  return view === "console" && !document.hidden;
}

function noteWatchChange() {
  if (consoleIsWatched()) {
    if (tapGraceTimer !== null) {
      clearTimeout(tapGraceTimer);
      tapGraceTimer = null;
    }
    resumeTap();
  } else if (tapGraceTimer === null && currentTap !== null) {
    tapGraceTimer = setTimeout(() => {
      tapGraceTimer = null;
      suspendTap();
    }, TAP_GRACE_MS);
  }
}

async function suspendTap() {
  if (currentTap === null || consoleIsWatched()) return;
  const closing = currentTap;
  currentTap = null;
  flushSave();   // the scrollback is the viewer's, and the viewer keeps it (§15.28)
  // Said out loud, because the gap marker that may follow the re-open is otherwise
  // unattributable: §5's honesty covers the loss this client caused itself.
  appendMarker("\n— tap released; this console was not being watched —\n");
  renderConsoles();
  await rpc("tap.close", { tap: closing });
}

// Re-attach through the same splice path a selection uses, so §15.38's epoch re-anchor
// and HIST-1's loss marker both still apply to what the grace interval cost.
async function resumeTap() {
  // `currentTap === null` plus a non-null `history` was the whole guard, and both hold
  // *inside* a selection — which is how a view switch or a tab unhide landed a second
  // `tap.open` in the same generation (37-WEBC-2). The in-flight window also covers this
  // function's own re-entry, since it is the only thing that starts an open outside a
  // selection. A flip ignored here is deferred rather than lost: `openTapAndAnchor` ends
  // by re-reading the watch state.
  if (selectionInFlight() || !selected || currentTap !== null || !history) return;
  const gen = selectGen;   // not a new selection: a real one supersedes this resume
  await openTapAndAnchor(selected, gen, history.frontier !== null);
  if (gen === selectGen) renderConsoles();
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
  // The other hole: a chunk that starts past the frontier. `splice` counts it — and
  // until HIST-1 nothing ever read that counter, so the only hole the console did not
  // announce was the one it had measured itself. The live case is not reachable through
  // the daemon's own paths today (§10 keeps the delivered-bytes space contiguous and
  // reports feed loss as `gap_before` instead), which is precisely why it needs saying
  // out loud if it ever happens rather than being spliced over.
  const droppedBefore = history.dropped;
  const fresh = splice(history, params.offset ?? history.frontier ?? 0, bytes);
  markOffsetGap(history.dropped - droppedBefore);
  if (fresh.length) {
    writeTerminal(decoder.decode(fresh, { stream: true }));
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
  // The send box follows the tap: typing into a detached console produced a refusal the
  // client then mis-reported as a lock conflict (WEBUI-1).
  renderConsoles();
}

function scheduleSave() {
  if (!opfsOk || !historyKey || saveTimer) return;
  // Debounce: snapshot the capped buffer at most every second, not per chunk.
  saveTimer = setTimeout(() => { saveTimer = null; flushSave(); }, 1000);
}

function flushSave() {
  if (saveTimer) { clearTimeout(saveTimer); saveTimer = null; }
  if (!opfsOk || !historyKey || !history || history.frontier === null) return;
  // No epoch, no record. A snapshot is only meaningful in the offset space its offsets
  // count in, and until `tap.open` has answered (or a restored record has supplied its
  // own) there is nothing honest to stamp — writing a guess is what made the next
  // selection declare a false restart and re-render the ring (HIST-4).
  if (history.epoch === null) return;
  // Serialized per key by the saver: overlapping full-buffer rewrites of one console
  // would truncate each other (WEB-5). Errors arrive at `storageFailed`.
  saver.save(historyKey, bytesOf(history), history.frontier, history.epoch);
}

// ---- the terminal surface ---------------------------------------------------------
//
// Two review findings share this code, and they share it because they are one surface.
//
// HIST-2: nothing ever removed a node from `#term`, so the rendered scrollback grew with
// everything the console had ever emitted. The harm was not memory. The writer reads
// `scrollHeight` and writes `scrollTop` on every chunk, forcing a synchronous layout of a
// `<pre>` that only grows, so *render throughput decayed with the size of the pane* —
// measured 34.9 → 14.4 KiB/s over the first 2.9 MB, against a flat 121–144 KiB/s with the
// DOM bounded (10.3 MB in 75 s versus 3.1 MB in 145 s). A console that falls further
// behind its device the longer it stays attached is worse than one that shows less. So
// the rendered scrollback is capped, with the running character count kept in a
// *variable*: measuring it with `textContent.length` is O(n) and would reintroduce
// exactly the cost being removed.
//
// UIR-1: decoded bytes went into the DOM as a bare Text node, so every CSI sequence an
// ordinary device console emits rendered as literal litter. `ansi.mjs` interprets the
// minimal subset §17 sanctions; this is the writer that turns its operations into DOM.
//
// The model is a scrollback log, not a grid. Committed lines are immutable nodes; only
// the *last, incomplete* line is editable, which is exactly the reach `\r`, `\b` and
// `CSI K` need and no more. Per chunk that costs one inserted node (this chunk's whole
// committed output, batched — a node per line would be hundreds of insertions) plus one
// rewritten pending line. Every insertion goes through `textContent`/`createTextNode`;
// nothing here builds markup from device bytes.
let ansi = newAnsiState();
let committed = [];        // [{node, chars}] — the immutable lines, in DOM order
let committedChars = 0;    // their total, tracked rather than measured (HIST-2)
let pendingEl = null;      // the incomplete last line; always `#term`'s last child
let pendingRuns = [];      // [{text, cls}] — its content
let pendingLen = 0;        // characters in `pendingRuns`
let pendingCol = 0;        // the cursor's column within them
let block = null;          // this chunk's committed output, inserted once
let blockChars = 0;

function termCharCap() {
  return Math.min(history ? history.cap : TERM_CHAR_CAP, TERM_CHAR_CAP);
}

function resetTerminal() {
  // Before anything else: text queued for the console being torn down must not be
  // painted into the one replacing it (§15.67). The bytes are not lost — `history` and
  // the OPFS record hold them, and this is the *screen*, which the caller is clearing on
  // purpose.
  discardQueuedRender();
  termEl.replaceChildren();
  ansi = newAnsiState();
  committed = [];
  committedChars = 0;
  pendingEl = null;
  pendingRuns = [];
  pendingLen = 0;
  pendingCol = 0;
  renderUnknownBadge();   // the tally belongs to the parser, and that is a fresh one
}

// §17 sanctions consuming every CSI, OSC, DCS and ESC sequence outside the subset, "the
// counter being §5's honesty applied to the one surface where the operator cannot
// otherwise see what was thrown away" — which it is only if the counter is *shown*. It
// was computed at eighteen sites in ansi.mjs and read by nothing (37-WEBC-5), the same
// shape as HIST-1's measured-but-unread hole. It sits beside the drop badge because the
// two answer one question about different losses: bytes the daemon could not deliver, and
// sequences this renderer does not speak.
//
// The last rendered value is kept so an ordinary chunk costs a comparison rather than a
// DOM write: the writer already forces one synchronous layout per chunk (HIST-2), and a
// second one for a counter that did not move is exactly the cost that measurement was
// about.
let unknownShown = -1;
function renderUnknownBadge() {
  const n = ansi ? ansi.unknown : 0;
  if (n === unknownShown) return;
  unknownShown = n;
  unknownEl.textContent = n > 0 ? `⚑ ${n} escape sequences not rendered` : "";
}

// Insert one immutable node ahead of the pending line, then drop the oldest until the
// rendered scrollback is back under the cap (HIST-2). The last committed node is never
// dropped: a cap smaller than one chunk must shrink the screen, not blank it.
function commitNode(node, chars) {
  termEl.insertBefore(node, pendingEl);
  committed.push({ node, chars });
  committedChars += chars;
  const cap = termCharCap();
  while (committedChars > cap && committed.length > 1) {
    const oldest = committed.shift();
    committedChars -= oldest.chars;
    oldest.node.remove();
  }
}

function runsInto(parent, runs) {
  for (const r of runs) {
    if (!r.text) continue;
    if (r.cls) {
      const s = document.createElement("span");
      s.className = r.cls;
      s.textContent = r.text;
      parent.appendChild(s);
    } else {
      parent.appendChild(document.createTextNode(r.text));
    }
  }
}

// Freeze the incomplete line and start a new one. Inside a chunk the frozen runs join
// that chunk's batch; outside one (a marker) they become a node of their own.
function commitPending(withNewline) {
  const runs = pendingRuns;
  let chars = pendingLen;
  if (withNewline) {
    runs.push({ text: "\n", cls: "" });
    chars += 1;
  }
  if (chars > 0) {
    if (block) {
      runsInto(block, runs);
      blockChars += chars;
      // Keep this flush's committed output in nodes the trim can actually drop
      // (`TERM_BLOCK_CAP`). Committing here rather than only at the end of the chunk is
      // what keeps `#term` bounded when a flush carries a whole backlog.
      if (blockChars >= TERM_BLOCK_CAP) {
        commitNode(block, blockChars);
        block = document.createElement("span");
        blockChars = 0;
      }
    } else {
      const node = document.createElement("span");
      runsInto(node, runs);
      commitNode(node, chars);
    }
  }
  pendingRuns = [];
  pendingLen = 0;
  pendingCol = 0;
}

function renderPending() {
  if (!pendingEl) {
    pendingEl = document.createElement("span");
    termEl.appendChild(pendingEl);
  }
  pendingEl.replaceChildren();
  runsInto(pendingEl, pendingRuns);
}

// `CSI 2J` / `CSI H`. The SGR attributes survive — a real terminal keeps its colours
// across an erase — but nothing on screen does, including this chunk's own output.
function clearScreen() {
  for (const c of committed) c.node.remove();
  committed = [];
  committedChars = 0;
  if (block) {
    block.replaceChildren();
    blockChars = 0;
  }
  pendingRuns = [];
  pendingLen = 0;
  pendingCol = 0;
}

function writeRun(text, cls) {
  if (pendingCol === pendingLen) {
    const last = pendingRuns[pendingRuns.length - 1];
    if (last && last.cls === cls) last.text += text;
    else pendingRuns.push({ text, cls });
    pendingLen += text.length;
    pendingCol = pendingLen;
  } else {
    overwriteRun(text, cls);
  }
  if (pendingLen > TERM_LINE_CAP) commitPending(false);
}

// `\r` or `\b` moved the cursor back into the line: write over what is there rather than
// appending, which is the whole point of interpreting them at all (UIR-1).
function overwriteRun(text, cls) {
  const start = pendingCol;
  const end = start + text.length;
  const head = [];
  const tail = [];
  let at = 0;
  for (const r of pendingRuns) {
    const rEnd = at + r.text.length;
    if (at < start) {
      head.push({ text: r.text.slice(0, Math.min(r.text.length, start - at)), cls: r.cls });
    }
    if (rEnd > end) tail.push({ text: r.text.slice(Math.max(end - at, 0)), cls: r.cls });
    at = rEnd;
  }
  pendingRuns = head
    .filter((r) => r.text)
    .concat([{ text, cls }], tail.filter((r) => r.text));
  pendingLen = Math.max(pendingLen, end);
  pendingCol = end;
}

function truncatePending(col) {
  const head = [];
  let at = 0;
  for (const r of pendingRuns) {
    if (at >= col) break;
    head.push({ text: r.text.slice(0, Math.min(r.text.length, col - at)), cls: r.cls });
    at += r.text.length;
  }
  pendingRuns = head.filter((r) => r.text);
  pendingLen = col;
}

function applyOps(ops) {
  for (const op of ops) {
    switch (op.t) {
      case "text": writeRun(op.s, op.cls); break;
      case "nl": commitPending(true); break;
      case "cr": pendingCol = 0; break;
      case "bs": if (pendingCol > 0) pendingCol -= 1; break;
      case "el": if (pendingCol < pendingLen) truncatePending(pendingCol); break;
      case "el-all":
        pendingRuns = [];
        pendingLen = 0;
        pendingCol = 0;
        break;
      case "clear": clearScreen(); break;
    }
  }
}

// ---- one paint per frame, not one per notification (§15.67) -------------------------
//
// `writeTerminal` used to run to completion inside the `tap.data` handler, and it brackets
// its DOM work with a **read** of `scrollHeight`/`clientHeight`/`scrollTop` and a write of
// `scrollTop`. Reading a scroll metric forces the browser to lay out the whole `<pre>`
// synchronously, and that pane holds up to `TERM_CHAR_CAP` characters — so the cost is
// per *notification* and almost independent of how many bytes the notification carried.
//
// The daemon coalesces on its own schedule, which at console rates is one frame every few
// milliseconds. Measured against a serial node delivering **49 KiB/s** — an ordinary
// 460800-baud device, not a stress case — the shipped client spent **78 % of the main
// thread** on 10 notifications a second, ~120 ms each for 5 KB apiece (23 µs per byte,
// which no amount of text processing accounts for). Work arrived faster than it drained,
// so the event-loop backlog grew without bound: main-thread latency reached **1.1–2.6 s**
// and one console selection took **15.4 s**. The console was not merely behind its device;
// it had stopped answering the operator.
//
// So arrivals are queued and drained once per animation frame. Consecutive text is
// concatenated before parsing — `parseAnsi` is a resumable machine over a character
// stream, so parsing `a + b` is *identical* to parsing `a` then `b`, and in fact strictly
// better: an escape sequence split across two notifications no longer crosses a call
// boundary. Markers keep their place in the order. The whole batch pays **one** layout
// read and one scroll write, whatever it contains.
//
// **The queue cannot stall a hidden tab into unbounded growth**: `requestAnimationFrame`
// does not fire while the document is hidden, so a timer is armed alongside it and
// whichever runs first performs the flush. It cannot grow unboundedly *while visible*
// either, because a flush drains everything queued rather than a fixed slice.
//
// (`renderQueue` and `renderScheduled` are declared with the other module state at the top
// of the file rather than here, beside the code that uses them, for one reason: `let` has a
// temporal dead zone, and `resetTerminal` — declared far above — touches the queue.
// Nothing calls `resetTerminal` during module evaluation today, so the hazard is currently
// unreachable; declaring the state where it cannot bite is cheaper than depending on that
// staying true.)

function scheduleRender() {
  if (renderScheduled) return;
  renderScheduled = true;
  let ran = false;
  const once = () => {
    if (ran) return;
    ran = true;
    renderScheduled = false;
    flushRender();
  };
  // Both, deliberately. rAF is the right cadence when the tab is visible and never fires
  // when it is not; the timer is the floor that keeps a backgrounded console draining.
  if (typeof requestAnimationFrame === "function") requestAnimationFrame(once);
  setTimeout(once, 50);
}

/// Queue decoded device text for the terminal. The stream's bytes are already in
/// `history` by the time this is called; this is presentation only.
function writeTerminal(s) {
  if (!s) return;
  const last = renderQueue[renderQueue.length - 1];
  // Join here rather than in the flush: one growing string beats an array of thousands
  // of small ones when a console is genuinely busy.
  if (last && last.t === "text") last.s += s;
  else renderQueue.push({ t: "text", s });
  scheduleRender();
}

/// Queue an annotation *about* the stream. Ordered against the text around it, so a gap
/// marker still lands exactly where the gap was.
function appendMarker(s) {
  renderQueue.push({ t: "marker", s });
  scheduleRender();
}

function flushRender() {
  if (!renderQueue.length) return;
  const batch = renderQueue;
  renderQueue = [];
  // The one layout read, for the whole batch. Taken before any of the batch's DOM work,
  // so it answers "was the operator at the tail *before* this arrived" exactly as the
  // per-notification version did.
  const atBottom = termEl.scrollTop + termEl.clientHeight >= termEl.scrollHeight - 4;
  let sawMarker = false;
  for (const op of batch) {
    if (op.t === "text") writeTerminalNow(op.s);
    else {
      appendMarkerNow(op.s);
      sawMarker = true;
    }
  }
  renderUnknownBadge();
  // A marker scrolls unconditionally — it is the client speaking, and §5's honesty is
  // only served if the operator sees it — which is what `appendMarker` did before it was
  // batched.
  if (atBottom || sawMarker) termEl.scrollTop = termEl.scrollHeight;
}

/// Everything a flush drops on the queue is discarded here as well as on screen: a
/// selection tears the terminal down synchronously, and text queued for the console the
/// operator just left must not be painted into the one they just chose.
function discardQueuedRender() {
  renderQueue = [];
}

function writeTerminalNow(s) {
  if (!s) return;
  const ops = parseAnsi(ansi, s);
  if (!ops.length) return;
  block = document.createElement("span");
  blockChars = 0;
  applyOps(ops);
  const node = block;
  const chars = blockChars;
  block = null;
  blockChars = 0;
  if (chars > 0) commitNode(node, chars);
  renderPending();
}

// A marker is an annotation *about* the stream, so it lands after the bytes already on
// screen: freeze the incomplete line first rather than inserting in front of it.
function appendMarkerNow(s) {
  commitPending(false);
  const span = document.createElement("span");
  span.className = "marker";
  span.textContent = s;
  commitNode(span, s.length);
  renderPending();
}

// One wording for one kind of hole, wherever it is noticed: bytes that existed in the
// endpoint's offset space and that this client will never see. `lost <= 0` is the
// ordinary case and prints nothing. Note that the marker is a *terminal* annotation
// only — `bytesOf(history)` stays the raw device log the export and the OPFS record are
// meant to be, so nothing here edits the byte stream itself.
function markOffsetGap(lost) {
  if (lost > 0) {
    appendMarker(`\n— ${lost} bytes lost (the daemon's ring rotated past this history) —\n`);
  }
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

clearBtn.onclick = () => {
  if (!selected) return;
  if (!confirm(`Clear stored scrollback for ${selected}?`)) return;
  // Everything below the delete is synchronous, and that is the fix rather than the
  // style: `confirm` blocks the renderer, so a debounced snapshot scheduled before the
  // click comes due while the dialog is up and fires at the first await — with the
  // pre-clear buffer still in `history`, re-creating the very record the operator asked
  // to destroy (HISTC-2). Cancel the debounce and detach the buffer *first*, then ask
  // for the delete.
  if (saveTimer) { clearTimeout(saveTimer); saveTimer = null; }
  // The key comes from the *selection*, not from `historyKey`. A selection in flight has
  // already nulled `historyKey` and re-adopts it only after its awaits — and `confirm`
  // parks that continuation for exactly as long as the dialog is up, so the window cannot
  // close itself while the operator is looking at it. Guarding the delete on `historyKey`
  // therefore made a clear confirmed in that window delete nothing at all, while the
  // resuming selection put the record back on screen (37-WEBC-1). `keyFor` is what that
  // selection will adopt, so this is the same record either way.
  const key = historyKey ?? keyFor(selected);
  clearSeq += 1;
  clearedKey = key;
  // Keep the live frontier and the offset space so the ongoing stream is not
  // re-duplicated after a clear. A selection in flight owns `history` — key and buffer are
  // adopted in one step — so leave it alone rather than handing it a buffer keyed to
  // nothing; it reads `clearSeq` and starts empty.
  if (history) {
    history = fromStored(new Uint8Array(0), history.frontier ?? 0, history.epoch);
  }
  resetTerminal();
  appendMarker("— history cleared —\n");
  // Through the saver, so the delete is ordered after a write that is already inside
  // `createWritable()` instead of racing it.
  if (opfsOk) saver.remove(key);
};

// Persist a final snapshot when the tab is hidden or closed (a reload otherwise loses the
// last debounce window), and start or cancel the tap's grace timer (§17, DEVR-5). The
// listener sits on `document`, where the event is actually dispatched.
document.addEventListener("visibilitychange", () => {
  if (document.hidden) flushSave();
  noteWatchChange();
});
window.addEventListener("pagehide", flushSave);

/// The lock holder a refusal names: the daemon's own answer first, the live `state`
/// second. `null` when nobody holds it — which is a real outcome for a LOCKED refusal (a
/// `send` that timed out behind a queue, an endpoint torn down mid-send, a write the
/// target would not accept), and inventing a name for it is precisely what WEBUI-1 was.
///
/// **The first source is aspirational today and is kept deliberately.** This comment used
/// to say `data.held_by` was filled in by `locked_error`; no such field is emitted
/// anywhere in the workspace (`grep held_by` finds only `core/src/lock.rs`'s prose and an
/// unrelated `held_bytes`), so every holder this function returns comes from the `state`
/// snapshot. That is a *stale claim*, corrected here rather than left standing — but the
/// branch stays, because a refusal that names its own holder is strictly better than one
/// inferred from a snapshot that may be up to 200 ms old, and this is the site that would
/// consume it the day §10 grows it.
function holderOf(display, err) {
  const named = err && err.data ? err.data.held_by : null;
  if (named) return named;
  const ep = endpointsFromState(lastState).find((x) => x.display === display);
  return (ep && ep.lock && ep.lock.holder) || null;
}

// §17 clause 3, as amended by §15.66: "a LOCKED refusal shows the holder by name, and
// whether the line is then sent over that holder follows a **standing, visible operator
// preference** — never an automatic steal the operator did not set."
//
// The affordance used to be a `confirm()` per refused line, and the modal was the wrong
// shape for the question in two ways. It asked at the worst moment — a lock conflict is
// discovered *after* the operator has typed and pressed Enter, so the dialog lands on top
// of the console and has to be dismissed before anything can be read — and it asked the
// same question every time, of an operator whose answer is a property of how they work
// rather than of this particular line. Worse, `confirm()` blocks the renderer, which this
// client has had to design around twice already (37-WEBC-1, HISTC-2): the OPFS clear race
// exists precisely because a dialog can park a continuation for as long as a human takes
// to read it.
//
// So the answer moves to a checkbox beside the send box, **checked by default**. What is
// preserved from §17's original clause is the part that mattered: the steal is never
// silent — it is announced in the terminal, naming the holder that was displaced — and it
// never happens on a refusal that is not a lock conflict. What is given up is the
// per-line confirmation, deliberately and with the reasons above recorded at §15.66.
//
// Every *other* refusal is a different failure and gets the daemon's own words, the
// stance editor.mjs already takes: a structural refusal names the rule it broke, and
// paraphrasing it into "locked by someone" cost the operator the only precise thing they
// had. This goes through `rpcFull`, which exists for exactly that (WEBUI-1).
sendForm.onsubmit = async (e) => {
  e.preventDefault();
  if (!selected) return;
  const endpoint = selected;
  const line = sendLine.value;
  sendLine.value = "";
  // The first attempt never steals, whatever the preference says. The refusal is what
  // carries the holder's name (`data.held_by`), and an operator who has left the box
  // ticked is still owed the sentence saying whose lock was taken — which is only
  // obtainable by being refused once. It costs one round trip on a contended endpoint
  // and nothing at all on an uncontended one, which is every ordinary send.
  const msg = await rpcFull("send", { endpoint, line, steal: false });
  if (msg && !msg.error) return;
  const err = (msg && msg.error) || { code: 0, message: "no reply from the daemon" };
  // A refused line was never delivered, so it belongs back in the box rather than in the
  // bin — unless the operator has already typed something else over it.
  if (sendLine.value === "") sendLine.value = line;

  if (err.code !== APP_LOCKED) {
    appendMarker(`\n— send refused: ${err.message} —\n`);
    return;
  }
  const holder = holderOf(endpoint, err);
  // **No holder, no steal**, whatever the preference says — and this is a rule about the
  // verb, not a caution. `-32003` is the daemon's answer to three different situations
  // (§6): the floor is held by someone else; the target would not accept the write inside
  // the deadline (`targetward backpressure`); and the endpoint was torn down mid-send.
  // Only the first has anything for a steal to take. Retrying the other two with
  // `steal: true` re-runs the identical failure, and — worse — replaces the daemon's own
  // words with a lock narration that was never true, which is WEBUI-1 exactly: the console
  // telling the operator a false story about why their line did not land. So the steal
  // follows the preference only where a holder is actually named; otherwise the refusal is
  // reported in the daemon's words, which is what every other refusal already gets.
  if (!stealBox.checked || !holder) {
    appendMarker(
      holder
        ? `\n— send refused: ${endpoint} is locked by ${holder}; tick "automatically ` +
            `steal during line write" to send over it —\n`
        : `\n— send refused: ${err.message} —\n`,
    );
    return;
  }
  // Announced before the retry, not after: a steal displaces another origin's floor
  // (§6), and §5's honesty says the operator sees that happen rather than inferring it
  // from a line that went through.
  appendMarker(`\n— stealing the write lock from ${holder} —\n`);
  const retry = await rpcFull("send", { endpoint, line, steal: true });
  if (retry && retry.error) {
    // The old code inspected nothing here, so an operator who accepted the steal saw
    // absolutely nothing happen — no delivery, no message, and the line already gone.
    appendMarker(`\n— steal refused: ${retry.error.message} —\n`);
  } else if (sendLine.value === line) {
    sendLine.value = "";
  }
};

const initialHash = location.hash.replace("#", "");
if (initialHash === "graph" || initialHash === "editor") view = initialHash;
renderView();
connect();
