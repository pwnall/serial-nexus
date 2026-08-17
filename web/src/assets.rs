//! Static frontend assets, embedded in the binary (design §17: "no Node toolchain,
//! no bundler"). Plan §11.3 ships a minimal shell that proves the WebSocket bridge;
//! plan §11.4 replaces `INDEX_HTML`/`APP_JS`/`APP_CSS` with the full console UI. The
//! layout is the contract; the rendering iterates freely under §15.16.

/// One served static asset: its bytes, MIME type, and the validator a browser may
/// present to avoid being sent them again (§15.64).
pub struct Asset {
    pub content_type: &'static str,
    pub body: &'static [u8],
    /// A strong `ETag` value **including its quotes**, ready to be written into a
    /// header and compared byte-for-byte against `If-None-Match`.
    pub etag: &'static str,
}

const INDEX_HTML: &str = include_str!("assets/index.html");
const APP_JS: &str = include_str!("assets/app.js");
const APP_CSS: &str = include_str!("assets/app.css");
// The ES modules app.js imports: the pure offset-splice/retention core and the
// per-key write serializer (plan §11.9, both unit-tested under `node --test`), and the
// thin OPFS persistence adapter.
const HISTORY_MJS: &str = include_str!("assets/history.mjs");
const SAVER_MJS: &str = include_str!("assets/saver.mjs");
const OPFS_MJS: &str = include_str!("assets/opfs.mjs");
// The graph and editor views (§17, §15.35): pure renderers, handed snapshots by
// app.js rather than reaching for the network themselves.
const GRAPH_MJS: &str = include_str!("assets/graph.mjs");
const EDITOR_MJS: &str = include_str!("assets/editor.mjs");
// The minimal ANSI subset §17's implementation stance calls for (review 32 UIR-1):
// a resumable parser that turns console escape sequences into render ops. Pure —
// it never touches the DOM — and unit-tested under `node --test`.
const ANSI_MJS: &str = include_str!("assets/ansi.mjs");

const JS: &str = "text/javascript; charset=utf-8";

/// The served table: request paths, MIME type, body. One row per asset, so the
/// validator below can be derived for every asset by construction rather than
/// remembered per arm — the shape §16.5 asks for wherever two things must agree.
const TABLE: &[(&[&str], &str, &str)] = &[
    (
        &["/", "/index.html"],
        "text/html; charset=utf-8",
        INDEX_HTML,
    ),
    (&["/app.js"], JS, APP_JS),
    (&["/app.css"], "text/css; charset=utf-8", APP_CSS),
    (&["/history.mjs"], JS, HISTORY_MJS),
    (&["/saver.mjs"], JS, SAVER_MJS),
    (&["/opfs.mjs"], JS, OPFS_MJS),
    (&["/graph.mjs"], JS, GRAPH_MJS),
    (&["/editor.mjs"], JS, EDITOR_MJS),
    (&["/ansi.mjs"], JS, ANSI_MJS),
];

/// The `ETag` values, one per [`TABLE`] row, computed once on first use.
///
/// A **content** hash, deliberately, and not a per-run nonce or the crate version
/// (§15.64). A nonce would invalidate every cache entry on every restart — and a
/// restart is the routine event here, since `scripts/bless` reinstalls whenever `sys/`
/// changes — while `CARGO_PKG_VERSION` is wrong in the other direction: it does not move
/// across the dev rebuilds that change these files most often, so a browser would hold a
/// stale console and the tag would swear it was current. The bodies are `include_str!`
/// constants, so their hash is fixed for the life of the binary and changes exactly when
/// the file does.
static ETAGS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

/// FNV-1a, 64-bit. A validator needs to change when the bytes change; it does not need
/// to resist an adversary, and nothing downstream trusts it for anything else — a
/// mismatched tag costs a re-send, which is the unfixed behaviour.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn etags() -> &'static [String] {
    ETAGS.get_or_init(|| {
        TABLE
            .iter()
            .map(|(_, _, body)| format!("\"{:016x}\"", fnv1a(body.as_bytes())))
            .collect()
    })
}

/// Resolve a request path to a static asset, or `None` for a 404. The token/Host
/// gate has already run in the server (§15.29); this only maps paths to bytes.
pub fn lookup(path: &str) -> Option<Asset> {
    let (i, row) = TABLE
        .iter()
        .enumerate()
        .find(|(_, (paths, _, _))| paths.contains(&path))?;
    Some(Asset {
        content_type: row.1,
        body: row.2.as_bytes(),
        etag: etags()[i].as_str(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_module_app_js_imports_is_served() {
        // A 404 on any import breaks the module chain and the console never boots, so
        // the import list is read out of app.js itself rather than restated here.
        let mut found = 0;
        for stmt in APP_JS.split("from \"").skip(1) {
            let spec = stmt.split('"').next().unwrap_or("");
            if !spec.starts_with('/') {
                continue; // not a served absolute specifier
            }
            assert!(
                lookup(spec).is_some(),
                "app.js imports {spec}, which lookup() would 404"
            );
            found += 1;
        }
        assert!(
            found >= 3,
            "expected app.js's module imports, found {found}"
        );
    }

    #[test]
    fn unknown_paths_are_not_assets() {
        assert!(lookup("/nope.mjs").is_none());
        assert!(lookup("/../server.rs").is_none());
    }

    /// §15.64. Two assets sharing a validator would tell a browser holding one that it
    /// already has the other, which serves a console assembled from the wrong files.
    /// Asserted over the whole table, so an asset added tomorrow is covered without
    /// anyone remembering to extend a list — and asserted here, at the pure-logic layer,
    /// as well as over HTTP in `p8_web.rs`, because this is where the property lives.
    #[test]
    fn every_asset_has_its_own_quoted_validator() {
        let mut tags: Vec<&str> = Vec::new();
        for (paths, _, _) in TABLE {
            let a = lookup(paths[0]).expect("a table row resolves");
            assert!(
                a.etag.starts_with('"') && a.etag.ends_with('"') && a.etag.len() > 2,
                "{}'s ETag must be a quoted strong validator, got {:?}",
                paths[0],
                a.etag
            );
            // Aliases of one asset share a body and therefore a tag, by construction.
            for alias in *paths {
                assert_eq!(
                    lookup(alias).expect("alias resolves").etag,
                    a.etag,
                    "{alias} and {} are one asset and must share a validator",
                    paths[0]
                );
            }
            tags.push(a.etag);
        }
        let mut sorted = tags.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            tags.len(),
            "two assets share an ETag: {tags:?}"
        );
    }

    /// The tag has to move when the bytes move, or a browser holds a stale console and
    /// the validator swears it is current — the failure mode `CARGO_PKG_VERSION` would
    /// have had across every dev rebuild. Asserted on the hash rather than through
    /// `lookup`, whose inputs are compile-time constants.
    #[test]
    fn the_validator_follows_the_bytes() {
        assert_ne!(fnv1a(b"a"), fnv1a(b"b"));
        assert_ne!(fnv1a(APP_JS.as_bytes()), fnv1a(APP_CSS.as_bytes()));
        assert_eq!(fnv1a(APP_JS.as_bytes()), fnv1a(APP_JS.as_bytes()));
        // One flipped byte in a long body must change it, which is the case a weak
        // "hash" like a length or a checksum of the first block would fail.
        let mut mutated = APP_JS.as_bytes().to_vec();
        let last = mutated.len() - 1;
        mutated[last] ^= 0x01;
        assert_ne!(fnv1a(APP_JS.as_bytes()), fnv1a(&mutated));
    }
}
