//! The compiled-in codec registry as a **value** (design §8, §15.26).
//!
//! Earlier revisions instantiated codecs from a `match` on the codec name baked
//! into the daemon. That made the built-in set a source-edit away and shut out any
//! consumer who could not fork the daemon. §15.26 replaces the match with a
//! [`Registry`] value: [`Registry::with_builtins`] seeds the in-tree codecs (each
//! behind its Cargo feature), and an embedding binary chains
//! [`register`](Registry::register) to add its own — a closed-source codec crate
//! plus a dozen-line custom daemon, with everything else in the ecosystem
//! (`serialnexusctl`, `nexus-sim`, `nexus-doctor`, the scripts) working against it
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

use codec_api::Codec;

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
                    Ok(Box::new(codec_reference::ReferenceCodec::new()) as Box<dyn Codec>)
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

    /// The registered codec names, sorted — for the `info` verb (§10/§15.26) and
    /// for the available-list an unknown-codec error carries.
    pub fn codec_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.factories.keys().cloned().collect();
        names.sort();
        names
    }

    /// Whether `name` names a registered in-process codec (used by the daemon's
    /// structural pre-check so an unknown codec aborts the load with the available
    /// list, §8/§11). The reserved `exec` name is *not* in the registry but is a
    /// valid codec at load time, so callers check it separately.
    pub fn contains(&self, name: &str) -> bool {
        self.factories.contains_key(name)
    }

    /// Build a codec by name at instantiate time (§8). The name was validated by
    /// the daemon's structural pre-check; the factory validates the attribute
    /// schema. An unknown name still errors here (a defensive fallback for direct
    /// callers) with the available list, so no path can silently do nothing.
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

    #[test]
    fn with_builtins_registers_the_reference_codec() {
        let registry = Registry::with_builtins();
        assert!(registry.contains("reference"));
        assert_eq!(registry.codec_names(), vec!["reference".to_owned()]);
    }

    #[test]
    fn register_adds_a_codec_and_sorts_names() {
        let registry = Registry::with_builtins()
            .register("aaa", dummy())
            .expect("fresh name registers");
        // Sorted, so "aaa" precedes "reference".
        assert_eq!(
            registry.codec_names(),
            vec!["aaa".to_owned(), "reference".to_owned()]
        );
    }

    #[test]
    fn a_duplicate_name_is_a_startup_error() {
        let err = Registry::with_builtins()
            .register("reference", dummy())
            .expect_err("reference is already a built-in");
        assert_eq!(err, RegistryError::Duplicate("reference".to_owned()));
    }

    #[test]
    fn the_exec_name_is_reserved() {
        let err = Registry::new()
            .register("exec", dummy())
            .expect_err("exec is reserved for the child-process codec");
        assert_eq!(err, RegistryError::Reserved("exec".to_owned()));
    }

    #[test]
    fn an_unknown_codec_build_names_the_available_list() {
        let registry = Registry::with_builtins();
        // `Box<dyn Codec>` is not `Debug`, so match rather than `expect_err`.
        let err = match registry.build("nope", &toml::Table::new()) {
            Ok(_) => panic!("nope is not a registered codec"),
            Err(e) => e,
        };
        assert!(err.contains("unknown codec"));
        assert!(err.contains("reference"), "the available list is present");
    }
}
