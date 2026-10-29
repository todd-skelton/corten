//! Static files with one content version in the URL: `/assets/{version}/style.css`.
//!
//! The app's static directory is compiled into the program by [`embed!`], which gives each file
//! a constant, so a page names one by it and a missing file does not compile:
//!
//! ```
//! corten::assets::embed!(pub struct Static = "src/assets/fixture");
//!
//! # use corten::prelude::*;
//! fn head(req: &Req) -> impl Render + '_ {
//!     html! { <link rel="stylesheet" href={req.asset(Static::STYLE_CSS)}> }
//! }
//! let _ = App::new(Routes::new()).assets(Static);
//! ```
//!
//! ```compile_fail,E0599
//! corten::assets::embed!(pub struct Static = "src/assets/fixture");
//!
//! # use corten::prelude::*;
//! fn head(req: &Req) -> impl Render + '_ {
//!     html! { <link rel="stylesheet" href={req.asset(Static::STYLES_CSS)}> }
//! }
//! ```
//!
//! A directory that is not there, or two files with one constant (`item-1.svg` and
//! `item_1.svg`), do not compile, nor does a file name a URL cannot carry as it is (a space,
//! `#`, `?`, `%`; the fixture has `50%.css`):
//!
//! ```compile_fail
//! corten::assets::embed!(pub struct Static = "src/assets/missing");
//! ```
//!
//! ```compile_fail
//! corten::assets::embed!(pub struct Static = "src/assets/clash");
//! ```
//!
//! ```compile_fail
//! corten::assets::embed!(pub struct Static = "src/assets/unsafe");
//! ```
//!
//! Nor does a path outside the directory it names (the crate's root, `..`, an absolute path):
//!
//! ```compile_fail
//! corten::assets::embed!(pub struct Static = "src/..");
//! ```
//!
//! The program is the whole deploy: nothing is read from disk, except under `corten dev`, which
//! reads the directory the macro read, so a file added to it is served without a rebuild. The
//! files are hashed at startup. Every URL carries the
//! hash, so browsers, CDNs and the service worker may cache the files forever, and a deploy
//! that changes any file changes every URL. The version is a directory prefix, not a per-file
//! name, so scripts keep importing each other with relative paths. Handler scripts
//! (`#[component(js)]` and `script!`, compiled into the binary) are served from
//! `scripts/{name}.js` in the same version, the app's typed events from `events.js` (imported as
//! `corten/events`), and the components' stylesheets, joined in the order the app lists them,
//! from `components.css`, except an area's (`Area::components`), which are joined into
//! `sections/{prefix}.css`, and those several areas list, joined into a file the areas share
//! (`sections/search+categories.css`).
//!
//! During a rolling deploy a page from the old version can ask a new server for an old URL. It
//! gets the current file with `Cache-Control: no-cache`, so nothing caches the mismatch.

use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::body::Bytes;
use axum::{
    extract::Path as UrlPath,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};

use crate::{fnv, Component};

pub use corten_macros::embed;

/// A static directory compiled into the program: the struct [`embed!`] declares. Give it to
/// `App::assets`.
pub trait Embed {
    /// Where the macro read the directory, for `corten dev` to read again: its full path in a
    /// debug build, empty in a release build (which never reads the disk).
    const DIR: &'static str;
    /// The struct's path (`shop::views::Static`), which each of its files carries, so a file of
    /// another app's directory is found in debug builds.
    const OWNER: &'static str;
    /// Each file: its path in the directory (`icons/192.png`) and its bytes.
    const FILES: &'static [(&'static str, &'static [u8])];
}

/// A file of the app's static directory, a constant [`embed!`] makes (`Static::STYLE_CSS`).
/// `req.asset(file)` is its versioned URL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Asset {
    owner: &'static str,
    path: &'static str,
}

impl Asset {
    #[doc(hidden)]
    pub const fn __embedded(owner: &'static str, path: &'static str) -> Self {
        Asset { owner, path }
    }

    /// Its path in the directory, `icons/192.png`.
    pub const fn path(self) -> &'static str {
        self.path
    }
}

/// A static file's versioned URL, written where it is used: `/assets/{version}/style.css`.
#[derive(Clone, Copy, Debug)]
pub struct AssetUrl<'a> {
    prefix: &'a str,
    path: &'a str,
}

impl<'a> AssetUrl<'a> {
    /// `/{path}`, for a file no directory serves.
    pub(crate) const fn unserved(path: &'a str) -> Self {
        AssetUrl { prefix: "/", path }
    }

    fn write(&self, out: &mut String) {
        out.push_str(self.prefix);
        crate::view::Render::render(self.path, out);
    }
}

impl std::fmt::Display for AssetUrl<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.prefix)?;
        f.write_str(self.path)
    }
}

impl crate::view::Render for AssetUrl<'_> {
    fn render(&self, out: &mut String) {
        self.write(out)
    }
}

impl crate::view::AttrValue for AssetUrl<'_> {
    fn write_attr(&self, name: &str, out: &mut String) {
        out.push_str(name);
        out.push_str("=\"");
        self.write(out);
        out.push('"');
    }
}

/// The file the components' stylesheets are joined into.
pub const STYLESHEET: &str = "components.css";

/// The module of the app's typed events, `corten/events`.
pub const EVENTS: &str = "events.js";

/// The module of the app's view classes, `corten/views`.
pub const VIEWS: &str = "views.js";

/// The start of `events.js`: one class per `#[event]`, which bubbles and carries its fields.
const EVENT_CLASS: &str = "// The app's typed events (#[event]), generated by Corten.
const event = (type) =>
  class extends Event {
    constructor(init = {}) {
      super(type, { bubbles: true, composed: true });
      Object.assign(this, init);
    }
  };
";

/// Cache header for a URL that can never change.
pub(crate) const IMMUTABLE: HeaderValue = HeaderValue::from_static("public, max-age=31536000, immutable");

struct File {
    #[cfg(not(feature = "compress"))]
    body: Bytes,
    #[cfg(feature = "compress")]
    body: crate::compress::Precompressed,
    content_type: &'static str,
    hash: u64,
}

/// A script module's tag in a page's head.
struct ModuleTag {
    name: Box<str>,
    tag: Box<str>,
    /// It imports the typed events, so a page that loads it preloads them.
    imports_events: bool,
}

/// A directory of static files served under a versioned prefix. Load it once, at startup.
pub struct Assets {
    mount: String,
    version: String,
    prefix: String,
    scripts_dir: String,
    files: HashMap<String, File>,
    /// Names with a module at `scripts/{name}.js`.
    scripts: std::collections::HashSet<String>,
    /// The components' and `script!` modules: they hand their exports to the runtime as they run, so a
    /// page loads them with a module script of its own.
    registered: std::collections::HashSet<String>,
    /// Each script module's tag in a page's head, sorted by name: a `<script type="module">` for a
    /// registered module, else a `<link rel="modulepreload">`. Made here so a page only looks it up.
    module_tags: Box<[ModuleTag]>,
    /// The views' module preload tag, when the app lists any `#[component(view)]`.
    views_tag: Option<Box<str>>,
    /// The typed events' module preload tag, when a listed script imports any `#[event]`.
    events_tag: Option<Box<str>>,
    /// The joined component stylesheets' URL, when any component has one.
    stylesheet: Option<String>,
    /// Each section stylesheet: the prefixes whose pages link it, its URL and the components in it.
    sections: Vec<Section>,
    /// The `embed!` struct the files came from (`Embed::OWNER`), when the app gave one.
    pub(crate) owner: Option<&'static str>,
}

impl Assets {
    /// Reads every file under `dir` and serves them at `{mount}/{version}/...` (`mount` is
    /// usually `/assets`).
    #[cfg(test)]
    pub(crate) fn load(dir: impl AsRef<Path>, mount: &str) -> io::Result<Self> {
        Self::build(Some(dir.as_ref()), &[], mount)
    }

    /// The files under `dir` (when given), each script at `scripts/{name}.js`, the typed events in
    /// `events.js` and the components' stylesheets joined into `components.css`, served at
    /// `{mount}/{version}/...`. A component listed twice counts once; two different scripts with one
    /// name or two components with one class prefix (`ui::Card` and `admin::Card`) are an error.
    #[cfg(test)]
    pub(crate) fn build(dir: Option<&Path>, components: &[Component], mount: &str) -> io::Result<Self> {
        let files = match dir {
            Some(dir) => read_dir(dir)?,
            None => Vec::new(),
        };
        Self::build_with(files, components, &[], mount)
    }

    /// The files, components and sections, with sections: each section's components are served the same way, but
    /// their stylesheets are joined into `sections/{prefix}.css` instead of `components.css`. A
    /// component several sections list is in one file those sections share,
    /// `sections/{a}+{b}.css`, with the others the same sections share. A component with a
    /// stylesheet is in `components` or in sections, not both, and a token file's (global)
    /// stylesheet only in `components`.
    pub(crate) fn build_with(
        files: Vec<(String, Bytes)>,
        components: &[Component],
        sections: &[(&str, &[Component])],
        mount: &str,
    ) -> io::Result<Self> {
        let mount = mount.trim_end_matches('/');
        assert!(mount.starts_with('/'), "the assets mount is an absolute path, like /assets");
        let mut sources = files;
        sources.reserve(components.len() + 1);
        let clash = |message: String| io::Error::new(io::ErrorKind::AlreadyExists, message);
        // Where each component is listed: 0 is `components`, i + 1 the section i.
        // Each script's typed events follow it, where it is listed.
        let listed = components
            .iter()
            .map(|c| (0, c))
            .chain(sections.iter().enumerate().flat_map(|(i, (_, list))| list.iter().map(move |c| (i + 1, c))))
            .flat_map(|(at, c)| {
                let events = c.script.as_ref().map_or(&[][..], |s| s.events);
                std::iter::once((at, c)).chain(events.iter().map(move |e| (at, e)))
            });
        let place = |at: usize| if at == 0 { "App::components".to_string() } else { format!("the area {}", sections[at - 1].0) };
        // The sections that list each styled component: one that several list is in their shared file.
        let mut listed_in: Vec<(&str, Vec<usize>)> = Vec::new();
        for (i, (_, list)) in sections.iter().enumerate() {
            for c in list.iter().filter(|c| c.style.is_some()) {
                match listed_in.iter_mut().find(|(name, _)| *name == c.name) {
                    Some((_, at)) if !at.contains(&(i + 1)) => at.push(i + 1),
                    Some(_) => {}
                    None => listed_in.push((c.name, vec![i + 1])),
                }
            }
        }
        let mut seen: Vec<(&Component, usize)> = Vec::new();
        // Each stylesheet: the sections that link it (none: `components.css`), its CSS and the
        // components in it.
        let mut sheets: Vec<(Vec<usize>, String, Vec<&'static str>)> = Vec::new();
        let mut events = String::new();
        let mut registered = std::collections::HashSet::new();
        // The scripts that import `corten/events`, so a page that loads one preloads it too.
        let mut imports_events = std::collections::HashSet::new();
        for (at, component) in listed {
            if let Some((same, first)) = seen.iter().find(|(c, _)| c.name == component.name) {
                let script = |c: &Component| c.script.map(|s| s.source);
                let style = |c: &Component| c.style.map(|s| s.css);
                let event = |c: &Component| c.event.map(|e| e.name);
                let view = |c: &Component| c.view.map(|v| v.js);
                if script(same) == script(component)
                    && style(same) == style(component)
                    && event(same) == event(component)
                    && view(same) == view(component)
                {
                    if *first != at && (*first == 0 || at == 0) && component.style.is_some() {
                        return Err(clash(format!(
                            "{} is listed in {} and in {}: App::components links its stylesheet from every page, so list it there only",
                            component.name,
                            place(*first),
                            place(at)
                        )));
                    }
                    continue;
                }
            }
            if let Some(script) = component.script {
                let url = format!("scripts/{}.js", script.name);
                if sources.iter().any(|(u, _)| *u == url) {
                    return Err(clash(format!(
                        "two scripts named {}: {url} is already there when adding {}'s {}",
                        script.name, component.name, script.file
                    )));
                }
                sources.push((url, crate::script::served(script.name, script.source).into_bytes().into()));
                registered.insert(script.name.to_string());
                if !script.events.is_empty() {
                    imports_events.insert(script.name.to_string());
                }
            }
            if let Some(event) = component.event {
                if let Some(other) =
                    seen.iter().filter_map(|(c, _)| c.event).find(|e| e.class == event.class || e.name == event.name)
                {
                    return Err(clash(format!(
                        "two events are {} ({:?}): `corten/events` has one class and the page one event of a name, so rename one",
                        if other.class == event.class { event.class } else { event.name },
                        [other.class, event.class]
                    )));
                }
                if events.is_empty() {
                    events.push_str(EVENT_CLASS);
                }
                events.push_str(&format!("export const {} = event({:?});\n", event.class, event.name));
            }
            if let Some(style) = component.style {
                if at > 0 && style.global() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "{} styles every page (custom properties, themes, utilities): list it in App::components, not in {}",
                            component.name,
                            place(at)
                        ),
                    ));
                }
                if let Some((other, _)) = seen.iter().find(|(c, _)| c.style.is_some_and(|s| s.name == style.name)) {
                    return Err(clash(format!(
                        "{} and {} both style .{}__*: two components named {} in different modules need different names",
                        other.name, component.name, style.name, component.name
                    )));
                }
                let linked = match listed_in.iter().find(|(name, _)| *name == component.name) {
                    Some((_, at)) if !at.is_empty() => {
                        let mut at = at.clone();
                        at.sort_unstable();
                        at
                    }
                    _ => Vec::new(),
                };
                let sheet = match sheets.iter().position(|(s, _, _)| *s == linked) {
                    Some(i) => &mut sheets[i],
                    None => {
                        sheets.push((linked, String::new(), Vec::new()));
                        sheets.last_mut().expect("just pushed")
                    }
                };
                if !sheet.1.is_empty() {
                    sheet.1.push('\n');
                }
                sheet.1.push_str(style.css);
                sheet.2.push(component.name);
            }
            seen.push((component, at));
        }
        // The formatter a handler's format argument needs, for the pages whose handlers take one.
        if !registered.is_empty() {
            let name = crate::script::FORMAT_MODULE;
            let url = format!("scripts/{name}.js");
            if sources.iter().any(|(u, _)| *u == url) {
                return Err(clash(format!("the static directory has a {url}, the name of Corten's script formats")));
            }
            sources.push((url, crate::script::served(name, crate::script::FORMAT_JS).into_bytes().into()));
            registered.insert(name.to_string());
        }
        if !events.is_empty() {
            if sources.iter().any(|(u, _)| u == EVENTS) {
                return Err(clash(format!("the static directory has an {EVENTS}, the name of the typed events' module")));
            }
            sources.push((EVENTS.to_string(), events.into_bytes().into()));
        }
        let all: Vec<Component> = seen.iter().map(|(c, _)| **c).collect();
        if let Some(views) = crate::script::views_module(&all).map_err(clash)? {
            if sources.iter().any(|(u, _)| u == VIEWS) {
                return Err(clash(format!("the static directory has a {VIEWS}, the name of the views' module")));
            }
            sources.push((VIEWS.to_string(), views.into_bytes().into()));
        }
        let styles: Vec<(&str, crate::Style)> = seen.iter().filter_map(|(c, _)| Some((c.name, c.style?))).collect();
        undeclared_properties(&sheets, &sources, &styles, sections)?;
        let css = sheets.iter().position(|(s, _, _)| s.is_empty()).map(|i| sheets.remove(i).1).unwrap_or_default();
        if !css.is_empty() {
            if sources.iter().any(|(u, _)| u == STYLESHEET) {
                return Err(clash(format!("the static directory has a {STYLESHEET}, the name of the components' stylesheet")));
            }
            sources.push((STYLESHEET.to_string(), css.into_bytes().into()));
        }
        // In the order the sections are listed, each section's own file before the files it shares.
        sheets.sort_by_key(|(at, _, _)| (at[0], at.len()));
        let mut section_files = Vec::new();
        for (at, css, components) in sheets {
            let prefixes: Vec<String> = at.iter().map(|i| sections[i - 1].0.to_string()).collect();
            let file = section_file(&prefixes);
            if sources.iter().any(|(u, _)| *u == file) {
                return Err(clash(format!("the static directory has a {file}, the name of a section's stylesheet")));
            }
            sources.push((file.clone(), css.into_bytes().into()));
            section_files.push(Section { prefixes, href: file, components });
        }
        sources.sort_by(|a, b| a.0.cmp(&b.0));
        let mut files = HashMap::new();
        let mut digest = fnv::OFFSET;
        let scripts: std::collections::HashSet<String> = sources
            .iter()
            .filter_map(|(url, _)| url.strip_prefix("scripts/")?.strip_suffix(".js"))
            .filter(|name| !name.contains('/'))
            .map(str::to_string)
            .collect();
        for (url, body) in sources {
            let hash = fnv::hash(&body);
            digest = fnv::extend(digest, url.as_bytes());
            digest = fnv::extend(digest, &hash.to_le_bytes());
            let content_type = content_type(&url);
            #[cfg(feature = "compress")]
            let body = crate::compress::Precompressed::new(body, content_type);
            files.insert(url.clone(), File { body, content_type, hash });
        }
        let version = format!("{:012x}", digest >> 16);
        let prefix = format!("{mount}/{version}/");
        let stylesheet = files.contains_key(STYLESHEET).then(|| format!("{prefix}{STYLESHEET}"));
        let sections = section_files.into_iter().map(|s| Section { href: format!("{prefix}{}", s.href), ..s }).collect();
        let scripts_dir = format!("{prefix}scripts/");
        let mut module_tags: Vec<ModuleTag> = scripts
            .iter()
            .map(|name: &String| {
                let tag = if registered.contains(name) {
                    format!(r#"<script type="module" src="{scripts_dir}{name}.js"></script>"#)
                } else {
                    format!(r#"<link rel="modulepreload" href="{scripts_dir}{name}.js">"#)
                };
                ModuleTag { name: name.as_str().into(), tag: tag.into(), imports_events: imports_events.contains(name) }
            })
            .collect();
        module_tags.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        let views_tag = files.contains_key(VIEWS).then(|| format!(r#"<link rel="modulepreload" href="{prefix}{VIEWS}">"#).into());
        let events_tag =
            files.contains_key(EVENTS).then(|| format!(r#"<link rel="modulepreload" href="{prefix}{EVENTS}">"#).into());
        Ok(Assets {
            module_tags: module_tags.into(),
            views_tag,
            events_tag,
            scripts_dir,
            mount: mount.into(),
            version,
            prefix,
            files,
            scripts,
            registered,
            stylesheet,
            sections,
            owner: None,
        })
    }

    /// Each section stylesheet, in the order the sections are listed.
    pub(crate) fn section_stylesheets(&self) -> &[Section] {
        &self.sections
    }

    /// The URL of the components' joined stylesheet, when a listed component has one.
    pub fn stylesheet(&self) -> Option<&str> {
        self.stylesheet.as_deref()
    }

    /// The URL of `file`. One of another app's directory is a bug, found in debug builds.
    pub fn asset(&self, file: Asset) -> AssetUrl<'_> {
        debug_assert!(
            self.owner == Some(file.owner),
            "corten: {} is a file of {}, and this app's static directory is {} (App::assets)",
            file.path,
            file.owner,
            self.owner.unwrap_or("none")
        );
        AssetUrl { prefix: &self.prefix, path: file.path }
    }

    /// The URL of the file at `path` (`cards/item_1.svg`), a name from data, when there is one.
    pub fn find<'a>(&'a self, path: &'a str) -> Option<AssetUrl<'a>> {
        self.files.contains_key(path).then_some(AssetUrl { prefix: &self.prefix, path })
    }

    /// True when the directory has `path` (relative, like `style.css`).
    pub fn contains(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    /// `{mount}/{version}/`.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// `{mount}/`, the prefix every version shares.
    pub fn mount_prefix(&self) -> String {
        format!("{}/", self.mount)
    }

    /// True when there is a script module `name` (a component's or a `script!` module, or a
    /// file in `scripts/`).
    pub fn has_script(&self, name: &str) -> bool {
        self.scripts.contains(name)
    }

    /// True when the script module `name` hands its exports to the runtime as it runs (a component's or
    /// a `script!` module): a page loads it with `<script type="module">`, not an `import()`.
    pub fn registers(&self, name: &str) -> bool {
        self.registered.contains(name)
    }

    /// The tag that loads the script module `name` with a page, when there is one.
    pub(crate) fn module_tag(&self, name: &str) -> Option<&str> {
        self.module(name).map(|(tag, _)| tag)
    }

    /// The tag that loads the script module `name`, and whether it imports the typed events.
    pub(crate) fn module(&self, name: &str) -> Option<(&str, bool)> {
        let at = self.module_tags.binary_search_by(|m| (*m.name).cmp(name)).ok()?;
        let module = &self.module_tags[at];
        Some((&module.tag, module.imports_events))
    }

    /// The typed events' module preload tag, when a listed script imports any `#[event]`.
    pub(crate) fn events_tag(&self) -> Option<&str> {
        self.events_tag.as_deref()
    }

    /// The views' module preload tag, when the app lists any `#[component(view)]`.
    pub(crate) fn views_tag(&self) -> Option<&str> {
        self.views_tag.as_deref()
    }

    /// Where script modules are served from: `{prefix}scripts/`.
    pub fn scripts_dir(&self) -> &str {
        &self.scripts_dir
    }

    /// The URL of the typed events' module, when a listed script imports any `#[event]`.
    pub fn events(&self) -> Option<String> {
        self.files.contains_key(EVENTS).then(|| format!("{}{EVENTS}", self.prefix))
    }

    /// The URL of the views' module, when the app lists any `#[component(view)]`.
    pub fn views(&self) -> Option<String> {
        self.files.contains_key(VIEWS).then(|| format!("{}{VIEWS}", self.prefix))
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    /// Every file's URL, for a service worker precache list.
    pub fn urls(&self) -> impl Iterator<Item = String> + '_ {
        self.files.keys().map(|path| format!("{}{path}", self.prefix))
    }

    /// The route that serves the files. Merge into the app router.
    pub fn routes<S: Clone + Send + Sync + 'static>(self: &Arc<Self>) -> Router<S> {
        let assets = self.clone();
        Router::new().route(
            &format!("{}/{{version}}/{{*path}}", self.mount),
            get(move |UrlPath((version, path)): UrlPath<(String, String)>, headers: HeaderMap| {
                let assets = assets.clone();
                async move { assets.serve(&version, &path, &headers) }
            }),
        )
    }

    fn serve(&self, version: &str, path: &str, headers: &HeaderMap) -> Response {
        let Some(file) = self.files.get(path) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let cache = if version == self.version { IMMUTABLE } else { HeaderValue::from_static("no-cache") };
        // Each coding of the file is a different representation, so it has its own tag.
        #[cfg(feature = "compress")]
        let (body, coding) = file.body.pick(headers);
        #[cfg(feature = "compress")]
        let etag = coding.map_or_else(|| format!("\"{:016x}\"", file.hash), |c| format!("\"{:016x}-{}\"", file.hash, c.tag()));
        #[cfg(not(feature = "compress"))]
        let (body, etag) = (file.body.clone(), format!("\"{:016x}\"", file.hash));
        let etag = HeaderValue::try_from(etag).expect("hex is a valid header");
        if headers.get(header::IF_NONE_MATCH).is_some_and(|tag| tag == etag) {
            #[allow(unused_mut)]
            let mut res = (StatusCode::NOT_MODIFIED, [(header::ETAG, etag), (header::CACHE_CONTROL, cache)]).into_response();
            #[cfg(feature = "compress")]
            crate::compress::encoded_headers(res.headers_mut(), file.content_type, None);
            return res;
        }
        let head = [
            (header::CONTENT_TYPE, HeaderValue::from_static(file.content_type)),
            (header::CACHE_CONTROL, cache),
            (header::ETAG, etag),
        ];
        #[allow(unused_mut)]
        let mut res = (head, body).into_response();
        #[cfg(feature = "compress")]
        crate::compress::encoded_headers(res.headers_mut(), file.content_type, coding);
        res
    }
}

/// A stylesheet of components only some sections list (`Area::components`).
#[derive(Clone, Debug)]
pub(crate) struct Section {
    /// The prefixes of the sections that list them: pages at or under one link it.
    pub prefixes: Vec<String>,
    /// Its URL.
    pub href: String,
    /// The components whose rules it has.
    pub components: Vec<&'static str>,
}

/// Each `var(--x)` with no fallback in a component's stylesheet (or the token file's) must name
/// a custom property a stylesheet on the same pages declares: the token file's, a component's in
/// `components.css`, a `.css` file in the static directory, or, for an area's component, one in
/// a stylesheet linked from every one of its areas. Otherwise the value is invalid and the
/// property inherits, which shows only as a wrong color in a browser. Checked here because a
/// component's stylesheet is compiled without the theme; this runs once, at startup.
fn undeclared_properties(
    sheets: &[(Vec<usize>, String, Vec<&'static str>)],
    sources: &[(String, Bytes)],
    styles: &[(&str, crate::Style)],
    sections: &[(&str, &[Component])],
) -> io::Result<()> {
    use std::collections::HashSet;
    let declared = |css: &str, into: &mut HashSet<String>| {
        into.extend(crate::style::custom_properties(css).0.into_iter().map(str::to_string));
    };
    let mut everywhere = HashSet::new();
    for (_, body) in sources.iter().filter(|(url, _)| url.ends_with(".css")) {
        // A file that is not text declares nothing.
        if let Ok(css) = std::str::from_utf8(body) {
            declared(css, &mut everywhere);
        }
    }
    for (_, css, _) in sheets.iter().filter(|(at, _, _)| at.is_empty()) {
        declared(css, &mut everywhere);
    }
    // What each area's pages add: the section stylesheets linked from them, its own and those of
    // the areas it is under (`/admin`'s on `/admin/reports`'s pages).
    let in_area: Vec<HashSet<String>> = (1..=sections.len())
        .map(|i| {
            let mut set = HashSet::new();
            let linked = |j: &usize| crate::api::under(sections[j - 1].0, sections[i - 1].0);
            for (_, css, _) in sheets.iter().filter(|(at, _, _)| at.iter().any(linked)) {
                declared(css, &mut set);
            }
            set
        })
        .collect();
    let mut missing = Vec::new();
    for (at, _, names) in sheets {
        for (name, style) in names.iter().filter_map(|n| styles.iter().find(|(s, _)| s == n)) {
            for property in crate::style::custom_properties(style.css).1 {
                if everywhere.contains(property) {
                    continue;
                }
                let lacking: Vec<&str> =
                    at.iter().filter(|i| !in_area[**i - 1].contains(property)).map(|i| sections[i - 1].0).collect();
                let line = if at.is_empty() {
                    format!("{} ({name}) reads var({property}), which no stylesheet declares", style.file)
                } else if !lacking.is_empty() {
                    format!(
                        "{} ({name}) reads var({property}), which no stylesheet on the pages of {} declares",
                        style.file,
                        lacking.join(", ")
                    )
                } else {
                    continue;
                };
                if !missing.contains(&line) {
                    missing.push(line);
                }
            }
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "{}. A custom property comes from tokens!, a component's stylesheet or a .css file in the static directory: fix the name, declare it, or give the var() a fallback (var(--x, ..)) when a script sets it",
            missing.join("; ")
        ),
    ))
}

/// Where a section's stylesheet is served: `sections/admin.css` for `/admin`, and
/// `sections/search+categories.css` for the components `/search` and `/categories` share.
fn section_file(prefixes: &[String]) -> String {
    let names: Vec<&str> = prefixes.iter().map(|p| p.trim_start_matches('/')).collect();
    format!("sections/{}.css", names.join("+"))
}

/// Every file under `dir`, by its path there (`icons/192.png`): for `corten dev`, which reads the
/// directory an [`embed!`] read, so a file added since the build is served.
pub(crate) fn read_dir(dir: &Path) -> io::Result<Vec<(String, Bytes)>> {
    let mut paths = Vec::new();
    collect(dir, dir, &mut paths)?;
    let mut files = Vec::with_capacity(paths.len());
    for (url, path) in paths {
        files.push((url, Bytes::from(std::fs::read(&path)?)));
    }
    Ok(files)
}

/// The files [`embed!`] compiled in, with no copy.
pub(crate) fn embedded<E: Embed>() -> Vec<(String, Bytes)> {
    E::FILES.iter().map(|(path, body)| ((*path).to_string(), Bytes::from_static(body))).collect()
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        // Dotfiles (.env, .DS_Store, editor swap files) are never public.
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect(root, &path, out)?;
        } else if kind.is_symlink() && path.is_dir() {
            // `embed!` refuses a linked directory, so one added under `corten dev` is not served.
            continue;
        } else {
            let relative = path.strip_prefix(root).expect("walked from root");
            let url = relative.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            out.push((url, path));
        }
    }
    Ok(())
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()).as_deref() {
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("webmanifest") => "application/manifest+json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("gif") => "image/gif",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("wasm") => "application/wasm",
        Some("txt") => "text/plain; charset=utf-8",
        Some("xml") => "application/xml",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn dir(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("corten-assets-{name}-{}", std::process::id()));
        for (path, body) in files {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        root
    }

    #[tokio::test]
    async fn versioned_urls_are_immutable_and_old_versions_are_not_cached() {
        let root = dir("serve", &[("style.css", "a{}"), ("scripts/x_y.js", "export function f() {}"), (".env", "SECRET=1")]);
        let assets = Arc::new(Assets::load(&root, "/assets").unwrap());
        assert_eq!(assets.scripts_dir(), format!("/assets/{}/scripts/", assets.version()));
        let app: Router = assets.routes();
        let get = |uri: String| app.clone().oneshot(Request::get(uri).body(Body::empty()).unwrap());
        let res = get(assets.url("style.css")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()["cache-control"], "public, max-age=31536000, immutable");
        assert_eq!(res.headers()["content-type"], "text/css; charset=utf-8");
        let res = get(format!("/assets/{}/scripts/x_y.js", assets.version())).await.unwrap();
        assert_eq!(res.headers()["content-type"], "text/javascript; charset=utf-8");
        let res = get("/assets/000000000000/style.css".into()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()["cache-control"], "no-cache");
        assert_eq!(get(assets.url("style.css").replace("style", "nope")).await.unwrap().status(), StatusCode::NOT_FOUND);
        let traversal = format!("/assets/{}/../Cargo.toml", assets.version());
        assert_eq!(get(traversal).await.unwrap().status(), StatusCode::NOT_FOUND, "only loaded files are served");
        let dotfile = format!("/assets/{}/.env", assets.version());
        assert_eq!(get(dotfile).await.unwrap().status(), StatusCode::NOT_FOUND, "dotfiles are skipped");
    }

    const SCRIPT: Component = Component {
        name: "XY",
        script: Some(crate::Script {
            name: "x_y",
            file: "x_y.js",
            source: "export function f() {}",
            handlers: &["f"],
            events: &[],
        }),
        style: None,
        event: None,
        view: None,
    };

    const fn styled(name: &'static str, prefix: &'static str, css: &'static str) -> Component {
        Component {
            name,
            script: None,
            style: Some(crate::Style { name: prefix, file: "x.css", css, classes: &[] }),
            event: None,
            view: None,
        }
    }

    #[test]
    fn a_name_from_data_is_found_only_when_the_directory_has_it() {
        let files = vec![("cards/item-1.svg".to_string(), Bytes::from_static(b"<svg/>"))];
        let mut assets = Assets::build_with(files, &[], &[], "/assets").unwrap();
        let url = assets.find("cards/item-1.svg").expect("there").to_string();
        assert_eq!(url, format!("{}cards/item-1.svg", assets.prefix()));
        assert!(assets.find("cards/item-2.svg").is_none());
        assets.owner = Some("shop::Static");
        let file = Asset::__embedded("shop::Static", "cards/item-1.svg");
        assert_eq!(assets.asset(file).to_string(), url);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "is a file of admin::Static")]
    fn another_apps_file_is_found_in_debug_builds() {
        let files = vec![("style.css".to_string(), Bytes::from_static(b"a{}"))];
        let mut assets = Assets::build_with(files, &[], &[], "/assets").unwrap();
        assets.owner = Some("shop::Static");
        let _ = assets.asset(Asset::__embedded("admin::Static", "style.css"));
    }

    #[test]
    fn scripts_are_files_of_the_same_version() {
        let root = dir("scripts", &[("style.css", "a{}")]);
        let plain = Assets::load(&root, "/assets").unwrap();
        let with = Assets::build(Some(&root), &[SCRIPT], "/assets").unwrap();
        assert!(with.has_script("x_y") && with.contains("scripts/x_y.js") && !plain.has_script("x_y"));
        assert_ne!(plain.version(), with.version(), "a script change is a new version");
        let only = Assets::build(None, &[SCRIPT], "/assets").unwrap();
        assert!(only.has_script("x_y"), "an app needs no static directory for its scripts");
        assert!(
            only.stylesheet().is_none() && only.events().is_none(),
            "no stylesheet or events without components that have them"
        );
        let clash = dir("clash", &[("scripts/x_y.js", "export default 1")]);
        let error = Assets::build(Some(&clash), &[SCRIPT], "/assets").err().expect("one name, two scripts");
        assert!(error.to_string().contains("two scripts named x_y"), "{error}");
    }

    #[test]
    fn typed_events_are_one_module_of_classes() {
        const TOGGLED: Component = Component {
            name: "RowToggled",
            script: None,
            style: None,
            event: Some(crate::script::EventDef { class: "RowToggled", name: "row-toggled", fields: &[] }),
            view: None,
        };
        let assets = Assets::build(None, &[TOGGLED], "/assets").unwrap();
        assert_eq!(assets.events(), Some(assets.url(EVENTS)));
        let File { hash, .. } = &assets.files[EVENTS];
        assert_ne!(*hash, 0);
        // A script that imports the event brings it: the app lists only the script's component,
        // and listing the event as well changes nothing.
        const GRID: Component = Component {
            name: "Grid",
            script: Some(crate::Script {
                name: "grid",
                file: "grid.js",
                source: "import { RowToggled } from \"corten/events\";\nexport function f() {}",
                handlers: &["f"],
                events: &[TOGGLED],
            }),
            style: None,
            event: None,
            view: None,
        };
        let carried = Assets::build(None, &[GRID], "/assets").unwrap();
        assert_eq!(carried.events(), Some(carried.url(EVENTS)));
        let module = |a: &Assets| {
            #[cfg(feature = "compress")]
            let body = a.files[EVENTS].body.identity();
            #[cfg(not(feature = "compress"))]
            let body = &a.files[EVENTS].body;
            String::from_utf8(body.to_vec()).unwrap()
        };
        assert!(module(&carried).contains("export const RowToggled = event(\"row-toggled\");"), "{}", module(&carried));
        let both = Assets::build(None, &[GRID, TOGGLED], "/assets").unwrap();
        assert_eq!(module(&both), module(&carried));
        // Another event of the same class (an app's own, beside a library's) is an error, not merged.
        const OTHER: Component = Component {
            event: Some(crate::script::EventDef { class: "RowToggled", name: "ui-row-toggled", fields: &[] }),
            ..TOGGLED
        };
        let error = Assets::build(None, &[GRID, OTHER], "/assets").err().expect("two RowToggled");
        assert!(error.to_string().contains("two events are RowToggled"), "{error}");
    }

    #[test]
    fn views_are_one_module_and_one_name_is_one_view() {
        const fn view(js: &'static str) -> Component {
            Component {
                name: "Row",
                script: None,
                style: None,
                event: None,
                view: Some(crate::script::ViewDef { class: "Row", js, ts: "" }),
            }
        }
        const ROW: Component = view("export class Row {}\n");
        let assets = Assets::build(None, &[ROW, ROW], "/assets").unwrap();
        assert_eq!(assets.views(), Some(assets.url(VIEWS)));
        let module = crate::script::views_module(&[ROW, ROW]).unwrap().unwrap();
        assert_eq!(module.matches("export class Row").count(), 1, "listed twice, served once");
        assert!(module.starts_with("// The app's views"), "with the helpers the classes use");
        assert!(module.contains("const{checked,rows,text,value}=") && module.contains("export { rows };"), "and rows()");
        let error = Assets::build(None, &[ROW, view("export class Row { other() {} }\n")], "/assets").err().expect("two Rows");
        assert!(error.to_string().contains("two views named Row"), "{error}");
        assert!(Assets::build(None, &[SCRIPT], "/assets").unwrap().views().is_none());
    }

    #[tokio::test]
    async fn component_stylesheets_are_one_file_in_the_order_listed() {
        let tokens = styled("tokens.json", "tokens.json", ":root{--x:1}");
        let card = styled("Card", "card", ".card__a{color:red}");
        let assets = Arc::new(Assets::build(None, &[tokens, card, SCRIPT, card], "/assets").unwrap());
        let url = assets.stylesheet().expect("a stylesheet").to_string();
        assert_eq!(url, assets.url(STYLESHEET));
        let app: Router = assets.routes();
        let res = app.oneshot(axum::http::Request::get(url).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.headers()["content-type"], "text/css; charset=utf-8");
        let body = http_body_util::BodyExt::collect(res.into_body()).await.unwrap().to_bytes();
        assert_eq!(&body[..], b":root{--x:1}\n.card__a{color:red}", "listed twice, joined once");
        let other = Assets::build(None, &[tokens, styled("Card", "card", ".card__a{color:blue}")], "/assets").unwrap();
        assert_ne!(
            other.version(),
            Assets::build(None, &[tokens, card], "/assets").unwrap().version(),
            "a style change is a new version"
        );
        let error = Assets::build(None, &[card, styled("admin::Card", "card", ".card__b{}")], "/assets")
            .err()
            .expect("one prefix, two components");
        assert!(error.to_string().contains("both style .card__*"), "{error}");
    }

    #[test]
    fn a_stylesheet_reads_only_the_custom_properties_its_pages_declare() {
        let tokens = styled("tokens.json", "tokens.json", ":root{--ink:#111;--space-1:4px}");
        let card = styled("Card", "card", ".card__a{color:var(--ink);padding:var(--space-1)}");
        assert!(Assets::build(None, &[tokens, card], "/assets").is_ok());
        let typo = styled("Card", "card", ".card__a{color:var(--color-primary-contrast);margin:var(--gap, 0)}");
        let error = Assets::build(None, &[tokens, typo], "/assets").err().expect("an undeclared property").to_string();
        assert!(error.contains("x.css (Card) reads var(--color-primary-contrast), which no stylesheet declares"), "{error}");
        assert!(!error.contains("--gap"), "a fallback is enough: {error}");
        let root = dir("declared", &[("style.css", ":root {\n  --color-primary-contrast: white;\n}")]);
        assert!(Assets::build(Some(&root), &[tokens, typo], "/assets").is_ok(), "a static .css file declares them too");

        // An area's component reads what its pages declare, the areas around it included.
        let admin = styled("Admin", "admin", ".admin__a{--admin-ink:red}");
        let report = styled("Report", "report", ".report__a{color:var(--admin-ink)}");
        let nested: &[(&str, &[Component])] = &[("/admin", &[admin]), ("/admin/reports", &[report])];
        assert!(Assets::build_with(Vec::new(), &[tokens], nested, "/assets").is_ok());
        let apart: &[(&str, &[Component])] = &[("/admin", &[admin]), ("/reports", &[report])];
        let error =
            Assets::build_with(Vec::new(), &[tokens], apart, "/assets").err().expect("declared on other pages").to_string();
        assert!(
            error.contains("x.css (Report) reads var(--admin-ink), which no stylesheet on the pages of /reports declares"),
            "{error}"
        );
        let error = Assets::build_with(Vec::new(), &[tokens, report], &[("/admin", &[admin])], "/assets")
            .err()
            .expect("an area's")
            .to_string();
        assert!(error.contains("x.css (Report) reads var(--admin-ink), which no stylesheet declares"), "{error}");
    }

    #[tokio::test]
    async fn a_section_has_a_stylesheet_of_its_own() {
        let tokens = styled("tokens.json", "tokens.json", ":root{--x:1}");
        let card = styled("Card", "card", ".card__a{color:red}");
        let chart = styled("Chart", "chart", ".chart__a{}");
        let table = styled("Table", "table", ".table__a{}");
        let sections: &[(&str, &[Component])] =
            &[("/admin", &[chart, SCRIPT, chart]), ("/admin/reports", &[table]), ("/scripts-only", &[SCRIPT])];
        let assets = Arc::new(Assets::build_with(Vec::new(), &[tokens, card], sections, "/assets").unwrap());
        assert!(assets.has_script("x_y"), "a section's scripts are served like the rest");
        let urls: Vec<_> = assets.section_stylesheets().iter().map(|s| (s.prefixes.join(" "), s.href.clone())).collect();
        assert_eq!(
            urls,
            [
                ("/admin".to_string(), assets.url("sections/admin.css")),
                ("/admin/reports".to_string(), assets.url("sections/admin/reports.css"))
            ],
            "a section without stylesheets has no file"
        );
        let app: Router = assets.routes();
        let body = |url: String| {
            let app = app.clone();
            async move {
                let res = app.oneshot(Request::get(url).body(Body::empty()).unwrap()).await.unwrap();
                http_body_util::BodyExt::collect(res.into_body()).await.unwrap().to_bytes()
            }
        };
        assert_eq!(&body(assets.url(STYLESHEET)).await[..], b":root{--x:1}\n.card__a{color:red}", "not the sections' rules");
        assert_eq!(&body(assets.url("sections/admin.css")).await[..], b".chart__a{}", "listed twice, joined once");
        assert_ne!(
            assets.version(),
            Assets::build_with(Vec::new(), &[tokens, card], &[("/admin", &[table])], "/assets").unwrap().version(),
            "a section's stylesheet is part of the version"
        );

        let error = |components: &[Component], sections: &[(&str, &[Component])]| {
            Assets::build_with(Vec::new(), components, sections, "/assets").err().expect("an error").to_string()
        };
        let twice = error(&[card], &[("/admin", &[card])]);
        assert!(twice.contains("Card is listed in App::components and in the area /admin"), "{twice}");
        let in_app_and_area = error(&[chart], &[("/admin", &[]), ("/shop", &[chart])]);
        assert!(in_app_and_area.contains("Chart is listed in App::components and in the area /shop"), "{in_app_and_area}");
        // tokens! names its stylesheet by the token file's path, as both name and file.
        let tokens = Component { style: Some(crate::Style { file: "tokens.json", ..tokens.style.unwrap() }), ..tokens };
        let global = error(&[], &[("/admin", &[tokens])]);
        assert!(global.contains("tokens.json styles every page"), "{global}");
        let prefix = error(&[card], &[("/admin", &[styled("admin::Card", "card", ".card__b{}")])]);
        assert!(prefix.contains("both style .card__*"), "{prefix}");
        let root = dir("section-clash", &[("sections/admin.css", "a{}")]);
        let file = Assets::build_with(read_dir(&root).unwrap(), &[], &[("/admin", &[chart])], "/assets")
            .err()
            .expect("taken")
            .to_string();
        assert!(file.contains("has a sections/admin.css"), "{file}");
    }

    #[tokio::test]
    async fn a_component_several_sections_list_is_in_a_file_they_share() {
        let page = styled("SearchPage", "search_page", ".search_page__a{}");
        let facet = styled("Facet", "facet", ".facet__a{}");
        let saved = styled("Saved", "saved", ".saved__a{}");
        let sections: &[(&str, &[Component])] =
            &[("/search", &[page, saved, facet]), ("/categories", &[facet, page]), ("/cart", &[])];
        let assets = Arc::new(Assets::build_with(Vec::new(), &[], sections, "/assets").unwrap());
        let files: Vec<_> =
            assets.section_stylesheets().iter().map(|s| (s.prefixes.join(" "), s.href.clone(), s.components.join(" "))).collect();
        assert_eq!(
            files,
            [
                ("/search".to_string(), assets.url("sections/search.css"), "Saved".to_string()),
                ("/search /categories".to_string(), assets.url("sections/search+categories.css"), "SearchPage Facet".to_string()),
            ]
        );
        let res = assets
            .routes::<()>()
            .oneshot(Request::get(assets.url("sections/search+categories.css")).body(Body::empty()).unwrap());
        let body = http_body_util::BodyExt::collect(res.await.unwrap().into_body()).await.unwrap().to_bytes();
        assert_eq!(&body[..], b".search_page__a{}\n.facet__a{}", "each once, in the order listed");
    }

    #[test]
    fn any_change_changes_the_version() {
        let a = Assets::load(dir("v1", &[("a.css", "x")]), "/assets").unwrap();
        let b = Assets::load(dir("v2", &[("a.css", "y")]), "/assets").unwrap();
        let c = Assets::load(dir("v3", &[("b.css", "x")]), "/assets").unwrap();
        assert_ne!(a.version(), b.version());
        assert_ne!(a.version(), c.version());
        assert_eq!(a.version(), Assets::load(dir("v1", &[]), "/assets").unwrap().version(), "same files, same version");
    }
}
