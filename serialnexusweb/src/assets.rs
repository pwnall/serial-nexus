//! Static frontend assets, embedded in the binary (design §17: "no Node toolchain,
//! no bundler"). Plan §11.3 ships a minimal shell that proves the WebSocket bridge;
//! plan §11.4 replaces `INDEX_HTML`/`APP_JS`/`APP_CSS` with the full console UI. The
//! layout is the contract; the rendering iterates freely under §15.16.

/// One served static asset: its bytes and MIME type.
pub struct Asset {
    pub content_type: &'static str,
    pub body: &'static [u8],
}

const INDEX_HTML: &str = include_str!("assets/index.html");
const APP_JS: &str = include_str!("assets/app.js");
const APP_CSS: &str = include_str!("assets/app.css");
// The ES modules app.js imports: the pure offset-splice/retention core and the
// per-key write serializer (§11.9, both unit-tested under `node --test`), and the
// thin OPFS persistence adapter.
const HISTORY_MJS: &str = include_str!("assets/history.mjs");
const SAVER_MJS: &str = include_str!("assets/saver.mjs");
const OPFS_MJS: &str = include_str!("assets/opfs.mjs");

/// Resolve a request path to a static asset, or `None` for a 404. The token/Host
/// gate has already run in the server (§15.29); this only maps paths to bytes.
pub fn lookup(path: &str) -> Option<Asset> {
    match path {
        "/" | "/index.html" => Some(Asset {
            content_type: "text/html; charset=utf-8",
            body: INDEX_HTML.as_bytes(),
        }),
        "/app.js" => Some(Asset {
            content_type: "text/javascript; charset=utf-8",
            body: APP_JS.as_bytes(),
        }),
        "/app.css" => Some(Asset {
            content_type: "text/css; charset=utf-8",
            body: APP_CSS.as_bytes(),
        }),
        "/history.mjs" => Some(Asset {
            content_type: "text/javascript; charset=utf-8",
            body: HISTORY_MJS.as_bytes(),
        }),
        "/saver.mjs" => Some(Asset {
            content_type: "text/javascript; charset=utf-8",
            body: SAVER_MJS.as_bytes(),
        }),
        "/opfs.mjs" => Some(Asset {
            content_type: "text/javascript; charset=utf-8",
            body: OPFS_MJS.as_bytes(),
        }),
        _ => None,
    }
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
}
