//! Static files with one content version in the URL: `/assets/{version}/style.css`.
//!
//! [`Assets::load`] reads a directory into memory at startup and hashes it. Every URL carries the
//! hash, so browsers, CDNs and the service worker may cache the files forever, and a deploy
//! that changes any file changes every URL. The version is a directory prefix, not a per-file
//! name, so island modules keep importing each other with relative paths.
//!
//! During a rolling deploy a page from the old version can ask a new server for an old URL. It
//! gets the current file with `Cache-Control: no-cache`, so nothing caches the mismatch.

use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    body::Bytes,
    extract::Path as UrlPath,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};

use crate::fnv;

/// Cache header for a URL that can never change.
pub(crate) const IMMUTABLE: HeaderValue = HeaderValue::from_static("public, max-age=31536000, immutable");

struct File {
    body: Bytes,
    content_type: &'static str,
    etag: HeaderValue,
}

/// A directory of static files served under a versioned prefix. Load it once, at startup.
pub struct Assets {
    mount: String,
    version: String,
    prefix: String,
    islands_dir: String,
    files: HashMap<String, File>,
}

impl Assets {
    /// Reads every file under `dir` and serves them at `{mount}/{version}/...` (`mount` is
    /// usually `/assets`). Island modules belong in `dir/islands/`.
    pub fn load(dir: impl AsRef<Path>, mount: &str) -> io::Result<Self> {
        let mount = mount.trim_end_matches('/');
        assert!(mount.starts_with('/'), "the assets mount is an absolute path, like /assets");
        let mut paths = Vec::new();
        collect(dir.as_ref(), dir.as_ref(), &mut paths)?;
        paths.sort();
        let mut files = HashMap::new();
        let mut digest = fnv::OFFSET;
        for (url, path) in paths {
            let body = std::fs::read(&path)?;
            let hash = fnv::hash(&body);
            digest = fnv::extend(digest, url.as_bytes());
            digest = fnv::extend(digest, &hash.to_le_bytes());
            let etag = HeaderValue::try_from(format!("\"{hash:016x}\"")).expect("hex is a valid header");
            files.insert(url.clone(), File { body: body.into(), content_type: content_type(&url), etag });
        }
        let version = format!("{:012x}", digest >> 16);
        let prefix = format!("{mount}/{version}/");
        Ok(Assets { islands_dir: format!("{prefix}islands/"), mount: mount.into(), version, prefix, files })
    }

    /// The URL for `path` (relative to the directory, like `style.css`). A typo is a bug, so a
    /// missing file panics in debug builds.
    pub fn url(&self, path: &str) -> String {
        debug_assert!(self.files.contains_key(path), "no static file {path:?}");
        format!("{}{path}", self.prefix)
    }

    /// `{mount}/{version}/`.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// `{mount}/`, the prefix every version shares.
    pub fn mount_prefix(&self) -> String {
        format!("{}/", self.mount)
    }

    /// Where island modules are served from: `{prefix}islands/`.
    pub fn islands_dir(&self) -> &str {
        &self.islands_dir
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
        if headers.get(header::IF_NONE_MATCH).is_some_and(|tag| tag == file.etag) {
            return (StatusCode::NOT_MODIFIED, [(header::ETAG, file.etag.clone()), (header::CACHE_CONTROL, cache)])
                .into_response();
        }
        (
            [
                (header::CONTENT_TYPE, HeaderValue::from_static(file.content_type)),
                (header::CACHE_CONTROL, cache),
                (header::ETAG, file.etag.clone()),
            ],
            file.body.clone(),
        )
            .into_response()
    }
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        // Dotfiles (.env, .DS_Store, editor swap files) are never public.
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if entry.file_type()?.is_dir() {
            collect(root, &path, out)?;
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
        let root = dir("serve", &[("style.css", "a{}"), ("islands/x-y.js", "export default 1"), (".env", "SECRET=1")]);
        let assets = Arc::new(Assets::load(&root, "/assets").unwrap());
        assert_eq!(assets.islands_dir(), format!("/assets/{}/islands/", assets.version()));
        let app: Router = assets.routes();
        let get = |uri: String| app.clone().oneshot(Request::get(uri).body(Body::empty()).unwrap());
        let res = get(assets.url("style.css")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()["cache-control"], "public, max-age=31536000, immutable");
        assert_eq!(res.headers()["content-type"], "text/css; charset=utf-8");
        let res = get(format!("/assets/{}/islands/x-y.js", assets.version())).await.unwrap();
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
