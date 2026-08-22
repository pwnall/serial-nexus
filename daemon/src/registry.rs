//! The compiled-in codec registry as a **value** (design §8, §15.26).
//!
//! Earlier revisions instantiated codecs from a `match` on the codec name baked
//! into the daemon. That made the built-in set a source-edit away and shut out any
//! consumer who could not fork the daemon. §15.26 replaces the match with a
//! [`Registry`] value: [`Registry::with_builtins`] seeds the in-tree codecs (each
//! behind its Cargo feature), and an embedding binary chains
//! [`register`](Registry::register) to add its own — an out-of-tree codec crate
//! plus a dozen-line custom daemon, with everything else in the ecosystem
//! (`serial-nexus-ctl`, `serial-nexus-sim`, `serial-nexus-doctor`, the scripts) working against it
//! unchanged because they speak RPC and the envelope, never the codec list.
//!
//! **No dynamic loading.** Registration is source-level composition: a factory is
//! an ordinary Rust closure, so there is no `dlopen`, no ABI surface, and no
//! runtime-plugin trust boundary (§15.11/§15.26). Collisions and reserved names
//! fail at **startup**, before any configuration is read, so a misconfigured
//! embedder never limps into serving traffic with two codecs fighting over a name.
//!
//! **The exec codec is not here.** `exec` (§7.6) is a child *process*, not an
//! in-process [`Codec`] transform, and is routed to the exec node before the
//! registry is consulted; its name is reserved (see `RESERVED_NAMES`) so an
//! embedder cannot shadow it.

use std::collections::HashMap;
use std::rc::Rc;

use serial_nexus_codec_api::Codec;

/// A factory that builds a fresh in-process codec transform from its (already
/// parsed) attribute table (§8). The factory validates the attributes itself and
/// returns a structural error string on a schema failure — consistent with §11
/// (the load aborts, nothing created). Everything runs on the one runtime thread,
/// so the factory need not be `Send`/`Sync` (hence `Rc`, not `Arc`).
pub type CodecFactory = Rc<dyn Fn(&toml::Table) -> Result<Box<dyn Codec>, String>>;

/// Codec names an embedder may never register, because the daemon gives them a
/// different, built-in meaning. `exec` is a child-process boundary (§7.6/§15.22),
/// handled before the registry is consulted; registering it would be a silent
/// no-op footgun, so it is rejected loudly at startup instead.
pub const RESERVED_NAMES: &[&str] = &["exec"];

/// A registration failure — always a startup error (§8/§15.26), surfaced before
/// any configuration is read so a bad embedding never serves traffic.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// Two factories claimed the same codec name.
    #[error("codec name {0:?} is already registered")]
    Duplicate(String),
    /// The name is reserved for a built-in meaning (e.g. `exec`).
    #[error("codec name {0:?} is reserved and cannot be registered")]
    Reserved(String),
}

/// The set of compiled-in codec factories the daemon can instantiate (§8). Built
/// with [`with_builtins`](Registry::with_builtins) and extended by an embedder via
/// [`register`](Registry::register); handed to [`crate::run`], which shares it
/// (read-only) with the graph so `load`/`add-node` can build codec nodes.
#[derive(Clone, Default)]
pub struct Registry {
    factories: HashMap<String, CodecFactory>,
}

impl std::fmt::Debug for Registry {
    /// The factories are closures (not `Debug`); show the registered names, which
    /// is what an embedder actually wants to see.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("codecs", &self.codec_names())
            .finish()
    }
}

impl Registry {
    /// An empty registry — no codecs at all. Rarely what an embedder wants;
    /// [`with_builtins`](Registry::with_builtins) is the usual starting point.
    pub fn new() -> Self {
        Registry::default()
    }

    /// The registry seeded with every in-tree codec whose Cargo feature is
    /// enabled (§8). The default binary uses exactly this; an embedder starts here
    /// and chains [`register`](Registry::register).
    pub fn with_builtins() -> Self {
        // `mut` is unused in a minimal build with every built-in feature off (§8),
        // where the block below is compiled out — allow that so a codec-less daemon
        // still passes `-D warnings`.
        #[cfg_attr(not(feature = "codec-reference"), allow(unused_mut))]
        let mut registry = Registry::new();
        #[cfg(feature = "codec-reference")]
        {
            // The reference framing codec (§7.5/§9) takes no attributes; a config
            // bearing one is a structural schema failure (the factory says so).
            registry.factories.insert(
                "reference".to_owned(),
                Rc::new(|attributes: &toml::Table| {
                    if !attributes.is_empty() {
                        let keys: Vec<&String> = attributes.keys().collect();
                        return Err(format!(
                            "codec \"reference\" takes no attributes; got {keys:?}"
                        ));
                    }
                    Ok(
                        Box::new(serial_nexus_codec_reference::ReferenceCodec::new())
                            as Box<dyn Codec>,
                    )
                }),
            );
        }
        registry
    }

    /// Register a codec factory under `name`, returning the registry for chaining
    /// (`Registry::with_builtins().register(..)?.register(..)?`). A duplicate name
    /// or a reserved one (`exec`) is a **startup error** (§8/§15.26) — the
    /// embedder's `main` propagates it before calling [`crate::run`], so the daemon
    /// never serves traffic with an ambiguous registry.
    pub fn register<F>(mut self, name: impl Into<String>, factory: F) -> Result<Self, RegistryError>
    where
        F: Fn(&toml::Table) -> Result<Box<dyn Codec>, String> + 'static,
    {
        let name = name.into();
        if RESERVED_NAMES.contains(&name.as_str()) {
            return Err(RegistryError::Reserved(name));
        }
        if self.factories.contains_key(&name) {
            return Err(RegistryError::Duplicate(name));
        }
        self.factories.insert(name, Rc::new(factory));
        Ok(self)
    }

    /// The **registered** codec names, sorted — the factories in this registry and
    /// nothing else. This is the registration-level answer ([`Debug`], embedder
    /// diagnostics); the operator-facing question "which codec names may a
    /// configuration use" is [`usable_codec_names`](Registry::usable_codec_names),
    /// which is a strictly larger set.
    pub fn codec_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.factories.keys().cloned().collect();
        names.sort();
        names
    }

    /// Every codec name a configuration may legally name, sorted: the registered
    /// factories **plus** [`RESERVED_NAMES`] (RV-10).
    ///
    /// `exec` is a usable codec (§7.6 packages it "as an ordinary compiled-in
    /// codec") that is deliberately not a registry entry, because it is a child
    /// *process* rather than an in-process [`Codec`] and is routed before the
    /// registry is consulted. That implementation fact had leaked into two operator
    /// surfaces: `info.codecs`, the discovery surface §15.26 makes normative, and —
    /// sharper, because `docs/rpc/configuration.md` promises the list "names the
    /// codecs that would have worked" — the `data.available` of an unknown-codec
    /// structural error, which answered a `codec = "exe"` typo with a list omitting
    /// the very name the operator wanted. Both now ask this method instead. The
    /// reserved names are unioned rather than inserted into `factories` so
    /// [`register`](Registry::register) still rejects `exec` as reserved and
    /// [`build`](Registry::build) still cannot be handed it.
    pub fn usable_codec_names(&self) -> Vec<String> {
        let mut names = self.codec_names();
        for reserved in RESERVED_NAMES {
            if !names.iter().any(|n| n == reserved) {
                names.push((*reserved).to_owned());
            }
        }
        names.sort();
        names
    }

    /// Whether `name` names a registered in-process codec (used by the daemon's
    /// structural pre-check so an unknown codec aborts the load with the available
    /// list, §8/§11). The reserved `exec` name is *not* in the registry but is a
    /// valid codec at load time, so callers check it separately — and report the
    /// refusal against [`usable_codec_names`](Registry::usable_codec_names), which
    /// includes it.
    pub fn contains(&self, name: &str) -> bool {
        self.factories.contains_key(name)
    }

    /// Build a codec by name at instantiate time (§8). The name was validated by
    /// the daemon's structural pre-check; the factory validates the attribute
    /// schema. An unknown name still errors here (a defensive fallback for direct
    /// callers) with the available list, so no path can silently do nothing.
    ///
    /// The list here is [`codec_names`](Registry::codec_names), deliberately *not*
    /// the usable set: this message answers "which codec could this method have
    /// built", and `exec` is precisely the one it never can.
    pub(crate) fn build(
        &self,
        name: &str,
        attributes: &toml::Table,
    ) -> Result<Box<dyn Codec>, String> {
        match self.factories.get(name) {
            Some(factory) => factory(attributes),
            None => Err(format!(
                "unknown codec {name:?}; available: {:?}",
                self.codec_names()
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy() -> impl Fn(&toml::Table) -> Result<Box<dyn Codec>, String> {
        |_| Err("dummy never builds".to_owned())
    }

    // **Which tests here may name `reference`, and why that is a `cfg` rather than a
    // convention (plan §18 item 102).** §8 puts every built-in codec behind its own
    // Cargo feature "so minimal builds drop what they don't need", and
    // `--no-default-features` is the build `docs/codec-authors.md` tells an in-tree
    // codec author to keep working. So `Registry::with_builtins()` has *two*
    // contracts, not one: the default build seeds `reference`, the minimal build
    // seeds nothing. A test that calls `with_builtins()` with no `#[cfg]` is
    // asserting whichever of the two the person running it happened to compile.
    //
    // Five tests in this module did exactly that, and the minimal build had been
    // red on them — `209 passed; 5 failed; 0 ignored` on Linux 7.0.0-30, and a red
    // lib binary means the doctests never ran either — while the sixth
    // (`the_reference_factory_satisfies_the_kit_attribute_suite`) carried the gate.
    // The pattern was known and applied once out of six. That figure and its
    // both-ends twin are the CI workflow's, taken against a scratch copy of the
    // commit rather than a working tree with other work in flight; read them there,
    // with their scope.
    //
    // **Nothing saw it, and the near-miss is the part worth keeping.** All three of
    // CI's `--no-default-features` clippy invocations omitted `--all-targets` (the
    // workspace clippy standing beside them has it), so no lane so much as
    // *compiled* these test targets. Adding `--all-targets` — which item 102 also
    // does — would not have found this: measured on the unfixed tree, `cargo clippy
    // -p serial-nexus-daemon-bin -p serial-nexus-daemon --no-default-features
    // --all-targets --locked -- -D warnings` exits **0** while the five assertions
    // are still red, because a failing assertion is a run-time fact and clippy never
    // runs a test. Only running the configuration finds *this* class, which is what
    // the `check` job's minimal-daemon test step now does.
    //
    // **The two instruments are complementary, not ordered.** This comment claimed
    // the clippy one was "strictly weaker" and that is measured false. Four plants
    // in this module's minimal-only test fn — one file, one stationary tree,
    // `--no-default-features` throughout; minimal clippy `--all-targets -- -D
    // warnings` first, then `cargo test -p serial-nexus-daemon
    // --no-default-features --locked`:
    //
    //   unused `use std::collections::HashMap;` │ clippy 101 │ test **0**, 214 passed
    //   `codec_names().len() == 0`              │ clippy 101 │ test **0**, 214 passed
    //   `!registry.contains(…)` un-negated      │ clippy **0** │ test 101, 1 failed
    //   `let _plant: u32 = "…";`                │ clippy 101 │ test 101
    //
    // Read it by axis. *Compiling* the minimal test targets is the one thing both
    // do, and there `cargo test` subsumes clippy (row 4). *Linting* them is clippy's
    // alone: the workspace table sets `clippy::all = "warn"` and denies only
    // `unused_must_use`, so `cargo test` compiles both lint plants green — row 2
    // with no diagnostic at all, rustc having no `len_zero` lint to emit. *Running*
    // them is `cargo test`'s alone, and row 3 is this module's own defect class
    // re-planted: an assertion that states the default build's contract inside the
    // minimal build. Neither instrument contains the other; dropping either loses a
    // whole axis, so CI keeps both.
    //
    // The one asymmetry worth naming, because it decides where `--all-targets` is
    // load-bearing rather than merely cheap: on Linux its *compile* half is indeed
    // redundant with the test step (row 4 reddens both) and only its *lint* half is
    // unique. On the Apple cross-check's two triples nothing can be run at all, so
    // there both halves are unique and that invocation is the only instrument in CI
    // that so much as looks at this code.
    //
    // The split below is by **property**, not by copying each test twice. Everything
    // `Registry` promises regardless of which codecs are compiled in is asserted
    // once, ungated, and so runs in *both* configurations; the built-in set itself
    // is asserted under `#[cfg(feature = "codec-reference")]` for the default build
    // and under `#[cfg(not(...))]` for the minimal one. That minimal arm is not
    // symmetry for its own sake: "`with_builtins()` is empty" is the promise §8
    // makes about a minimal build, and until item 102 no test on any platform
    // asserted it — the five failures were the only thing the configuration said,
    // and they said it where nobody was listening.
    //
    // **Deliberately not done:** driving these from a `BUILT_IN_NAMES` slice picked
    // by `cfg!`. Every assertion looped over an empty slice is vacuously true, so
    // the minimal arm's passing output would equal its not-running output — plan §3
    // rule 22's tell, and the same shape as the vacuous doctor loop notes §3.103
    // replaced. The expected names are written out literally, per configuration.

    /// Sorting belongs to `codec_names()` itself and not to whichever built-ins a
    /// build carries, so it is proven from an empty registry with the two names
    /// registered in *descending* order — which keeps the property in the minimal
    /// build too, instead of borrowing `reference` as a second name.
    #[test]
    fn register_adds_a_codec_and_sorts_names() {
        let registry = Registry::new()
            .register("zzz", dummy())
            .expect("a fresh name registers")
            .register("aaa", dummy())
            .expect("a second fresh name registers");
        // Registered descending, reported ascending.
        assert_eq!(
            registry.codec_names(),
            vec!["aaa".to_owned(), "zzz".to_owned()]
        );
        assert!(
            registry.contains("aaa"),
            "the first registration is queryable"
        );
        assert!(
            registry.contains("zzz"),
            "the second registration is queryable"
        );
    }

    /// The general rule (§8/§15.26). The instance an embedder actually meets — a
    /// collision against a name `with_builtins()` seeded — is
    /// `registering_over_a_built_in_name_is_a_startup_error`, which needs a built-in
    /// to exist and is therefore gated.
    #[test]
    fn a_duplicate_name_is_a_startup_error() {
        let err = Registry::new()
            .register("twice", dummy())
            .expect("a fresh name registers")
            .register("twice", dummy())
            .expect_err("the second registration collides");
        assert_eq!(err, RegistryError::Duplicate("twice".to_owned()));
    }

    /// RV-10: `exec` is a usable codec name, so the operator-facing list names it —
    /// while the registration-level list keeps meaning "registered factories", which
    /// is what `register` and `build` are judged against. Asserted over registries
    /// whose whole contents this test put there, so it holds in a minimal build as
    /// well as the default one.
    #[test]
    fn the_usable_names_include_the_reserved_exec_codec() {
        // An empty registry (a minimal daemon with no built-in codecs, §8) still has
        // `exec` — that is the whole point of the escape hatch.
        assert_eq!(
            Registry::new().usable_codec_names(),
            vec!["exec".to_owned()]
        );

        let registry = Registry::new()
            .register("aaa", dummy())
            .expect("a fresh name registers");
        assert_eq!(
            registry.usable_codec_names(),
            vec!["aaa".to_owned(), "exec".to_owned()],
            "the usable list must name every codec a configuration may use, sorted"
        );
        assert!(
            !registry.codec_names().contains(&"exec".to_owned()),
            "exec must stay out of the registration-level list: it has no factory"
        );
    }

    #[test]
    fn the_exec_name_is_reserved() {
        let err = Registry::new()
            .register("exec", dummy())
            .expect_err("exec is reserved for the child-process codec");
        assert_eq!(err, RegistryError::Reserved("exec".to_owned()));
    }

    /// `build`'s defensive fallback names what it *could* have built. The list is
    /// this test's own registration, so the assertion means the same thing in both
    /// builds; the default build's list is checked against `reference` by
    /// `an_unknown_codec_build_names_the_built_in_reference`.
    #[test]
    fn an_unknown_codec_build_names_the_available_list() {
        let registry = Registry::new()
            .register("aaa", dummy())
            .expect("a fresh name registers");
        // `Box<dyn Codec>` is not `Debug`, so match rather than `expect_err`.
        let err = match registry.build("nope", &toml::Table::new()) {
            Ok(_) => panic!("nope is not a registered codec"),
            Err(e) => e,
        };
        assert!(err.contains("unknown codec"));
        assert!(err.contains("aaa"), "the available list is present");
    }

    /// The default build's built-in set is exactly `reference` (`[features] default
    /// = ["codec-reference"]`). Gated because the minimal build answers the same
    /// question the other way, in
    /// `with_builtins_registers_nothing_when_every_built_in_codec_feature_is_off`.
    #[cfg(feature = "codec-reference")]
    #[test]
    fn with_builtins_registers_the_reference_codec() {
        let registry = Registry::with_builtins();
        assert!(registry.contains("reference"));
        assert_eq!(registry.codec_names(), vec!["reference".to_owned()]);
    }

    #[cfg(feature = "codec-reference")]
    #[test]
    fn register_adds_a_codec_alongside_the_built_in_and_sorts_names() {
        let registry = Registry::with_builtins()
            .register("aaa", dummy())
            .expect("fresh name registers");
        // Sorted, so "aaa" precedes "reference".
        assert_eq!(
            registry.codec_names(),
            vec!["aaa".to_owned(), "reference".to_owned()]
        );
    }

    #[cfg(feature = "codec-reference")]
    #[test]
    fn registering_over_a_built_in_name_is_a_startup_error() {
        let err = Registry::with_builtins()
            .register("reference", dummy())
            .expect_err("reference is already a built-in");
        assert_eq!(err, RegistryError::Duplicate("reference".to_owned()));
    }

    #[cfg(feature = "codec-reference")]
    #[test]
    fn the_usable_names_union_the_reserved_exec_over_the_built_ins() {
        let registry = Registry::with_builtins();
        assert_eq!(
            registry.usable_codec_names(),
            vec!["exec".to_owned(), "reference".to_owned()],
            "the usable list must name every codec a configuration may use, sorted"
        );
        assert!(
            !registry.codec_names().contains(&"exec".to_owned()),
            "exec must stay out of the registration-level list: it has no factory"
        );
    }

    #[cfg(feature = "codec-reference")]
    #[test]
    fn an_unknown_codec_build_names_the_built_in_reference() {
        let registry = Registry::with_builtins();
        // `Box<dyn Codec>` is not `Debug`, so match rather than `expect_err`.
        let err = match registry.build("nope", &toml::Table::new()) {
            Ok(_) => panic!("nope is not a registered codec"),
            Err(e) => e,
        };
        assert!(err.contains("unknown codec"));
        assert!(err.contains("reference"), "the available list is present");
    }

    /// The built-in `reference` factory owes the same attribute contract an
    /// out-of-tree codec owes (§8 clause 12; plan §18 item 32), so it is run through
    /// the kit's own suite rather than a bespoke assertion. `reference` takes no
    /// attributes, so its whole schema is the unknown-key refusal — which is exactly
    /// the arm the suite requires of every codec.
    #[cfg(feature = "codec-reference")]
    #[test]
    fn the_reference_factory_satisfies_the_kit_attribute_suite() {
        use serial_nexus_codec_api::test_support as kit;
        let registry = Registry::with_builtins();
        let factory = registry
            .factories
            .get("reference")
            .expect("the reference codec is a built-in")
            .clone();

        let mut widgets = toml::Table::new();
        widgets.insert("widgets".to_owned(), toml::Value::Integer(3));

        kit::attributes_are_structural(
            |t: &toml::Table| factory(t),
            &[toml::Table::new()],
            &[(widgets, "widgets")],
        );
    }

    /// **The minimal build's own promise, which nothing asserted before plan §18
    /// item 102.** §8 puts each built-in behind a Cargo feature "so minimal builds
    /// drop what they don't need"; `--no-default-features` is that build, and what
    /// it owes is an *empty* `with_builtins()` — not a registry that quietly kept
    /// `reference` anyway, and not a `with_builtins()` that fails to compile. `exec`
    /// survives because it is reserved rather than registered (§7.6/RV-10), and a
    /// configuration naming `reference` now fails structurally with the available
    /// list — which is the whole operator-visible difference between the two builds,
    /// and the reason `codec-authors.md` can promise the feature drops the codec.
    #[cfg(not(feature = "codec-reference"))]
    #[test]
    fn with_builtins_registers_nothing_when_every_built_in_codec_feature_is_off() {
        let registry = Registry::with_builtins();
        assert!(
            !registry.contains("reference"),
            "a build with codec-reference off must not carry the reference codec"
        );
        assert_eq!(
            registry.codec_names(),
            Vec::<String>::new(),
            "with_builtins() seeds exactly the built-ins whose feature is on"
        );
        assert_eq!(
            registry.usable_codec_names(),
            vec!["exec".to_owned()],
            "exec is reserved rather than registered, so it survives a minimal build"
        );
        // The operator-facing consequence, stated rather than implied: the config
        // that loads on the default binary is refused here, structurally and by
        // name, instead of the codec silently doing nothing.
        let err = match registry.build("reference", &toml::Table::new()) {
            Ok(_) => panic!("codec-reference is off in this build, so nothing builds it"),
            Err(e) => e,
        };
        assert!(err.contains("unknown codec"));
        assert!(
            err.contains("reference"),
            "the refusal names the codec the configuration asked for"
        );
    }
}
