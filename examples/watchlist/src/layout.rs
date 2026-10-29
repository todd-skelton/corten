//! The document around every page.
use corten::prelude::*;

use crate::{
    account::{self, Viewer},
    app::url,
    news,
    theme::*,
};

corten::assets::embed! {
    /// The files in `static/`, compiled into the program: `Static::STYLE_CSS` is `static/style.css`.
    pub struct Static = "static";
}

/// The document around every page: its title, its own `head` tags, who is looking and the page.
/// Client navigation swaps the title, the tags and the page only; signing in changes the viewer,
/// which makes the runtime load the next page in full.
#[component]
pub fn Layout<'a>(
    req: &'a Req,
    title: &'a str,
    viewer: Option<&'a Viewer>,
    head: impl Render,
    children: impl Render,
) -> impl Document {
    // The page's side padding, narrower on a phone so the header fits on two lines.
    let gutter = Responsive::new(Space::_4).sm(Space::_5);
    html! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <link rel="stylesheet" href={req.asset(Static::STYLE_CSS)}>
                <link rel="icon" type="image/svg+xml" href={req.asset(Static::FAVICON_SVG)}>
                <title>{title}" | Watchlist"</title>
                {head}
                <Corten />
            </head>
            <body class={(bg_bg(), fg_ink())}>
                <header class={(bg_surface(), border_b_line())}>
                    <div
                        class={(row(Space::_3), justify_between(), flex_wrap(), max_w_page(), mx_auto(), px(gutter), py_3())}
                    >
                        <nav class={(row(Space::_1), flex_wrap())}>
                            <span class={(weight_bold(), tracking_tight())}>"Watchlist"</span>
                            <a {..req.nav(&url::Home)}>"Stocks"</a>
                            <a {..req.nav(&news::url::News::default())}>"News"</a>
                            <a {..req.nav(&url::Alerts { saved: None })}>"Alerts"</a>
                            <a {..req.nav(&account::url::Account)}>"Account"</a>
                        </nav>
                        match viewer {
                            Some(viewer) => <ViewerBadge viewer={viewer} />,
                            None => <form method="post" action={url::SignIn} data-patch><button>"Sign in as Ada"</button></form>,
                        }
                    </div>
                </header>
                <main class={(max_w_page(), mx_auto(), px(gutter), py_6())}>{children}</main>
            </body>
        </html>
    }
}

/// The header's badge: who is signed in, with their picture once they upload one. The upload's
/// answer replaces it by its id.
#[derive(Id)]
pub struct ViewerBadgeId;

#[component]
pub fn ViewerBadge<'a>(viewer: &'a Viewer) -> impl Patch {
    html! {
        <span id={ViewerBadgeId} class={(row(Space::_2), muted())}>
            if let Some(id) = &viewer.picture {
                <img src={url::Picture { id }} alt="" width="28" height="28" class={rounded_pill()}>
            }
            "Signed in as "{&viewer.name}
        </span>
    }
}

#[cfg(test)]
mod tests {
    use corten::axum::{body::Body, http::Request};

    use crate::testing::{get, get_with, send};

    #[tokio::test]
    async fn the_shell_marks_the_current_link_and_shows_the_viewer() {
        let (_, guest) = send(get("/news")).await;
        assert!(guest.contains(r#"<a href="/news" data-nav aria-current="page">News</a>"#), "{guest}");
        assert!(guest.contains(r#"<a href="/" data-nav>Stocks</a>"#));
        assert!(guest.contains("Sign in as Ada"));
        let (_, ada) = send(get_with("/", "viewer=Ada")).await;
        assert!(ada.contains("Signed in as Ada") && !ada.contains("Sign in as Ada"));
        let lost =
            Request::get("/nope").header("cookie", "viewer=Ada").header("accept", "text/html").body(Body::empty()).unwrap();
        let (_, lost) = send(lost).await;
        assert!(lost.contains("<h1>Not found</h1>") && lost.contains("Signed in as Ada"), "the error page too: {lost}");
    }
}
