// The editor page (design §17, §15.35): add and remove nodes from a palette,
// connect and disconnect edges.
//
// The load-bearing stance is that **this page enforces nothing**. Every §4 rule —
// orientation pairing, one edge per target endpoint, acyclicity, the codec-mux
// write mode, held-origin uniqueness — is the daemon's to apply, and the editor's
// job is to *surface* the refusal, verbatim, beside the control that caused it. A
// client that pre-validated would grow a second, drifting copy of the rules, and
// the first time the two disagreed the operator would be told the wrong thing.
// So: build a request, send it, show what came back.
//
// The one thing the page does own is the confirmation for a destructive operation,
// and it names what cascades rather than asking "are you sure?" — an operator
// removing a node with three edges attached should be told it is three edges.

/// The node types an operator can add, with the fields each one needs. Kept as
/// data so the palette, the form and the request builder cannot disagree about
/// what a node of a given type looks like.
///
/// `existing-terminal` is deliberately absent: §7.7 specifies it and nothing
/// implements it, and offering a palette entry that always fails is worse than not
/// offering it. Serial nodes take an identity from `ports`, which is the workflow
/// §15.35 exists to close.
export const PALETTE = [
  {
    type: "serial",
    label: "serial port",
    hint: "Owns a physical port exclusively (§7.1).",
    fields: [
      { key: "name", label: "name", required: true },
      { key: "device", label: "device identity", required: true, fromPorts: true },
      { key: "baud", label: "baud", value: "115200", number: true },
    ],
  },
  {
    type: "pty",
    label: "pty console",
    hint: "An interactive pseudo-terminal plus a stable symlink (§7.2).",
    fields: [
      { key: "name", label: "name", required: true },
      { key: "path", label: "symlink path", required: true },
    ],
  },
  {
    type: "log",
    label: "log",
    hint: "Append-only capture, always read-only toward the device (§7.3).",
    fields: [
      { key: "name", label: "name", required: true },
      { key: "directory", label: "directory", required: true },
      { key: "filename", label: "filename", required: true },
    ],
  },
  {
    type: "map",
    label: "character map",
    hint: "picocom's --imap/--omap as a node (§7.8). Comma-separated mapping names.",
    fields: [
      { key: "name", label: "name", required: true },
      { key: "hostward", label: "hostward mappings", list: true },
      { key: "targetward", label: "targetward mappings", list: true },
    ],
  },
  {
    type: "codec",
    label: "codec (demux)",
    hint: "Interior demux/re-mux; channels are comma-separated (§7.5).",
    fields: [
      { key: "name", label: "name", required: true },
      { key: "codec", label: "codec name", value: "reference", required: true },
      { key: "channels", label: "channels", list: true, required: true },
    ],
  },
];

/// Build the `add-node` params for `type` from a `{key: string}` form snapshot.
/// Exported without the DOM so the shape is assertable.
export function nodeParams(type, values) {
  const spec = PALETTE.find((p) => p.type === type);
  if (!spec) throw new Error(`unknown node type ${type}`);
  const node = { type };
  for (const f of spec.fields) {
    const raw = (values[f.key] ?? "").trim();
    if (!raw) {
      if (f.required) throw new Error(`${f.label} is required`);
      // An omitted optional field is *omitted*, never sent as "" — the daemon's
      // schema is `deny_unknown_fields` with real defaults, and an empty string
      // would be a value it has to reject rather than a field it can default.
      continue;
    }
    if (f.list) {
      node[f.key] = raw.split(",").map((s) => s.trim()).filter(Boolean);
    } else if (f.number) {
      const n = Number(raw);
      if (!Number.isFinite(n)) throw new Error(`${f.label} must be a number`);
      node[f.key] = n;
    } else {
      node[f.key] = raw;
    }
  }
  return { node };
}

/// How many edges a node removal would take with it, and their display forms —
/// so the confirmation names what cascades instead of asking "are you sure?".
export function cascadeOf(dump, nodeName) {
  return (dump.edge || [])
    .filter((e) => {
      const ends = [e.a, e.b].map((x) => String(x).split("/")[0]);
      return ends.includes(nodeName);
    })
    .map((e) => `${e.a} ↔ ${e.b}`);
}

/// The last thing the editor said, carried across the re-render its own success
/// triggers. `run` refreshes on success, and the refresh replaces the whole panel —
/// including the status line the message was just written to — so without this an
/// operator sees every refusal and no confirmation at all, which reads exactly like
/// a request that silently failed.
let lastStatus = null;

function el(tag, className, text) {
  const n = document.createElement(tag);
  if (className) n.className = className;
  if (text !== undefined) n.textContent = text;
  return n;
}

/// Render the editor into `root`.
///
/// `ctx` supplies everything that touches the network or the user, so this module
/// stays pure of both: `{ dump, ports, rpc(method, params) -> Promise<{result, error}>,
/// confirm(text) -> bool, refresh() }`.
export function render(root, ctx) {
  root.replaceChildren();
  const status = el("div", "estatus");

  // Report the daemon's own words. A refusal names the rule it broke (§4/§11), and
  // paraphrasing it here would cost the operator the only precise thing they got.
  const say = (ok, text) => {
    lastStatus = { ok, text };
    status.className = `estatus ${ok ? "ok" : "err"}`;
    status.textContent = text;
  };
  if (lastStatus) say(lastStatus.ok, lastStatus.text);
  const run = async (label, method, params) => {
    const r = await ctx.rpc(method, params);
    if (r && r.error) {
      // A structural refusal puts its *first* message in `error.message` and the
      // whole list in `data.errors`, so appending the list wholesale prints the
      // first rule twice. Show the first from `message` and only the ones it does
      // not already carry — an operator who broke two rules should see both, and an
      // operator who broke one should read it once.
      const all = (r.error.data && r.error.data.errors) || [];
      const rest = all.filter((e) => !r.error.message.includes(e));
      const extra = rest.length ? `\n— ${rest.join("\n— ")}` : "";
      say(false, `${label} refused: ${r.error.message}${extra}`);
    } else {
      say(true, `${label} ok`);
      ctx.refresh();
    }
  };

  // ---- add a node -----------------------------------------------------------
  const add = el("section", "epanel");
  add.appendChild(el("h2", null, "Add a node"));
  const typeSel = el("select");
  for (const p of PALETTE) {
    const opt = el("option", null, p.label);
    opt.value = p.type;
    typeSel.appendChild(opt);
  }
  add.appendChild(typeSel);
  const hint = el("p", "ehint");
  const form = el("div", "eform");
  add.appendChild(hint);
  add.appendChild(form);

  const inputs = new Map();
  const buildForm = () => {
    const spec = PALETTE.find((p) => p.type === typeSel.value);
    hint.textContent = spec.hint;
    form.replaceChildren();
    inputs.clear();
    for (const f of spec.fields) {
      const row = el("label", "efield");
      row.appendChild(el("span", null, f.label));
      let input;
      if (f.fromPorts && (ctx.ports || []).length) {
        // The `ports` verb closes the loop §15.35 opens: bind what is plugged in,
        // by the identity the daemon itself would store, without an operator
        // learning a device path out of band. A port already bound is offered but
        // labelled, rather than hidden — the daemon will refuse it and say why.
        input = el("select");
        for (const p of ctx.ports) {
          const opt = el("option", null, `${p.identity} — ${p.description}${p.bound_to ? ` (bound: ${p.bound_to})` : ""}`);
          opt.value = p.identity;
          input.appendChild(opt);
        }
      } else {
        input = el("input");
        input.type = "text";
        if (f.value) input.value = f.value;
      }
      row.appendChild(input);
      form.appendChild(row);
      inputs.set(f.key, input);
    }
  };
  typeSel.onchange = buildForm;
  buildForm();

  const addBtn = el("button", null, "add node");
  addBtn.type = "button";
  addBtn.onclick = async () => {
    const values = {};
    for (const [k, input] of inputs) values[k] = input.value;
    let params;
    try {
      params = nodeParams(typeSel.value, values);
    } catch (e) {
      say(false, String(e.message || e));
      return;
    }
    await run(`add-node ${values.name || ""}`, "add-node", params);
  };
  add.appendChild(addBtn);
  root.appendChild(add);

  // ---- remove a node --------------------------------------------------------
  const remove = el("section", "epanel");
  remove.appendChild(el("h2", null, "Remove a node"));
  const nodeSel = el("select");
  for (const n of ctx.dump.node || []) {
    const opt = el("option", null, `${n.name} (${n.type})`);
    opt.value = n.name;
    nodeSel.appendChild(opt);
  }
  remove.appendChild(nodeSel);
  const rmBtn = el("button", null, "remove node");
  rmBtn.type = "button";
  rmBtn.onclick = async () => {
    const name = nodeSel.value;
    if (!name) return;
    const cascade = cascadeOf(ctx.dump, name);
    if (cascade.length) {
      // Name what goes, not just that something does (§15.35).
      const list = cascade.join("\n  ");
      if (!ctx.confirm(`Removing ${name} also removes ${cascade.length} edge(s):\n  ${list}\n\nProceed?`)) {
        return;
      }
    }
    await run(`remove-node ${name}`, "remove-node", {
      node: name,
      cascade: cascade.length > 0,
    });
  };
  remove.appendChild(rmBtn);
  root.appendChild(remove);

  // ---- connect / disconnect an edge ----------------------------------------
  const wire = el("section", "epanel");
  wire.appendChild(el("h2", null, "Wire an edge"));
  const aIn = el("input");
  const bIn = el("input");
  aIn.type = bIn.type = "text";
  aIn.placeholder = "endpoint a (e.g. usb0)";
  bIn.placeholder = "endpoint b (e.g. console/raw)";
  const modeSel = el("select");
  for (const m of ["(default)", "never", "on-demand", "held"]) {
    const opt = el("option", null, m);
    opt.value = m === "(default)" ? "" : m;
    modeSel.appendChild(opt);
  }
  const wrow = el("div", "eform");
  wrow.appendChild(aIn);
  wrow.appendChild(bIn);
  wrow.appendChild(modeSel);
  wire.appendChild(wrow);
  wire.appendChild(
    el(
      "p",
      "ehint",
      "Order does not matter: orientation comes from the endpoints' facings (§15.3). " +
        "The daemon applies every §4 rule and names the one you broke.",
    ),
  );
  const connBtn = el("button", null, "connect");
  connBtn.type = "button";
  connBtn.onclick = () =>
    run(`connect ${aIn.value} ↔ ${bIn.value}`, "connect", {
      a: aIn.value.trim(),
      b: bIn.value.trim(),
      write_mode: modeSel.value || null,
    });
  wire.appendChild(connBtn);
  root.appendChild(wire);

  // Existing edges, each with its own disconnect — the operator picks one that
  // exists rather than retyping a pair.
  const list = el("section", "epanel");
  list.appendChild(el("h2", null, "Existing edges"));
  for (const e of ctx.dump.edge || []) {
    const row = el("div", "gedge");
    row.appendChild(el("span", "gaddr", e.a));
    row.appendChild(el("span", "garrow", "↔"));
    row.appendChild(el("span", "gaddr", e.b));
    row.appendChild(el("span", "gmode", e.write_mode || "on-demand"));
    const btn = el("button", null, "disconnect");
    btn.type = "button";
    btn.onclick = async () => {
      // Disconnecting a writer that holds the lock changes who may write and drops
      // its un-flushed backlog, so it is confirmed and the daemon reports both.
      if (!ctx.confirm(`Disconnect ${e.a} ↔ ${e.b}?\n\nIf its origin holds the write lock, the lock is released and any un-flushed bytes are purged.`)) {
        return;
      }
      await run(`disconnect ${e.a} ↔ ${e.b}`, "disconnect", { a: e.a, b: e.b });
    };
    row.appendChild(btn);
    list.appendChild(row);
  }
  if (!(ctx.dump.edge || []).length) {
    list.appendChild(el("p", "empty", "No edges yet."));
  }
  root.appendChild(list);

  root.appendChild(status);
}
