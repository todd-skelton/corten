//! Installable app support: a web app manifest, a generated service worker and an offline page.
//!
//! ```ignore
//! let pwa = Pwa::new(Manifest::new("Card Market", "Card Market").icon("/icon-192.png", "192x192", "image/png"))
//!     .offline("/offline")
//!     .assets(&assets)
//!     .network_only("/api/");
//! let app = router.merge(pwa.routes());
//! ```
//!
//! and `const SERVICE_WORKER: Option<&'static str> = Some(corten::pwa::WORKER_PATH);` on the
//! [`crate::Shell`], plus `<link rel="manifest" href="/manifest.webmanifest">` in its template.
//!
//! What the worker does, and why:
//!
//! - Navigations go to the network first (with navigation preload, so the worker adds no
//!   startup delay) and fall back to the cached offline page. Pages are never cached: they
//!   carry the signed-in user's data and a per-request CSP nonce.
//! - Versioned files (`/assets/{version}/...` and the runtime) are served cache-first, and only
//!   responses marked `immutable` are stored, so a mismatched version is never cached.
//! - Everything else (runtime navigation fetches, form actions, feeds, JSON, anything under a
//!   `network_only` prefix) is not touched by the worker at all, so it costs nothing.
//! - A new deploy changes the worker's bytes (the cache name includes the version), the browser
//!   installs it, and old caches are deleted on activate.
//!
//! When the network is gone, the runtime's navigation fetch fails and it reloads the URL as a
//! normal navigation, which the worker answers with the offline page.

use std::sync::Arc;

use axum::{
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};

use crate::{assets::Assets, html::json_string};

/// Where the service worker is served. It must be at the root to control every page.
pub const WORKER_PATH: &str = "/service-worker.js";
/// Where the manifest is served.
pub const MANIFEST_PATH: &str = "/manifest.webmanifest";

/// The web app manifest.
#[derive(Clone, Debug)]
pub struct Manifest {
    name: String,
    short_name: String,
    description: Option<String>,
    start_url: String,
    scope: String,
    display: &'static str,
    theme_color: Option<String>,
    background_color: Option<String>,
    icons: Vec<(String, String, String, Option<&'static str>)>,
}

impl Manifest {
    pub fn new(name: &str, short_name: &str) -> Self {
        Manifest {
            name: name.into(),
            short_name: short_name.into(),
            description: None,
            start_url: "/".into(),
            scope: "/".into(),
            display: "standalone",
            theme_color: None,
            background_color: None,
            icons: Vec::new(),
        }
    }

    pub fn description(mut self, text: &str) -> Self {
        self.description = Some(text.into());
        self
    }

    pub fn start_url(mut self, url: &str) -> Self {
        self.start_url = url.into();
        self
    }

    pub fn theme_color(mut self, color: &str) -> Self {
        self.theme_color = Some(color.into());
        self
    }

    pub fn background_color(mut self, color: &str) -> Self {
        self.background_color = Some(color.into());
        self
    }

    /// An icon: `("/icons/192.png", "192x192", "image/png")`. Add 192 and 512 at least.
    pub fn icon(mut self, src: &str, sizes: &str, mime: &str) -> Self {
        self.icons.push((src.into(), sizes.into(), mime.into(), None));
        self
    }

    /// An icon with `"purpose": "maskable"` (the platform crops it to its own shape).
    pub fn maskable_icon(mut self, src: &str, sizes: &str, mime: &str) -> Self {
        self.icons.push((src.into(), sizes.into(), mime.into(), Some("maskable")));
        self
    }

    pub fn to_json(&self) -> String {
        let mut out = format!(
            r#"{{"name":{},"short_name":{},"start_url":{},"scope":{},"display":"{}""#,
            json_string(&self.name),
            json_string(&self.short_name),
            json_string(&self.start_url),
            json_string(&self.scope),
            self.display
        );
        for (key, value) in
            [("description", &self.description), ("theme_color", &self.theme_color), ("background_color", &self.background_color)]
        {
            if let Some(value) = value {
                out.push_str(&format!(r#","{key}":{}"#, json_string(value)));
            }
        }
        out.push_str(r#","icons":["#);
        for (i, (src, sizes, mime, purpose)) in self.icons.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(r#"{{"src":{},"sizes":{},"type":{}"#, json_string(src), json_string(sizes), json_string(mime)));
            if let Some(purpose) = purpose {
                out.push_str(&format!(r#","purpose":"{purpose}""#));
            }
            out.push('}');
        }
        out.push_str("]}");
        out
    }
}

/// The manifest plus the service worker policy.
#[derive(Clone, Debug)]
pub struct Pwa {
    manifest: Manifest,
    offline: String,
    precache: Vec<String>,
    network_only: Vec<String>,
    cached_prefixes: Vec<String>,
    version: String,
}

impl Pwa {
    pub fn new(manifest: Manifest) -> Self {
        Pwa {
            manifest,
            offline: "/offline".into(),
            precache: Vec::new(),
            network_only: Vec::new(),
            cached_prefixes: vec!["/corten/".into()],
            version: String::new(),
        }
    }

    /// The page shown for a navigation while offline (default `/offline`). It is fetched and
    /// cached when the worker installs, so it must not need a signed-in user.
    pub fn offline(mut self, path: &str) -> Self {
        self.offline = path.into();
        self
    }

    /// Versioned URLs to cache at install (the CSS and island modules the offline page and the
    /// first screens need). The runtime is always included.
    pub fn precache(mut self, urls: impl IntoIterator<Item = String>) -> Self {
        self.precache.extend(urls);
        self.precache.sort();
        self.precache.dedup();
        self
    }

    /// A path prefix the worker never touches (`/api/`, `/auth/`).
    pub fn network_only(mut self, prefix: &str) -> Self {
        self.network_only.push(prefix.into());
        self
    }

    /// The deploy's asset version ([`crate::assets::Assets::version`]); part of the cache name.
    pub fn version(mut self, version: &str) -> Self {
        self.version = version.into();
        self
    }

    /// Caches files from `assets`: its version names the cache, its URLs are served cache-first,
    /// and its scripts and stylesheets are fetched at install so the offline page has them.
    pub fn assets(mut self, assets: &Assets) -> Self {
        self.cached_prefixes.push(assets.mount_prefix());
        self.version = assets.version().into();
        self.precache(assets.urls().filter(|u| u.ends_with(".js") || u.ends_with(".css")))
    }

    /// The service worker source.
    pub fn worker_js(&self) -> String {
        let list = |items: &[String]| format!("[{}]", items.iter().map(|s| json_string(s)).collect::<Vec<_>>().join(","));
        let mut precache = self.precache.clone();
        precache.push(crate::runtime_path().to_string());
        #[cfg(feature = "solid")]
        precache.push(crate::solid_path().to_string());
        let cache = format!("corten-{}-{}", self.version, crate::runtime_version());
        WORKER_TEMPLATE
            .replace("__CACHE__", &json_string(&cache))
            .replace("__OFFLINE__", &json_string(&self.offline))
            .replace("__PRECACHE__", &list(&precache))
            .replace("__NETWORK_ONLY__", &list(&self.network_only))
            .replace("__CACHED__", &list(&self.cached_prefixes))
    }

    /// `GET /manifest.webmanifest` and `GET /service-worker.js`.
    pub fn routes<S: Clone + Send + Sync + 'static>(&self) -> Router<S> {
        let manifest: Arc<str> = self.manifest.to_json().into();
        let worker: Arc<str> = self.worker_js().into();
        Router::new()
            .route(
                MANIFEST_PATH,
                get(move || {
                    let manifest = manifest.clone();
                    async move { text(&manifest, "application/manifest+json", "public, max-age=3600") }
                }),
            )
            .route(
                WORKER_PATH,
                get(move || {
                    let worker = worker.clone();
                    async move { text(&worker, "text/javascript; charset=utf-8", "no-cache") }
                }),
            )
    }
}

fn text(body: &str, content_type: &'static str, cache: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
            (header::CACHE_CONTROL, HeaderValue::from_static(cache)),
        ],
        body.to_string(),
    )
        .into_response()
}

const WORKER_TEMPLATE: &str = r#"// Generated by corten::pwa. See the module docs for the rules.
const CACHE = __CACHE__;
const OFFLINE = __OFFLINE__;
const PRECACHE = __PRECACHE__;
const NETWORK_ONLY = __NETWORK_ONLY__;
const CACHED = __CACHED__;

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(CACHE).then((cache) => cache.addAll([OFFLINE, ...PRECACHE])).then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      if (self.registration.navigationPreload) await self.registration.navigationPreload.enable();
      for (const key of await caches.keys()) if (key.startsWith("corten-") && key !== CACHE) await caches.delete(key);
      await self.clients.claim();
    })(),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  const url = new URL(request.url);
  if (request.method !== "GET" || url.origin !== self.location.origin) return;
  if (NETWORK_ONLY.some((prefix) => url.pathname.startsWith(prefix))) return;
  if (request.mode === "navigate") {
    event.respondWith(
      (async () => {
        try {
          return (await event.preloadResponse) || (await fetch(request));
        } catch {
          return (await caches.match(OFFLINE)) || Response.error();
        }
      })(),
    );
    return;
  }
  // Only versioned files are cached; everything else is left to the browser.
  if (!CACHED.some((prefix) => url.pathname.startsWith(prefix))) return;
  event.respondWith(
    (async () => {
      const hit = await caches.match(request);
      if (hit) return hit;
      const response = await fetch(request);
      if (response.ok && (response.headers.get("cache-control") || "").includes("immutable")) {
        const copy = response.clone();
        event.waitUntil(caches.open(CACHE).then((cache) => cache.put(request, copy)));
      }
      return response;
    })(),
  );
});
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_json_with_icons() {
        let m = Manifest::new("Card \"Market\"", "CS")
            .theme_color("#111")
            .icon("/i/192.png", "192x192", "image/png")
            .maskable_icon("/i/512.png", "512x512", "image/png");
        assert_eq!(
            m.to_json(),
            concat!(
                r##"{"name":"Card \"Market\"","short_name":"CS","start_url":"/","scope":"/","display":"standalone","theme_color":"#111","##,
                r#""icons":[{"src":"/i/192.png","sizes":"192x192","type":"image/png"},"#,
                r#"{"src":"/i/512.png","sizes":"512x512","type":"image/png","purpose":"maskable"}]}"#
            )
        );
    }

    #[test]
    fn worker_embeds_the_policy_and_changes_with_the_version() {
        let pwa =
            Pwa::new(Manifest::new("A", "A")).precache(["/assets/v1/a.css".to_string()]).network_only("/api/").version("v1");
        let js = pwa.worker_js();
        assert!(js.contains(r#"const OFFLINE = "/offline";"#));
        assert!(js.contains(r#"const NETWORK_ONLY = ["/api/"];"#));
        assert!(js.contains(r#"const CACHED = ["/corten/"];"#));
        assert!(js.contains(&format!(r#"const PRECACHE = ["/assets/v1/a.css","{}""#, crate::runtime_path())));
        assert!(!js.contains("__"), "every placeholder is filled");
        assert_ne!(js, pwa.clone().version("v2").worker_js());
    }
}
