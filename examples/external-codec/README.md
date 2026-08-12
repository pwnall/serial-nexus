# Out-of-tree codec template

This directory is a **self-contained workspace** standing in for an *out-of-tree*
codec repository (design §15.26). It proves that the supported way to ship a
proprietary codec is source-level composition against two small, semver'd
contracts — never a dynamically loaded plugin:

- **`codec/`** — `acme-codec`, a trivial codec depending only on **`serial-nexus-codec-api`**
  (the codec trait, event vocabulary, and envelope types). It never depends on the
  daemon, so it can live in a differently-licensed repository.
- **`tinymux/`** — `tinymux-codec`, the second template: a two-channel tag framer with
  parser state and **one attribute**, so it exercises the three conformance-kit suites
  a passthrough cannot — `control_event_round_trip`, `assert_buffer_bounded`, and
  `attributes_are_structural`. It is deliberately *not* an envelope codec: a device's
  own framing is the commoner case, and this shows what it costs (a framer, a resync,
  and the kit still judging you). Copy this one if your codec has channels, state, or
  configuration; copy `acme` if you only need the embedding pattern.
- **`daemon/`** — `acme-daemon`, a custom daemon binary depending on
  **`serial-nexus-daemon`** (the entry API: run options, the codec `Registry`, version
  constants) plus both codec crates. Its `main` is the in-tree `serial-nexus-daemon` plus
  two chained registrations — `Registry::with_builtins().register("acme", …)?.register("tinymux", …)?`
  — before `serial_nexus_daemon::run`. The second shows a factory that takes
  attributes: the opaque table goes straight to the codec's own schema, and the
  factory validates nothing itself.

Everything else in the ecosystem — `serial-nexus-ctl`, `serial-nexus-sim`, `serial-nexus-doctor`,
the `serial-nexus-itest` harness — works against `acme-daemon` unchanged, because they speak
the RPC surface and the envelope, never the codec list (§15.16).

The path dependencies here (`../../../codec-api`, `../../../daemon`) stand in
for the version pins a real consumer would use against a released open core. This
workspace is **excluded** from the root serial_nexus workspace and built from the
consumer's own position by `itest/tests/p8_external_codec.rs`, so the pattern is
proven to compile *and serve* on every push rather than merely promised (plan
§10.3). That gate builds this workspace from its own manifest with `--locked`, runs
both codec crates' conformance-kit tests, boots `acme-daemon`, and asserts the `info`
codec list below, that a `codec = "acme"` node loads, and that a `codec = "tinymux"`
node loads with its attributes — and that a bad attribute table is refused **naming
the key**, with nothing created (§11).

## Build and run

```sh
cd examples/external-codec
cargo build

# Boot the custom daemon (short socket dir — Unix sockets are ~108-byte bound):
export XDG_RUNTIME_DIR=$(mktemp -d /tmp/acme.XXXXXX)
./target/debug/acme-daemon &

# The daemon reports its own codec alongside the built-ins:
serial-nexus-ctl --json info | jq '.codecs'      # ["acme","exec","reference","tinymux"]
```

`exec` is in that list without being in the registry: it is a child-*process*
boundary (§7.6/§15.22) routed before the registry is consulted, and its name is
reserved so an embedder cannot shadow it. `info` answers the operator's question —
which codec names a configuration may legally name — so it unions the reserved names
over the registered ones rather than reporting the registry directly.

A config may then name `codec = "acme"` on a `[[node]]` of `type = "codec"`, or
`codec = "tinymux"` with its one attribute:

```toml
[[node]]
type = "codec"
name = "mux"
codec = "tinymux"
channels = ["console", "trace"]

  [node.attributes]
  channels = ["console", "trace"]   # the codec's own schema: 1..=8 identities, no
                                    # duplicates, no "/" — a bad table is refused at
                                    # load, naming the key, with nothing created
```

The node's `channels` and the codec's `channels` attribute are two different things
that happen to agree here: the node's list declares the graph's channel *endpoints*,
and the attribute tells `tinymux` which wire tag means which identity. A codec whose
framing carried names rather than indices would need no attribute at all — which is
why `acme` has none.
