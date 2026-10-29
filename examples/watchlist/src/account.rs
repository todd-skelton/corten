//! Who is looking: signing in and out, and the account page with its profile picture.
use std::{
    hash::{BuildHasher, RandomState},
    sync::{Arc, Mutex},
};

use corten::{
    Error, Media,
    axum::{
        body::Bytes,
        extract::FromRef,
        http::{StatusCode, request::Parts},
    },
    prelude::*,
    security::cookie::{self, SetCookie},
};
use serde::Deserialize;

use crate::{
    app,
    layout::{Layout, ViewerBadge},
    recent::Recent,
    theme::*,
};

/// Someone's own pages: every route here, now and later, takes the viewer, so no one signed out
/// reaches one (they are sent to the watchlist, where the sign-in button is).
#[corten::routes(area = Account, caller = Viewer, state = app::Shared)]
pub mod url {
    #[route("/account", get = page)]
    pub struct Account;
}

/// Who is looking, which the layout shows: a name, and the id of their picture if they uploaded one.
#[derive(Clone, Hash)]
pub struct Viewer {
    pub name: String,
    pub picture: Option<String>,
}

/// A real app reads its session here; the example trusts a cookie, which is fine for a demo and
/// nothing else. The picture is another cookie, the id the upload answered with, counted only
/// while the server still has the picture (not after a restart, or once it is forgotten), so a
/// page never shows a picture that is not there.
impl<S: Sync> Identify<S> for Viewer
where
    Pictures: FromRef<S>,
{
    async fn identify(parts: &Parts, state: &S) -> Result<Self, Refused> {
        match cookie::get(&parts.headers, "viewer").filter(|name| !name.is_empty()) {
            Some(name) => {
                let pictures = Pictures::from_ref(state);
                let picture = cookie::get(&parts.headers, "picture").filter(|id| pictures.has(id)).map(str::to_string);
                Ok(Viewer { name: name.into(), picture })
            }
            None => Err(Refused::sign_in(&app::url::Home)),
        }
    }
}

pub(crate) async fn sign_in(req: Req) -> HtmlResult {
    Ok(req.redirect(&app::url::Home)?.with_cookie(SetCookie::new("viewer", "Ada")))
}

pub(crate) async fn sign_out(req: Req) -> HtmlResult {
    Ok(req.redirect(&app::url::Home)?.with_cookie(SetCookie::remove("viewer")).with_cookie(SetCookie::remove("picture")))
}

/// The pictures people uploaded, by id. An id is a 128-bit hash of the bytes with keys of this
/// process's, so its URL names its content (every cache may keep it for good,
/// `Public::immutable`) and is hard to guess; on a collision the first picture stays. They live
/// in memory: at most `KEPT`, and past that the one uploaded longest ago is forgotten, as all are
/// after a restart. Its URL is then a 404, and the pages show the name alone. Anyone can sign in
/// here, so anyone can push the others out; a real app keeps pictures in its store, per account.
#[derive(Clone)]
pub struct Pictures(Arc<Mutex<Kept>>);

struct Kept {
    pictures: Recent<Picture>,
    keys: [RandomState; 2],
}

#[derive(Clone)]
struct Picture {
    media_type: &'static str,
    bytes: Bytes,
}

/// How many pictures are kept: at most 16 MB of them, at 64 KB each.
const KEPT: usize = 256;

/// The largest picture, and the largest form with one.
const PICTURE_MAX: usize = 64 << 10;
const FORM_MAX: usize = 1 << 20;

impl Default for Pictures {
    fn default() -> Self {
        Pictures(Arc::new(Mutex::new(Kept { pictures: Recent::new(KEPT), keys: [RandomState::new(), RandomState::new()] })))
    }
}

impl Pictures {
    /// Keeps a picture and gives its id.
    fn keep(&self, media_type: &'static str, bytes: Bytes) -> String {
        let mut kept = self.0.lock().expect("pictures");
        let [a, b] = &kept.keys;
        let id = format!("{:016x}{:016x}", a.hash_one(&bytes[..]), b.hash_one(&bytes[..]));
        kept.pictures.entry(&id, || Picture { media_type, bytes });
        id
    }

    /// A picture by id. Reading one does not keep it longer: a cache keeps it for a year, so the
    /// server sees few reads of the pictures shown most.
    fn get(&self, id: &str) -> Option<Picture> {
        self.0.lock().expect("pictures").pictures.peek(id).cloned()
    }

    fn has(&self, id: &str) -> bool {
        self.0.lock().expect("pictures").pictures.peek(id).is_some()
    }
}

/// The picture's type, read from its first bytes, never from the type the browser sent with it:
/// a file named `me.png` that holds HTML is not a picture.
fn picture_type(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n', ..] => Some("image/png"),
        [0xff, 0xd8, 0xff, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => Some("image/gif"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("image/webp"),
        _ => None,
    }
}

/// A picture by its id. The id names the bytes, so the answer never changes and every cache may
/// keep it (`Public::immutable`); it reads no cookie, which is why it is not in the `Account`
/// area, and taking a `Caller` here would not compile.
pub(crate) async fn picture(
    State(pictures): State<Pictures>,
    Params(at): Params<app::url::Picture<'_>>,
) -> Public<corten::Result<Media>> {
    Public::immutable(
        pictures.get(&at.id).map(|picture| Media::new(picture.media_type, picture.bytes)).ok_or_else(Error::not_found),
    )
}

#[derive(Deserialize, Fields)]
pub(crate) struct AvatarForm {
    picture: Files,
}

#[derive(Id)]
struct AvatarFormId;

#[derive(Id)]
struct AvatarResultId;

#[derive(Id)]
struct AccountPictureId;

/// What the last upload did: empty on the page, then each answer's text, announced.
#[component]
fn AvatarResult(text: &str) -> impl Patch {
    html! { <p id={AvatarResultId} class={muted()} aria-live="polite">{text}</p> }
}

/// The viewer's picture on the account page, large, or a note that there is none yet.
#[component]
fn AccountPicture<'a>(viewer: &'a Viewer) -> impl Patch {
    html! {
        <div id={AccountPictureId}>
            match &viewer.picture {
                Some(id) => {
                    <img
                        src={app::url::Picture { id }}
                        alt={&viewer.name}
                        width="96"
                        height="96"
                        class={(rounded_pill(), border_line())}
                    >
                }
                None => <p class={muted()}>"No picture yet."</p>,
            }
        </div>
    }
}

/// Why an upload was not kept.
enum Refusal {
    NoPicture,
    NotAPicture,
    TooLarge,
}

impl Refusal {
    fn text(&self) -> &'static str {
        match self {
            Refusal::NoPicture => "Choose a picture to upload.",
            Refusal::NotAPicture => "That file is not a picture: choose a PNG, JPEG, GIF or WebP.",
            Refusal::TooLarge => "The picture is over 64 KB. Choose a smaller one.",
        }
    }
}

/// A form with a file, read as it arrives: the first picture in it, up to 64 KB, its type read
/// from its bytes. A body over 1 MiB is answered with the form's message, not the error page.
/// The picture is kept, its id goes in a cookie, and the answer replaces the header's badge and
/// the page's picture with JavaScript, or is the account page without.
pub(crate) async fn avatar(
    req: Req,
    Caller(viewer): Caller<Viewer>,
    State(pictures): State<Pictures>,
    mut upload: Upload<AvatarForm>,
) -> HtmlResult {
    let f = AvatarForm::FIELDS;
    upload.limit(FORM_MAX);
    // Every read can find the body over its limit, so they are one block and its outcome is
    // matched once.
    let read = async {
        let mut picture = Err(Refusal::NoPicture);
        while let Some(mut file) = upload.next_file().await? {
            if file.is(f.picture) && matches!(picture, Err(Refusal::NoPicture)) {
                let name = file.filename().to_string();
                picture = match file.bytes_up_to(PICTURE_MAX).await? {
                    None => Err(Refusal::TooLarge),
                    Some(bytes) if bytes.is_empty() => Err(Refusal::NoPicture),
                    Some(bytes) => picture_type(&bytes).map(|kind| (name, kind, bytes)).ok_or(Refusal::NotAPicture),
                };
            }
        }
        upload.fields().await?;
        Ok::<_, UploadError>(picture)
    };
    // A refusal is the status line with JavaScript, the account page with it without.
    let refuse = |status, text| match req.mode {
        Mode::Action => req.patch_with_status(status, AvatarResult { text }),
        _ => account_page(&req, &viewer, text, status),
    };
    let (name, kind, bytes) = match read.await {
        Ok(Ok(picture)) => picture,
        Ok(Err(refused)) => return refuse(StatusCode::UNPROCESSABLE_ENTITY, refused.text()),
        Err(UploadError::TooLarge { .. }) => return refuse(StatusCode::PAYLOAD_TOO_LARGE, "The form is over 1 MB."),
        Err(error) => return Err(error.into()),
    };
    let id = pictures.keep(kind, bytes);
    let viewer = &Viewer { picture: Some(id.clone()), ..viewer.clone() };
    let saved = &format!("Saved {name} as your picture.");
    let res = req
        .patch_or(
            html! {
                <ViewerBadge viewer={viewer} />
                <AccountPicture viewer={viewer} />
                <AvatarResult text={saved} />
            },
            || async { account_page(&req, viewer, saved, StatusCode::OK) },
        )
        .await?;
    // A year: the id only changes with a new upload.
    Ok(res.with_cookie(SetCookie::new("picture", &id).max_age(365 * 24 * 60 * 60)))
}

/// Only someone signed in gets here: the handler takes the viewer.
async fn page(req: Req, Caller(viewer): Caller<Viewer>) -> HtmlResult {
    account_page(&req, &viewer, "", StatusCode::OK)
}

fn account_page(req: &Req, viewer: &Viewer, result: &str, status: StatusCode) -> HtmlResult {
    let f = AvatarForm::FIELDS;
    req.page_with_status(
        status,
        html! {
            <Layout req={req} title="Account" viewer={Some(viewer)} head="">
                <div class={(stack(Space::_4), max_w_prose())}>
                    <h1>"Account"</h1>
                    <section class={(card(), row(Space::_3), justify_between(), flex_wrap())}>
                        <p>"Signed in as "{&viewer.name}"."</p>
                        <form method="post" action={app::url::SignOut} data-patch>
                            <button class={secondary()}>"Sign out"</button>
                        </form>
                    </section>
                    <form
                        id={AvatarFormId}
                        method="post"
                        action={app::url::Avatar}
                        enctype="multipart/form-data"
                        data-patch
                        class={(card(), stack(Space::_3))}
                    >
                        <h2>"Profile picture"</h2>
                        <AccountPicture viewer={viewer} />
                        <label class={stack(Space::_1)}>
                            <span class={muted()}>"A PNG, JPEG, GIF or WebP, up to 64 KB"</span>
                            <input type="file" name={f.picture} accept="image/png,image/jpeg,image/gif,image/webp" required>
                        </label>
                        <div><button>"Upload"</button></div>
                        <AvatarResult text={result} />
                    </form>
                </div>
            </Layout>
        },
    )
}

#[cfg(test)]
mod tests {
    use corten::axum::{body::Body, http::Request};
    use http_body_util::BodyExt;

    use super::picture_type;
    use crate::testing::{call, call_on, get, get_with, post, router, send, text};

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    /// `POST /avatar` as the runtime sends it, by Ada, with one file.
    fn upload(name: &str, bytes: &[u8]) -> Request<Body> {
        let mut req = upload_without_js(name, bytes);
        req.headers_mut().insert("corten", "action".parse().unwrap());
        req
    }

    /// The same post as a browser sends it without JavaScript.
    fn upload_without_js(name: &str, bytes: &[u8]) -> Request<Body> {
        let mut body = format!(
            "--b\r\ncontent-disposition: form-data; name=\"picture\"; filename=\"{name}\"\r\ncontent-type: image/png\r\n\r\n"
        )
        .into_bytes();
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n--b--\r\n");
        Request::post("/avatar")
            .header("content-type", "multipart/form-data; boundary=b")
            .header("cookie", "viewer=Ada")
            .body(Body::from(body))
            .unwrap()
    }

    #[tokio::test]
    async fn the_account_page_is_for_someone_signed_in() {
        let res = call(get("/account")).await;
        assert_eq!((res.status().as_u16(), &res.headers()["location"]), (303, &"/".parse().unwrap()));
        let (_, ada) = send(get_with("/account", "viewer=Ada")).await;
        assert!(ada.contains("<p>Signed in as Ada.</p>") && ada.contains("No picture yet."), "{ada}");
        assert!(ada.contains(r#"enctype="multipart/form-data""#) && ada.contains(r#"id="avatar-result""#), "{ada}");
        let res = call(post("/sign-out", None, None, "")).await;
        let cookies: Vec<_> = res.headers().get_all("set-cookie").iter().map(|c| c.to_str().unwrap().to_string()).collect();
        assert!(cookies[0].starts_with("viewer=; Path=/; SameSite=Lax; Max-Age=0"), "{cookies:?}");
        assert!(cookies[1].starts_with("picture=; Path=/; SameSite=Lax; Max-Age=0"), "{cookies:?}");
    }

    #[tokio::test]
    async fn an_uploaded_picture_is_kept_shown_and_served_for_good() {
        let router = router();
        let res = call_on(&router, upload("me.png", PNG)).await;
        assert_eq!(res.status().as_u16(), 200);
        let cookie = res.headers()["set-cookie"].to_str().unwrap().to_string();
        let id = cookie.strip_prefix("picture=").and_then(|c| c.split(';').next()).unwrap().to_string();
        let patch = text(res).await;
        let img = format!(r#"src="/pictures/{id}""#);
        assert!(patch.contains("Saved me.png as your picture.") && patch.matches(&img).count() == 2, "badge and page: {patch}");

        let res = call_on(&router, get(&format!("/pictures/{id}"))).await;
        assert_eq!(
            (res.headers()["content-type"].to_str().unwrap(), res.headers()["cache-control"].to_str().unwrap()),
            ("image/png", "public, max-age=31536000, immutable")
        );
        assert_eq!(res.into_body().collect().await.unwrap().to_bytes(), PNG);
        let res = call_on(&router, get_with("/", &format!("viewer=Ada; picture={id}"))).await;
        assert!(text(res).await.contains(&img), "the header shows it on every page");
        // The same bytes are the same picture.
        let again = call_on(&router, upload("again.png", PNG)).await;
        assert!(again.headers()["set-cookie"].to_str().unwrap().starts_with(&format!("picture={id};")));
        let missing = call_on(&router, get("/pictures/0123456789abcdef0123456789abcdef")).await;
        assert_eq!((missing.status().as_u16(), missing.headers()["cache-control"].to_str().unwrap()), (404, "private, no-cache"));

        // A picture the server no longer has (a restart) is not shown.
        let gone =
            text(call_on(&router, get_with("/account", "viewer=Ada; picture=0123456789abcdef0123456789abcdef")).await).await;
        assert!(gone.contains("No picture yet.") && !gone.contains("/pictures/"), "{gone}");
        // Without JavaScript the answer is the account page, the picture in it.
        let page = text(call_on(&router, upload_without_js("me.png", PNG)).await).await;
        assert!(
            page.contains("<h1>Account</h1>") && page.contains("Saved me.png as your picture.") && page.contains(&img),
            "{page}"
        );
        let refused = call_on(&router, upload_without_js("me.png", b"<html>")).await;
        assert_eq!(refused.status().as_u16(), 422);
        let refused = text(refused).await;
        assert!(
            refused.contains("<h1>Account</h1>") && refused.contains("That file is not a picture"),
            "the page, its message in it"
        );
    }

    #[tokio::test]
    async fn only_a_picture_is_kept() {
        let router = router();
        for (bytes, message) in [
            (&b"<script>alert(1)</script>"[..], "That file is not a picture"),
            (&[0x89; 70 << 10][..], "The picture is over 64 KB."),
            (&b""[..], "Choose a picture to upload."),
        ] {
            let res = call_on(&router, upload("me.png", bytes)).await;
            assert!(res.status().as_u16() == 422 && res.headers().get("set-cookie").is_none());
            assert!(text(res).await.contains(message), "{message}");
        }
        let signed_out =
            Request::post("/avatar").header("content-type", "multipart/form-data; boundary=b").body(Body::empty()).unwrap();
        assert_eq!(call_on(&router, signed_out).await.status().as_u16(), 303, "only someone signed in uploads");
    }

    #[test]
    fn a_picture_is_known_by_its_first_bytes() {
        assert_eq!(picture_type(PNG), Some("image/png"));
        assert_eq!(picture_type(b"\xff\xd8\xff\xe0"), Some("image/jpeg"));
        assert_eq!(picture_type(b"GIF89a"), Some("image/gif"));
        assert_eq!(picture_type(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(picture_type(b"<svg xmlns"), None);
    }
}
