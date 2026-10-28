//! Who is looking: signing in and out, and the account page with its profile picture.
use corten::{
    axum::http::{StatusCode, request::Parts},
    prelude::*,
    security::cookie::{self, SetCookie},
};
use serde::Deserialize;

use crate::{app::url as app_url, layout::Layout, theme::*};

/// Someone's own pages: every route here, now and later, takes the viewer, so no one signed out
/// reaches one (they are sent to the watchlist, where the sign-in button is).
#[corten::routes(area = Account, caller = Viewer)]
pub mod url {
    #[route("/account", get = page)]
    pub struct Account;
}

/// Who is looking, which the layout shows.
#[derive(Clone, Hash)]
pub struct Viewer {
    pub name: String,
}

/// A real app reads its session here; the example trusts a cookie, which is fine for a demo and
/// nothing else.
impl<S: Sync> Identify<S> for Viewer {
    async fn identify(parts: &Parts, _: &S) -> Result<Self, Refused> {
        match cookie::get(&parts.headers, "viewer").filter(|name| !name.is_empty()) {
            Some(name) => Ok(Viewer { name: name.into() }),
            None => Err(Refused::sign_in(&app_url::Home)),
        }
    }
}

pub async fn sign_in(req: Req) -> HtmlResult {
    Ok(req.redirect(&app_url::Home)?.with_cookie(SetCookie::new("viewer", "Ada")))
}

pub async fn sign_out(req: Req) -> HtmlResult {
    Ok(req.redirect(&app_url::Home)?.with_cookie(SetCookie::remove("viewer")))
}

#[derive(Deserialize, Fields)]
pub(crate) struct AvatarForm {
    name: String,
    picture: Files,
}

#[derive(Id)]
struct AvatarFormId;

#[derive(Id)]
struct AvatarResultId;

/// What the last upload did: empty on the page, then each answer's text, announced.
#[component]
fn AvatarResult(text: &str) -> impl Patch {
    html! { <p id={AvatarResultId} aria-live="polite">{text}</p> }
}

/// A form with a file: what came, read as it arrives (a picture over 64 KB is refused, the form
/// still read). A body over 1 MiB is answered with the form's message, not the error page.
pub async fn avatar(req: Req, mut upload: Upload<AvatarForm>) -> HtmlResult {
    let f = AvatarForm::FIELDS;
    upload.limit(1 << 20);
    // Every read can find the body over its limit, so they are one block and its outcome is
    // matched once.
    let read = async {
        let mut pictures = Vec::new();
        while let Some(mut file) = upload.next_file().await? {
            if file.is(f.picture) {
                let size =
                    file.bytes_up_to(64 << 10).await?.map_or_else(|| "too large".to_string(), |b| format!("{} bytes", b.len()));
                pictures.push(format!("{} ({}, {size})", file.filename(), file.content_type()));
            }
        }
        Ok::<_, UploadError>((upload.fields().await?, pictures))
    };
    match read.await {
        Ok((form, pictures)) => req.patch(AvatarResult { text: &format!("{}: {}", form.name, pictures.join(", ")) }),
        Err(UploadError::TooLarge { .. }) => {
            req.patch_with_status(StatusCode::PAYLOAD_TOO_LARGE, AvatarResult { text: "The pictures are over 1 MB together." })
        }
        Err(error) => Err(error.into()),
    }
}

/// Only someone signed in gets here: the handler takes the viewer.
async fn page(req: Req, Caller(viewer): Caller<Viewer>) -> HtmlResult {
    let f = AvatarForm::FIELDS;
    let viewer = &viewer;
    req.page(html! {
        <Layout req={&req} title="Account" viewer={Some(viewer)} head="">
            <div class={(stack(Space::_4), max_w_prose())}>
                <h1>"Account"</h1>
                <section class={(card(), row(Space::_3), justify_between(), flex_wrap())}>
                    <p>"Signed in as "{&viewer.name}"."</p>
                    <form method="post" action={app_url::SignOut} data-patch>
                        <button class={secondary()}>"Sign out"</button>
                    </form>
                </section>
                <form
                    id={AvatarFormId}
                    method="post"
                    action={app_url::Avatar}
                    enctype="multipart/form-data"
                    data-patch
                    class={(card(), stack(Space::_3))}
                >
                    <h2>"Profile picture"</h2>
                    <label class={stack(Space::_1)}>
                        <span class={muted()}>"Display name"</span>
                        <input name={f.name} value={&viewer.name} required autocomplete="nickname">
                    </label>
                    <label class={stack(Space::_1)}>
                        <span class={muted()}>"Picture (up to 64 KB)"</span>
                        <input type="file" name={f.picture} accept="image/*" required>
                    </label>
                    <button>"Upload"</button>
                    <AvatarResult text="" />
                </form>
            </div>
        </Layout>
    })
}

#[cfg(test)]
mod tests {
    use crate::testing::{call, get, get_with, post, send};

    #[tokio::test]
    async fn the_account_page_is_for_someone_signed_in() {
        let res = call(get("/account")).await;
        assert_eq!((res.status().as_u16(), &res.headers()["location"]), (303, &"/".parse().unwrap()));
        let (_, ada) = send(get_with("/account", "viewer=Ada")).await;
        assert!(ada.contains("<p>Signed in as Ada.</p>"), "{ada}");
        assert!(
            ada.contains(r#"enctype="multipart/form-data""#) && ada.contains(r#"<p id="avatar-result" aria-live="polite"></p>"#),
            "{ada}"
        );
        let res = call(post("/sign-out", None, None, "")).await;
        assert!(res.headers()["set-cookie"].to_str().unwrap().starts_with("viewer=; Path=/; SameSite=Lax; Max-Age=0"));
    }
}
