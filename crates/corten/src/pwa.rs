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
//! `/offline` with a plain offline page unless the app routes it. Icons, screenshots and the app's
//! worker script are files of the static directory, named by the constants
//! [`crate::assets::embed!`] makes, so a misspelled or removed file does not compile; an image
//! served from another site is a `Uri`, as every URL outside the app is.
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
    http::{header, HeaderValue, Uri},
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
    display: Display,
    display_override: Vec<DisplayOverride>,
    orientation: Option<Orientation>,
    id: Option<String>,
    lang: Option<String>,
    dir: Option<Dir>,
    theme_color: Option<String>,
    background_color: Option<String>,
    categories: Vec<String>,
    icons: Vec<Icon>,
    screenshots: Vec<Screenshot>,
    shortcuts: Vec<Shortcut>,
}

/// An icon: where it is, its sizes, its type (from the extension) and its purpose.
type Icon = (Src, String, &'static str, Option<&'static str>);

/// How the installed app's window looks (the manifest's `display`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Display {
    Fullscreen,
    /// Its own window without the browser's controls: the default.
    Standalone,
    MinimalUi,
    Browser,
}

impl Display {
    fn as_str(self) -> &'static str {
        match self {
            Display::Fullscreen => "fullscreen",
            Display::Standalone => "standalone",
            Display::MinimalUi => "minimal-ui",
            Display::Browser => "browser",
        }
    }
}

/// A mode `display_override` asks for before `display`, in order: one of [`Display`]'s, or one
/// only this list may name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayOverride {
    /// The app draws in the title bar's area (desktop).
    WindowControlsOverlay,
    /// The app's window has tabs.
    Tabbed,
    Fullscreen,
    Standalone,
    MinimalUi,
    Browser,
}

impl DisplayOverride {
    fn as_str(self) -> &'static str {
        match self {
            DisplayOverride::WindowControlsOverlay => "window-controls-overlay",
            DisplayOverride::Tabbed => "tabbed",
            DisplayOverride::Fullscreen => "fullscreen",
            DisplayOverride::Standalone => "standalone",
            DisplayOverride::MinimalUi => "minimal-ui",
            DisplayOverride::Browser => "browser",
        }
    }
}

/// The orientation the installed app opens in (the manifest's `orientation`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Any,
    Natural,
    Landscape,
    LandscapePrimary,
    LandscapeSecondary,
    Portrait,
    PortraitPrimary,
    PortraitSecondary,
}

impl Orientation {
    fn as_str(self) -> &'static str {
        match self {
            Orientation::Any => "any",
            Orientation::Natural => "natural",
            Orientation::Landscape => "landscape",
            Orientation::LandscapePrimary => "landscape-primary",
            Orientation::LandscapeSecondary => "landscape-secondary",
            Orientation::Portrait => "portrait",
            Orientation::PortraitPrimary => "portrait-primary",
            Orientation::PortraitSecondary => "portrait-secondary",
        }
    }
}

/// The direction of the manifest's text (`dir`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Ltr,
    Rtl,
    Auto,
}

impl Dir {
    fn as_str(self) -> &'static str {
        match self {
            Dir::Ltr => "ltr",
            Dir::Rtl => "rtl",
            Dir::Auto => "auto",
        }
    }
}

/// A screenshot the browser's install dialog shows: a file of the static directory
/// (`Static::SHOTS_CART_PNG`) or a `Uri` of another site, its size, and, for the richer dialog, the
/// screen it shows and a label. Chrome's richer dialog shows a PNG, JPEG or WebP between 320 and
/// 3840 pixels a side, its longer side at most 2.3 times its shorter: `Wide` ones on a desktop,
/// `Narrow` ones (or ones with no form factor) on a phone. The browser checks the image, not
/// Corten.
///
/// A path written as text does not compile: a file of the app is its constant, so a misspelled
/// one does not compile either.
///
/// ```compile_fail,E0277
/// use corten::pwa::Screenshot;
/// let _ = Screenshot::new("shots/cart.png", "1280x720");
/// ```
///
/// ```
/// use corten::pwa::{FormFactor, Manifest, Screenshot};
/// # use axum::http::Uri;
/// let src = Uri::from_static("https://cdn.example.com/shots/cart.png");
/// let shot = Screenshot::new(src, "1280x720").form_factor(FormFactor::Wide).label("Your cart");
/// let json = Manifest::new("Shop", "Shop").screenshot(shot).to_json();
/// assert!(json.contains(r#""screenshots":[{"src":"https://cdn.example.com/shots/cart.png","sizes":"1280x720","type":"image/png","form_factor":"wide","label":"Your cart"}]"#));
/// ```
#[derive(Clone, Debug)]
pub struct Screenshot {
    src: Src,
    sizes: String,
    mime: &'static str,
    form_factor: Option<FormFactor>,
    label: Option<String>,
}

impl Screenshot {
    pub fn new(src: impl Into<IconSrc>, sizes: &str) -> Self {
        let IconSrc(src) = src.into();
        let mime = src.image_type();
        Screenshot { src, sizes: sizes.into(), mime, form_factor: None, label: None }
    }

    /// The screen it shows: Chrome shows `Wide` ones on a desktop and `Narrow` ones on a phone.
    pub fn form_factor(mut self, form_factor: FormFactor) -> Self {
        self.form_factor = Some(form_factor);
        self
    }

    /// What it shows, for assistive technology.
    pub fn label(mut self, text: &str) -> Self {
        self.label = Some(text.into());
        self
    }
}

/// The screen a [`Screenshot`] shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormFactor {
    Wide,
    Narrow,
}

impl FormFactor {
    fn as_str(self) -> &'static str {
        match self {
            FormFactor::Wide => "wide",
            FormFactor::Narrow => "narrow",
        }
    }
}

/// A shortcut the platform offers on the installed app's icon: a name and a page of the app, by its
/// typed route.
///
/// ```
/// use corten::{prelude::*, pwa::{Manifest, Shortcut}};
///
/// #[derive(Route)]
/// #[route("/watchlist")]
/// struct Watchlist;
///
/// let json = Manifest::new("Shop", "Shop").shortcut(Shortcut::new("Watchlist", &Watchlist).short_name("Watch")).to_json();
/// assert!(json.contains(r#""shortcuts":[{"name":"Watchlist","url":"/watchlist","short_name":"Watch"}]"#));
/// ```
///
/// A URL written as text is not a shortcut's page, and neither is another site's, which a
/// browser would drop (a shortcut is a page in the app's scope):
///
/// ```compile_fail,E0277
/// use corten::pwa::Shortcut;
/// let _ = Shortcut::new("Watchlist", "/watchlist");
/// ```
///
/// ```compile_fail,E0277
/// use corten::pwa::Shortcut;
/// let _ = Shortcut::new("Help", &axum::http::Uri::from_static("https://docs.example.com/help"));
/// ```
#[derive(Clone, Debug)]
pub struct Shortcut {
    name: String,
    url: String,
    short_name: Option<String>,
    description: Option<String>,
    icons: Vec<Icon>,
}

impl Shortcut {
    pub fn new(name: &str, page: &(impl crate::route::OnSite + ?Sized)) -> Self {
        Shortcut { name: name.into(), url: crate::route::link(page), short_name: None, description: None, icons: Vec::new() }
    }

    pub fn short_name(mut self, name: &str) -> Self {
        self.short_name = Some(name.into());
        self
    }

    pub fn description(mut self, text: &str) -> Self {
        self.description = Some(text.into());
        self
    }

    /// An icon for the shortcut, as [`Manifest::icon`] takes one (96x96 at least): not a path as
    /// text.
    ///
    /// ```compile_fail,E0277
    /// # #[corten::routes] mod url { #[route("/watchlist", get = page)] pub struct Watchlist; }
    /// # async fn page() -> &'static str { "" }
    /// use corten::pwa::Shortcut;
    /// let _ = Shortcut::new("Watchlist", &url::Watchlist).icon("icons/w.png", "96x96");
    /// ```
    pub fn icon(self, src: impl Into<IconSrc>, sizes: &str) -> Self {
        self.with_icon(src.into(), sizes, None)
    }

    /// An icon with `"purpose": "maskable"`, as [`Manifest::maskable_icon`].
    pub fn maskable_icon(self, src: impl Into<IconSrc>, sizes: &str) -> Self {
        self.with_icon(src.into(), sizes, Some("maskable"))
    }

    fn with_icon(mut self, IconSrc(src): IconSrc, sizes: &str, purpose: Option<&'static str>) -> Self {
        let mime = src.image_type();
        self.icons.push((src, sizes.into(), mime, purpose));
        self
    }
}

/// Where an image (an icon or a screenshot) is: a file of the static directory (`Static::ICONS_192_PNG`), or a `Uri` for one
/// another site serves (`Uri::from_static("https://cdn.example.com/192.png")`), written as is.
#[derive(Clone, Debug)]
pub struct IconSrc(Src);

impl From<Asset> for IconSrc {
    fn from(file: Asset) -> Self {
        IconSrc(Src::File(file))
    }
}

impl From<Uri> for IconSrc {
    fn from(url: Uri) -> Self {
        IconSrc(Src::Url(url.to_string()))
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

    /// Its image type, from the extension (a URL's path's, without its query: `a.webp?v=2`).
    fn image_type(&self) -> &'static str {
        match self {
            Src::File(file) => image_type(file.path()),
            Src::Url(url) => image_type(url.parse::<Uri>().as_ref().map_or(url.as_str(), Uri::path)),
        }
    }

    /// A file's versioned URL in `assets`. A file the app's directory does not serve (no
    /// `App::assets`, or another app's directory) is a startup panic in every build, since no
    /// browser could load it; it runs once, when the app starts.
    fn resolve(&mut self, assets: Option<&Assets>) {
        if let Src::File(file) = self {
            let served = assets.filter(|a| a.owner == Some(file.owner()) && a.contains(file.path()));
            let assets = served.unwrap_or_else(|| {
                panic!(
                    "corten: the PWA names {:?} of {}, and the app's static directory is {} (App::assets)",
                    file.path(),
                    file.owner(),
                    assets.and_then(|a| a.owner).unwrap_or("none")
                )
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
            display: Display::Standalone,
            display_override: Vec::new(),
            orientation: None,
            id: None,
            lang: None,
            dir: None,
            theme_color: None,
            background_color: None,
            categories: Vec::new(),
            icons: Vec::new(),
            screenshots: Vec::new(),
            shortcuts: Vec::new(),
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

    /// How the installed app's window looks: [`Display::Standalone`] unless set.
    pub fn display(mut self, display: Display) -> Self {
        self.display = display;
        self
    }

    /// The modes to try before [`Manifest::display`], in order; a browser uses the first it
    /// supports.
    pub fn display_override(mut self, modes: &[DisplayOverride]) -> Self {
        self.display_override = modes.to_vec();
        self
    }

    pub fn orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = Some(orientation);
        self
    }

    /// The app's identity across changes to its `start_url`: a browser treats two manifests with
    /// the same `id` as one installed app. A path of this site (a typed route, or a
    /// `Local::parse("/app")`), since a browser ignores an `id` on another origin:
    ///
    /// ```compile_fail,E0277
    /// use corten::pwa::Manifest;
    /// let _ = Manifest::new("Shop", "Shop").id("https://other.example/app");
    /// ```
    pub fn id(mut self, id: &(impl crate::route::OnSite + ?Sized)) -> Self {
        self.id = Some(crate::route::link(id));
        self
    }

    /// The language of the manifest's text, a BCP 47 tag (`"en-US"`).
    pub fn lang(mut self, tag: &str) -> Self {
        self.lang = Some(tag.into());
        self
    }

    pub fn dir(mut self, dir: Dir) -> Self {
        self.dir = Some(dir);
        self
    }

    /// Store categories, a hint to app catalogs: `&["shopping", "productivity"]`.
    pub fn categories(mut self, categories: &[&str]) -> Self {
        self.categories = categories.iter().map(|c| c.to_string()).collect();
        self
    }

    /// A screenshot for the install dialog; add one per screen it shows.
    pub fn screenshot(mut self, screenshot: Screenshot) -> Self {
        self.screenshots.push(screenshot);
        self
    }

    /// A shortcut on the installed app's icon; add one per page.
    pub fn shortcut(mut self, shortcut: Shortcut) -> Self {
        self.shortcuts.push(shortcut);
        self
    }

    pub fn background_color(mut self, color: &str) -> Self {
        self.background_color = Some(color.into());
        self
    }

    /// An icon: `(Static::ICONS_192_PNG, "192x192")`, a file of the static directory by the
    /// constant [`crate::assets::embed!`] made for it, or a `Uri` for one another site serves.
    /// The type comes from the extension. Add 192 and 512 at least.
    ///
    /// A path is not an icon, so a misspelled file cannot reach the manifest:
    ///
    /// ```compile_fail,E0277
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
        let mime = src.image_type();
        self.icons.push((src, sizes.into(), mime, purpose));
        self
    }

    /// Every file it names, for [`Pwa::resolve`].
    fn sources(&mut self) -> impl Iterator<Item = &mut Src> {
        let shortcut_icons = self.shortcuts.iter_mut().flat_map(|s| s.icons.iter_mut().map(|icon| &mut icon.0));
        self.icons.iter_mut().map(|icon| &mut icon.0).chain(self.screenshots.iter_mut().map(|s| &mut s.src)).chain(shortcut_icons)
    }

    pub fn to_json(&self) -> String {
        let mut out = format!(
            r#"{{"name":{},"short_name":{},"start_url":{},"scope":{},"display":"{}""#,
            json_string(&self.name),
            json_string(&self.short_name),
            json_string(&self.start_url),
            json_string(&self.scope),
            self.display.as_str()
        );
        if !self.display_override.is_empty() {
            let modes: Vec<String> = self.display_override.iter().map(|m| format!(r#""{}""#, m.as_str())).collect();
            out.push_str(&format!(r#","display_override":[{}]"#, modes.join(",")));
        }
        let words = [("orientation", self.orientation.map(Orientation::as_str)), ("dir", self.dir.map(Dir::as_str))];
        for (key, value) in words {
            if let Some(value) = value {
                out.push_str(&format!(r#","{key}":"{value}""#));
            }
        }
        for (key, value) in [
            ("id", &self.id),
            ("lang", &self.lang),
            ("description", &self.description),
            ("theme_color", &self.theme_color),
            ("background_color", &self.background_color),
        ] {
            if let Some(value) = value {
                out.push_str(&format!(r#","{key}":{}"#, json_string(value)));
            }
        }
        if !self.categories.is_empty() {
            let list: Vec<String> = self.categories.iter().map(|c| json_string(c)).collect();
            out.push_str(&format!(r#","categories":[{}]"#, list.join(",")));
        }
        out.push_str(r#","icons":"#);
        icons(&mut out, &self.icons);
        if !self.screenshots.is_empty() {
            out.push_str(r#","screenshots":["#);
            for (i, shot) in self.screenshots.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&format!(
                    r#"{{"src":{},"sizes":{},"type":{}"#,
                    json_string(&shot.src.url()),
                    json_string(&shot.sizes),
                    json_string(shot.mime)
                ));
                if let Some(form_factor) = shot.form_factor {
                    out.push_str(&format!(r#","form_factor":"{}""#, form_factor.as_str()));
                }
                if let Some(label) = &shot.label {
                    out.push_str(&format!(r#","label":{}"#, json_string(label)));
                }
                out.push('}');
            }
            out.push(']');
        }
        if !self.shortcuts.is_empty() {
            out.push_str(r#","shortcuts":["#);
            for (i, shortcut) in self.shortcuts.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&format!(r#"{{"name":{},"url":{}"#, json_string(&shortcut.name), json_string(&shortcut.url)));
                for (key, value) in [("short_name", &shortcut.short_name), ("description", &shortcut.description)] {
                    if let Some(value) = value {
                        out.push_str(&format!(r#","{key}":{}"#, json_string(value)));
                    }
                }
                if !shortcut.icons.is_empty() {
                    out.push_str(r#","icons":"#);
                    icons(&mut out, &shortcut.icons);
                }
                out.push('}');
            }
            out.push(']');
        }
        out.push('}');
        out
    }
}

/// `[{"src":..,"sizes":..,"type":..}, ..]`.
fn icons(out: &mut String, icons: &[Icon]) {
    out.push('[');
    for (i, (src, sizes, mime, purpose)) in icons.iter().enumerate() {
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
    out.push(']');
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
    /// (`Static::SW_PUSH_JS`), whose versioned URL changes the worker when the file changes. A
    /// file of the app's own directory is same-origin, as a worker script must be; a path is not
    /// one:
    ///
    /// ```compile_fail,E0308
    /// use corten::pwa::{Manifest, Pwa};
    /// let _ = Pwa::new(Manifest::new("Shop", "Shop")).import_script("sw-push.js");
    /// ```
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
    /// static files are cached (see [`Pwa::assets`]). Panics when it names a file the app's static
    /// directory does not serve (none given, or another app's).
    pub fn resolve(mut self, assets: Option<&Assets>) -> Self {
        for file in self.manifest.sources() {
            file.resolve(assets);
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

/// The image's type as the static directory serves it ([`crate::assets`]'s one table); a name
/// that is not an image's is taken for a PNG, as before.
fn image_type(path: &str) -> &'static str {
    match crate::assets::content_type(path) {
        image if image.starts_with("image/") => image,
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
            .icon(Uri::from_static("/i/192.png"), "192x192")
            .maskable_icon(Uri::from_static("https://cdn.example.com/512.webp?v=2"), "512x512");
        assert_eq!(
            m.to_json(),
            concat!(
                r##"{"name":"Card \"Market\"","short_name":"CM","start_url":"/","scope":"/","display":"standalone","theme_color":"#111","##,
                r#""icons":[{"src":"/i/192.png","sizes":"192x192","type":"image/png"},"#,
                r#"{"src":"https://cdn.example.com/512.webp?v=2","sizes":"512x512","type":"image/webp","purpose":"maskable"}]}"#
            )
        );
    }

    #[derive(crate::Route)]
    #[route("/watchlist")]
    struct Watchlist;

    #[test]
    fn the_manifests_other_members_are_written_when_set() {
        let m = Manifest::new("A", "A")
            .id(&crate::route::Local::parse("/").unwrap())
            .lang("en-US")
            .dir(Dir::Ltr)
            .display(Display::MinimalUi)
            .display_override(&[DisplayOverride::WindowControlsOverlay, DisplayOverride::Standalone])
            .orientation(Orientation::PortraitPrimary)
            .categories(&["shopping", "productivity"])
            .screenshot(Screenshot::new(Uri::from_static("/s/wide.webp"), "1280x720").form_factor(FormFactor::Wide).label("Cart"))
            .screenshot(Screenshot::new(Uri::from_static("/s/narrow.png"), "720x1280").form_factor(FormFactor::Narrow))
            .shortcut(
                Shortcut::new("Watchlist", &Watchlist)
                    .short_name("Watch")
                    .description("Cards you follow")
                    .icon(Uri::from_static("/i/w.png"), "96x96")
                    .maskable_icon(Uri::from_static("/i/m.png"), "192x192"),
            );
        assert_eq!(
            m.to_json(),
            concat!(
                r#"{"name":"A","short_name":"A","start_url":"/","scope":"/","display":"minimal-ui","#,
                r#""display_override":["window-controls-overlay","standalone"],"orientation":"portrait-primary","dir":"ltr","#,
                r#""id":"/","lang":"en-US","categories":["shopping","productivity"],"icons":[],"#,
                r#""screenshots":[{"src":"/s/wide.webp","sizes":"1280x720","type":"image/webp","form_factor":"wide","label":"Cart"},"#,
                r#"{"src":"/s/narrow.png","sizes":"720x1280","type":"image/png","form_factor":"narrow"}],"#,
                r#""shortcuts":[{"name":"Watchlist","url":"/watchlist","short_name":"Watch","description":"Cards you follow","#,
                r#""icons":[{"src":"/i/w.png","sizes":"96x96","type":"image/png"},"#,
                r#"{"src":"/i/m.png","sizes":"192x192","type":"image/png","purpose":"maskable"}]}]}"#
            )
        );
    }

    /// Every value each enum writes, as the spec names it.
    #[test]
    fn each_mode_is_written_as_the_spec_names_it() {
        use DisplayOverride as O;
        let display = [Display::Fullscreen, Display::Standalone, Display::MinimalUi, Display::Browser].map(Display::as_str);
        assert_eq!(display, ["fullscreen", "standalone", "minimal-ui", "browser"]);
        let overrides =
            [O::WindowControlsOverlay, O::Tabbed, O::Fullscreen, O::Standalone, O::MinimalUi, O::Browser].map(O::as_str);
        assert_eq!(overrides, ["window-controls-overlay", "tabbed", "fullscreen", "standalone", "minimal-ui", "browser"]);
        let orientations = [
            Orientation::Any,
            Orientation::Natural,
            Orientation::Landscape,
            Orientation::LandscapePrimary,
            Orientation::LandscapeSecondary,
            Orientation::Portrait,
            Orientation::PortraitPrimary,
            Orientation::PortraitSecondary,
        ]
        .map(Orientation::as_str);
        let named = [
            "any",
            "natural",
            "landscape",
            "landscape-primary",
            "landscape-secondary",
            "portrait",
            "portrait-primary",
            "portrait-secondary",
        ];
        assert_eq!(orientations, named);
        assert_eq!([Dir::Ltr, Dir::Rtl, Dir::Auto].map(Dir::as_str), ["ltr", "rtl", "auto"]);
        assert_eq!([FormFactor::Wide, FormFactor::Narrow].map(FormFactor::as_str), ["wide", "narrow"]);
    }

    #[test]
    fn a_screenshot_and_a_shortcuts_icon_are_their_versioned_urls() {
        let assets = assets();
        let m = Manifest::new("A", "A")
            .screenshot(Screenshot::new(ICON, "192x192"))
            .shortcut(Shortcut::new("Watchlist", &Watchlist).icon(ICON, "192x192"));
        let json = Pwa::new(m).resolve(Some(&assets)).manifest.to_json();
        let url = format!(r#""src":"{}icons/192.png""#, assets.prefix());
        assert_eq!(json.matches(&url).count(), 2, "{json}");
    }

    #[test]
    #[should_panic(expected = "of admin::Static, and the app's static directory is shop::Static")]
    fn another_apps_screenshot_panics_in_every_build() {
        let other = Asset::__embedded("admin::Static", "icons/192.png");
        let _ = Pwa::new(Manifest::new("A", "A").screenshot(Screenshot::new(other, "192x192"))).resolve(Some(&assets()));
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
    #[should_panic(expected = "the app's static directory is none")]
    fn a_static_file_needs_the_apps_directory() {
        let _ = Pwa::new(Manifest::new("A", "A").icon(ICON, "192x192")).resolve(None);
    }

    /// An app that lists components but was not given its directory has assets of no owner: the
    /// icon would 404, in a release build too.
    #[test]
    #[should_panic(expected = "the app's static directory is none")]
    fn a_static_file_of_no_directory_panics_in_every_build() {
        let mut assets = assets();
        assets.owner = None;
        let _ = Pwa::new(Manifest::new("A", "A").icon(ICON, "192x192")).resolve(Some(&assets));
    }

    #[test]
    #[should_panic(expected = "of admin::Static, and the app's static directory is shop::Static")]
    fn another_apps_file_panics_in_every_build() {
        let other = Asset::__embedded("admin::Static", "icons/192.png");
        let _ = Pwa::new(Manifest::new("A", "A").icon(other, "192x192")).resolve(Some(&assets()));
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
