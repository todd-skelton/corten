mod views;

use corten::prelude::*;
use serde::Deserialize;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    corten::serve(app()).await
}

/// Every URL the app serves, with the handler that serves it. Views link to these types, so a
/// link cannot point at a route that does not exist, changing a path here changes every link to
/// it, and a route without a handler does not compile.
#[corten::routes]
pub mod url {
    #[route("/", get = home)]
    pub struct Home;

    #[route("/greet", post = greet)]
    pub struct Greet;
}

/// The routes, the error page and the components with scripts or styles. Security headers, static
/// files, health checks, compression and live reload are Corten's defaults; see `App` to change
/// them.
fn app() -> App {
    App::new(url::router())
        .error_page(views::error_page, ())
        .components(&[views::ClickCounter])
}

async fn home(req: Req) -> HtmlResult {
    req.page(views::home(&req, ""))
}

#[derive(Deserialize, Fields)]
struct GreetForm {
    name: String,
}

/// With JavaScript the runtime swaps in the greeting; without it the form posts and the whole
/// page comes back. One handler serves both.
async fn greet(req: Req, Form(form): Form<GreetForm>) -> HtmlResult {
    let name = form.name.trim();
    match req.mode {
        Mode::Action => req.patch(views::Greeting { name }),
        _ => req.page(views::home(&req, name)),
    }
}

#[cfg(test)]
mod tests {
    use corten::axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    async fn send(req: Request<Body>) -> String {
        let res = app().router().oneshot(req).await.unwrap();
        String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
    }

    #[tokio::test]
    async fn home_page_renders_in_the_layout() {
        let html = send(Request::get("/").body(Body::empty()).unwrap()).await;
        assert!(html.contains("<title>{{name}}</title>"));
        assert!(html.contains("<header>"));
        assert!(html.contains(r#"<link rel="icon" type="image/svg+xml""#));
        assert!(
            html.contains("click_counter.count(@total)"),
            "the counter's handler: {html}"
        );
    }

    #[tokio::test]
    async fn greeting_is_a_patch_for_the_runtime() {
        let req = Request::post("/greet")
            .header("corten", "action")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("name=Ada"))
            .unwrap();
        assert_eq!(
            send(req).await,
            r#"<p id="greeting" aria-live="polite">Hello, Ada!</p>"#
        );
    }

    #[tokio::test]
    async fn an_unknown_url_gets_the_error_page_in_the_layout() {
        let html = send(Request::get("/nope").body(Body::empty()).unwrap()).await;
        assert!(html.contains("<h1>Not found</h1>") && html.contains("<header>"));
    }
}
