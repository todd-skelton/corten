mod views;

use corten::prelude::*;
use serde::Deserialize;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    corten::serve(app()).await
}

/// Every URL the app serves. Views link to these types, so a link cannot point at a route that
/// does not exist, and changing a path here changes every link to it.
#[derive(Route)]
#[route("/")]
pub struct Home;

#[derive(Route)]
#[route("/greet")]
pub struct Greet;

/// The routes and the shell. Security headers, error pages, static files, health checks,
/// compression and live reload are Corten's defaults; see `App` to change them.
fn app() -> App {
    let routes = Router::new()
        .route(Home::PATH, get(home))
        .route(Greet::PATH, post(greet));
    App::new(routes)
        .shell(views::shell)
        .components(&[views::ClickCounter])
}

async fn home(req: Req) -> corten::Result {
    req.page("{{name}}", views::home(""))
}

#[derive(Deserialize, Fields)]
struct GreetForm {
    name: String,
}

/// With JavaScript the runtime swaps in the greeting; without it the form posts and the whole
/// page comes back. One handler serves both.
async fn greet(req: Req, Form(form): Form<GreetForm>) -> corten::Result {
    let name = form.name.trim();
    match req.mode {
        Mode::Action => req.patch(views::Greeting { name }),
        _ => req.page("{{name}}", views::home(name)),
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
    async fn home_page_renders_in_the_shell() {
        let html = send(Request::get("/").body(Body::empty()).unwrap()).await;
        assert!(html.contains("<title>{{name}}</title>"));
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
}
