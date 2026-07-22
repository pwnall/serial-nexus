# Out-of-tree codec template

This directory is a **self-contained workspace** standing in for a *closed-source*
codec repository (design §15.26). It proves that the supported way to ship a
proprietary codec is source-level composition against two small, semver'd
contracts — never a dynamically loaded plugin:

- **`codec/`** — `acme-codec`, a trivial codec depending only on **`codec-api`**
  (the codec trait, event vocabulary, and envelope types). It never depends on the
  daemon, so it can live in a differently-licensed repository.
- **`daemon/`** — `acme-daemon`, a custom daemon binary depending on
  **`nexus-daemon`** (the entry API: run options, the codec `Registry`, version
  constants) plus `acme-codec`. Its `main` is the in-tree `serialnexusd` plus one
  line — `Registry::with_builtins().register("acme", …)` — before `nexus_daemon::run`.

Everything else in the ecosystem — `serialnexusctl`, `nexus-sim`, `nexus-doctor`,
the validation scripts — works against `acme-daemon` unchanged, because they speak
the RPC surface and the envelope, never the codec list (§15.16).

The path dependencies here (`../../../codec-api`, `../../../nexus-daemon`) stand in
for the version pins a real consumer would use against a released open core. This
workspace is **excluded** from the root serial_nexus workspace and built from the
consumer's own position by `scripts/validate/phase8/external-codec.sh`, so the
pattern is proven to compile on every push rather than merely promised (plan §10.3).

## Build and run

```sh
cd examples/external-codec
cargo build

# Boot the custom daemon (short socket dir — Unix sockets are ~108-byte bound):
export XDG_RUNTIME_DIR=$(mktemp -d /tmp/acme.XXXXXX)
./target/debug/acme-daemon &

# The daemon reports its own codec alongside the built-ins:
serialnexusctl --json info | jq '.codecs'      # ["acme","reference"]
```

A config may then name `codec = "acme"` on a `[[node]]` of `type = "codec"`.
