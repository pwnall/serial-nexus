// serial_nexus web console client (design §17). A pure browser client of the
// server's WebSocket, which is itself a filtering proxy of the daemon's JSON-RPC
// surface (§10). The per-session token rides the same-origin cookie the bootstrap
// URL set (§15.29), so this code never sees it. The layout is the contract; this
// rendering iterates freely (§15.16).
"use strict";

const consolesEl = document.getElementById("consoles");
const connEl = document.getElementById("conn");
const termEl = document.getElementById("term");
const titleEl = document.getElementById("pane-title");
const lockEl = document.getElementById("pane-lock");
const dropsEl = document.getElementById("pane-drops");
const sendForm = document.getElementById("sendform");
const sendLine = document.getElementById("sendline");
const sendBtn = document.getElementById("sendbtn");

let ws = null;
let nextId = 1;
const pending = new Map();          // id -> resolve
let selected = null;                // selected endpoint display
let currentTap = null;              // active tap id
let lastState = { nodes: [], taps: [] };
const decoder = new TextDecoder("utf-8", { fatal: false });

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
    refreshState();
  };
  ws.onclose = () => {
    connEl.textContent = "disconnected — reload to reconnect";
    connEl.className = "disconnected";
    sendLine.disabled = sendBtn.disabled = true;
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
}

async function selectConsole(display) {
  if (currentTap !== null) { await rpc("tap.close", { tap: currentTap }); currentTap = null; }
  selected = display;
  termEl.textContent = "";
  const res = await rpc("tap.open", { endpoint: display, replay: true });
  if (res) {
    currentTap = res.tap;
    if (res.replay_bytes > 0) appendMarker(`— replay (${res.replay_bytes} bytes) —\n`);
    else appendMarker("— no history (set replay_ring to keep scrollback) —\n");
  }
  renderConsoles();
}

function onTapData(params) {
  if (!params || params.tap !== currentTap) return;
  const bin = atob(params.data);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  appendText(decoder.decode(bytes, { stream: true }));
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
