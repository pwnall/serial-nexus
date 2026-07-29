// The graph page (design §17, §15.35): the whole topology, rendered from the RPC
// surface and nothing else.
//
// Two sources, deliberately: `dump` for the *configuration* — node types, facings,
// edges and their declared write modes — and `state` for the *observed* half —
// each node's active/waiting/faulted indicator, its reason, and the lock holder on
// every host-facing endpoint. Keeping them apart in the client is the same split
// §15.8 keeps in the daemon; a page that merged them would be inventing a third
// view of the graph that neither verb actually reports.
//
// Pure rendering: this module owns no network code and no state of its own. It is
// handed the two snapshots and a DOM root, which makes it testable and keeps the
// live-update policy (subscribe-driven, in app.js) in one place.

/// Every endpoint a node exposes, derived from its configured kind (§7). The
/// daemon does not report endpoint *shapes* over RPC — they are a property of the
/// node type, not of its state — so the mapping lives here, beside the renderer
/// that needs it, rather than being guessed from whatever `state` happens to show.
export function endpointsOf(node) {
  const eps = [];
  const host = (name) => eps.push({ name, facing: "host" });
  const target = (name) => eps.push({ name, facing: "target" });
  switch (node.type) {
    case "serial":
      host("");
      break;
    case "pty":
    case "log":
      target("");
      break;
    case "map":
      host("");
      target("raw");
      break;
    case "codec": {
      // A demultiplexer's multiplexed side faces target and its channels face
      // host; a re-multiplexer (`faces = "host"`) is the mirror (§7.5).
      const mux = node.faces === "host" ? host : target;
      const chan = node.faces === "host" ? target : host;
      mux("");
      for (const c of node.channels || []) chan(c);
      break;
    }
    case "leg": {
      // A leg has no default endpoint — its socket lives off-graph (§7.4).
      const chan = node.faces === "host" ? host : target;
      for (const c of node.channels || []) chan(c);
      break;
    }
    default:
      break;
  }
  return eps;
}

/// The display address of an endpoint on `node` (§3): the bare node name for the
/// default endpoint, `node/channel` otherwise.
export function addressOf(nodeName, endpoint) {
  return endpoint ? `${nodeName}/${endpoint}` : nodeName;
}

/// Fold `dump` + `state` into one renderable model. Exported so the shape is
/// assertable without a DOM.
export function model(dump, state) {
  const observed = new Map();
  for (const n of state.nodes || []) observed.set(n.name, n);
  const nodes = (dump.node || []).map((cfg) => {
    const obs = observed.get(cfg.name) || {};
    const locks = new Map();
    if (obs.lock) locks.set(addressOf(cfg.name, ""), obs.lock);
    for (const [ch, cv] of Object.entries(obs.channels || {})) {
      if (cv.lock) locks.set(addressOf(cfg.name, ch), cv.lock);
    }
    return {
      name: cfg.name,
      type: cfg.type,
      status: obs.status || "unknown",
      reason: obs.reason || null,
      endpoints: endpointsOf(cfg).map((ep) => ({
        ...ep,
        address: addressOf(cfg.name, ep.name),
        lock: locks.get(addressOf(cfg.name, ep.name)) || null,
      })),
    };
  });
  const edges = (dump.edge || []).map((e) => ({
    a: e.a,
    b: e.b,
    // An omitted write_mode is the `on-demand` default (§6). Two runtime
    // promotions exist (a log target forced to `never`, a map's raw edge to
    // `held`) and are deliberately NOT re-derived here: that computation has
    // exactly one implementation, in the daemon, and a second one in a browser is
    // how a validator and a runtime come to disagree (invariant 12). The page
    // shows what configuration declares, which is what `dump` round-trips.
    write_mode: e.write_mode || "on-demand",
  }));
  return { nodes, edges };
}

const STATUS_GLYPH = { active: "●", waiting: "◌", faulted: "✕" };

function el(tag, className, text) {
  const n = document.createElement(tag);
  if (className) n.className = className;
  if (text !== undefined) n.textContent = text;
  return n;
}

/// Render the model into `root`, replacing whatever was there.
export function render(root, m) {
  root.replaceChildren();
  if (!m.nodes.length) {
    root.appendChild(el("p", "empty", "The graph is empty. Add a node on the editor page."));
    return;
  }
  const list = el("div", "gnodes");
  for (const n of m.nodes) {
    const card = el("div", `gnode ${n.status}`);
    const head = el("div", "ghead");
    head.appendChild(el("span", `gstatus ${n.status}`, STATUS_GLYPH[n.status] || "?"));
    head.appendChild(el("span", "gname", n.name));
    head.appendChild(el("span", "gtype", n.type));
    card.appendChild(head);
    if (n.reason) card.appendChild(el("div", "greason", n.reason));
    for (const ep of n.endpoints) {
      const row = el("div", `gep ${ep.facing}`);
      row.appendChild(el("span", "gfacing", ep.facing === "host" ? "host ▸" : "▸ target"));
      row.appendChild(el("span", "gaddr", ep.address));
      // A lock badge belongs only on a host-facing endpoint: that is where
      // arbitration lives (§6).
      if (ep.lock && ep.lock.holder) {
        row.appendChild(el("span", "glock", `🔒 ${ep.lock.holder}`));
      }
      const waiters = ep.lock && ep.lock.waiters ? ep.lock.waiters.length : 0;
      if (waiters) row.appendChild(el("span", "gwait", `+${waiters}`));
      card.appendChild(row);
    }
    list.appendChild(card);
  }
  root.appendChild(list);

  const edges = el("div", "gedges");
  edges.appendChild(el("h2", null, `Edges (${m.edges.length})`));
  if (!m.edges.length) {
    edges.appendChild(el("p", "empty", "No edges. Nothing is wired to anything yet."));
  }
  for (const e of m.edges) {
    const row = el("div", "gedge");
    row.appendChild(el("span", "gaddr", e.a));
    row.appendChild(el("span", "garrow", "↔"));
    row.appendChild(el("span", "gaddr", e.b));
    row.appendChild(el("span", "gmode", e.write_mode));
    edges.appendChild(row);
  }
  root.appendChild(edges);
}
