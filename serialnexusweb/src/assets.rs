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
        _ => None,
    }
}
