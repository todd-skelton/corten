//! Installable app support: a web app manifest, a generated service worker and an offline page.
//!
//! ```ignore
//! App::new(routes).pwa(
//!     Pwa::new(Manifest::new("Card Market", "Cards").icon(Static::ICONS_192_PNG, "192x192").icon(Static::ICONS_512_PNG, "512x512"))
//!         .network_only("/api/"),
//! )
//! ```
//!
//! [`crate::App::pwa`] serves the manifest and the worker, links the manifest in every page,
//! registers the worker (not under `corten dev`), caches the static files, and answers
//! `/offline` with a plain offline page unless the app routes it. Icons and the app's worker
//! script are files of the static directory, named by the constants
//! [`crate::assets::embed!`] makes, so a misspelled or removed file does not compile; an icon
//! served from elsewhere is a [`Url`].
//!
//! What the worker does, and why:
//!
//! - Navigations go to the network first (with navigation preload, so the worker adds no
//!   startup delay) and fall back to the cached offline page. The worker never caches pages:
//!   they may carry the signed-in user's data, and the browser's HTTP cache already follows the
//!   page's `Cache-Control` ([`crate::cache`]).
//! - Versioned files (`/assets/{version}/...` and the runtime) are served cache-first, and only
//!   responses marked `immutable` are stored, so a mismatched version is never cached.
//! - Everything else (runtime navigation fetches, form actions, feeds, JSON, anything under a
//!   `network_only` prefix) is not touched by the worker at all, so it costs nothing.
//! - A new deploy changes the worker's bytes (the cache name includes the version), the browser
//!   installs it, and old caches are deleted on activate.
//!
//! When the network is gone, the runtime's navigation fetch fails and it reloads the URL as a
//! normal navigation, which the worker answers with the offline page.
//!
//! The worker is generated, so an app adds its own worker code (push notifications, background
//! sync) with [`Pwa::import_script`]: a same-origin script, loaded with `importScripts` before the
//! worker's own handlers. Sending pushes (VAPID keys, payload encryption, subscription storage)
//! is the app's, with a crate such as `web-push`; the framework only makes room for the handler.

use std::sync::Arc;

use axum::{
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};

use crate::{
    assets::{Asset, Assets},
    html::json_string,
};

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
    icons: Vec<(Src, String, &'static str, Option<&'static str>)>,
}

/// An icon served from outside the static directory, as written: an absolute URL (a CDN) or a
/// path one of the app's routes answers (`"/brand/icon.png"`). Nothing checks it, so prefer a file
/// of the static directory, which the compiler does.
#[derive(Clone, Copy, Debug)]
pub struct Url<'a>(pub &'a str);

/// Where an icon is: a file of the static directory (`Static::ICONS_192_PNG`) or a [`Url`].
#[derive(Clone, Debug)]
pub struct IconSrc(Src);

impl From<Asset> for IconSrc {
    fn from(file: Asset) -> Self {
        IconSrc(Src::File(file))
    }
}

impl From<Url<'_>> for IconSrc {
    fn from(url: Url<'_>) -> Self {
        IconSrc(Src::Url(url.0.into()))
    }
}

/// A file of the static directory, until the app's directory gives its versioned URL, or a URL.
#[derive(Clone, Debug)]
enum Src {
    File(Asset),
    Url(String),
}

impl Src {
    /// Its URL: a file not yet resolved is written `/{path}`.
    fn url(&self) -> std::borrow::Cow<'_, str> {
        match self {
            Src::File(file) => format!("/{}", file.path()).into(),
            Src::Url(url) => url.as_str().into(),
        }
    }

    /// A file's versioned URL in `assets`. The app names a file but has no static directory to
    /// serve it (no `App::assets`): a startup panic, since no page could load it.
    fn resolve(&mut self, assets: Option<&Assets>) {
        if let Src::File(file) = self {
            let assets = assets.unwrap_or_else(|| {
                panic!("corten: the PWA names {:?}, and the app has no static directory (App::assets)", file.path())
            });
            *self = Src::Url(assets.asset(*file).to_string());
        }
    }
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

    /// An icon: `(Static::ICONS_192_PNG, "192x192")`, a file of the static directory by the
    /// constant [`crate::assets::embed!`] made for it, or a [`Url`] for one served elsewhere. The
    /// type comes from the extension. Add 192 and 512 at least.
    ///
    /// A path is not an icon, so a misspelled file cannot reach the manifest:
    ///
    /// ```compile_fail
    /// use corten::pwa::Manifest;
    /// let _ = Manifest::new("Shop", "Shop").icon("icons/192.png", "192x192");
    /// ```
    pub fn icon(self, src: impl Into<IconSrc>, sizes: &str) -> Self {
        self.with_icon(src.into(), sizes, None)
    }

    /// An icon with `"purpose": "maskable"` (the platform crops it to its own shape).
    pub fn maskable_icon(self, src: impl Into<IconSrc>, sizes: &str) -> Self {
        self.with_icon(src.into(), sizes, Some("maskable"))
    }

    fn with_icon(mut self, IconSrc(src): IconSrc, sizes: &str, purpose: Option<&'static str>) -> Self {
        let mime = image_type(&src.url());
        self.icons.push((src, sizes.into(), mime, purpose));
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
            out.push_str(&format!(
                r#"{{"src":{},"sizes":{},"type":{}"#,
                json_string(&src.url()),
                json_string(sizes),
                json_string(mime)
            ));
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
    imports: Vec<Src>,
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
            imports: Vec::new(),
        }
    }

    /// The page shown for a navigation while offline (default `/offline`). It is fetched and
    /// cached when the worker installs, so it must not need a signed-in user.
    pub fn offline(mut self, path: &str) -> Self {
        self.offline = path.into();
        self
    }

    /// Versioned URLs to cache at install (the CSS and script modules the offline page and the
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

    /// A script the worker loads first with `importScripts`, for the app's own worker code (a
    /// `push` handler, say): a file of the static directory by its constant
    /// (`Static::SW_PUSH_JS`), whose versioned URL changes the worker when the file changes.
    pub fn import_script(mut self, file: Asset) -> Self {
        self.imports.push(Src::File(file));
        self
    }

    /// The theme color, if the manifest has one.
    pub fn theme_color(&self) -> Option<&str> {
        self.manifest.theme_color.as_deref()
    }

    /// The offline page's path.
    pub fn offline_path(&self) -> &str {
        &self.offline
    }

    /// The icons and scripts named by their static files get those files' versioned URLs, and the
    /// static files are cached (see [`Pwa::assets`]). Panics when it names a file and the app has
    /// no static directory.
    pub fn resolve(mut self, assets: Option<&Assets>) -> Self {
        for icon in &mut self.manifest.icons {
            icon.0.resolve(assets);
        }
        for import in &mut self.imports {
            import.resolve(assets);
        }
        match assets {
            Some(assets) if self.version.is_empty() => self.assets(assets),
            _ => self,
        }
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
        let imports: Vec<String> = self.imports.iter().map(|src| src.url().into_owned()).collect();
        let mut precache = self.precache.clone();
        precache.push(crate::runtime_path().to_string());
        let cache = format!("corten-{}-{}", self.version, crate::runtime_version());
        let imports = if imports.is_empty() { String::new() } else { format!("importScripts(...{});\n", list(&imports)) };
        WORKER_TEMPLATE
            .replace("__IMPORTS__", &imports)
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

fn image_type(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()).as_deref() {
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("avif") => "image/avif",
        Some("ico") => "image/x-icon",
        _ => "image/png",
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
__IMPORTS__const CACHE = __CACHE__;
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

    const ICON: Asset = Asset::__embedded("shop::Static", "icons/192.png");
    const PUSH: Asset = Asset::__embedded("shop::Static", "sw-push.js");

    fn assets() -> Assets {
        let files = ["icons/192.png", "sw-push.js"].map(|f| (f.to_string(), axum::body::Bytes::from_static(b"x"))).to_vec();
        let mut assets = Assets::build_with(files, &[], &[], "/assets").unwrap();
        assets.owner = Some("shop::Static");
        assets
    }

    #[test]
    fn manifest_is_json_with_icons() {
        let m = Manifest::new("Card \"Market\"", "CM")
            .theme_color("#111")
            .icon(Url("/i/192.png"), "192x192")
            .maskable_icon(Url("https://cdn.example.com/512.webp"), "512x512");
        assert_eq!(
            m.to_json(),
            concat!(
                r##"{"name":"Card \"Market\"","short_name":"CM","start_url":"/","scope":"/","display":"standalone","theme_color":"#111","##,
                r#""icons":[{"src":"/i/192.png","sizes":"192x192","type":"image/png"},"#,
                r#"{"src":"https://cdn.example.com/512.webp","sizes":"512x512","type":"image/webp","purpose":"maskable"}]}"#
            )
        );
    }

    #[test]
    fn a_static_file_is_its_versioned_url() {
        let assets = assets();
        let pwa = Pwa::new(Manifest::new("A", "A").icon(ICON, "192x192")).import_script(PUSH).resolve(Some(&assets));
        let icon = format!(r#""icons":[{{"src":"{}icons/192.png","sizes":"192x192","type":"image/png"}}]"#, assets.prefix());
        assert!(pwa.manifest.to_json().contains(&icon), "{}", pwa.manifest.to_json());
        assert!(pwa.worker_js().contains(&format!("importScripts(...[\"{}sw-push.js\"]);", assets.prefix())));
    }

    #[test]
    #[should_panic(expected = "the app has no static directory")]
    fn a_static_file_needs_the_apps_directory() {
        let _ = Pwa::new(Manifest::new("A", "A").icon(ICON, "192x192")).resolve(None);
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
        assert!(!js.contains("importScripts"));
        assert_ne!(js, pwa.clone().version("v2").worker_js());
    }
}
