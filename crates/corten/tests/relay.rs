//! A relay behind catch-all routes: `any = ..` serves every method, and one generic handler
//! serves several routes while each still takes its own `Params`.
use axum::{body::Body, extract::Request, http::StatusCode};
use corten::{prelude::*, route::FromUrl, App, Relay};
use http_body_util::BodyExt;
use tower::ServiceExt;

/// Answers with the method and path it was asked for, as an upstream would.
async fn relay<R: FromUrl>(_: Params<R>, req: Request) -> corten::Result<Relay> {
    let said = format!("{} {}", req.method(), req.uri().path());
    Ok(Relay::new(StatusCode::OK, Default::default(), Body::from(said)))
}

async fn home() -> &'static str {
    "home"
}

#[corten::routes]
mod url {
    #[route("/", get = home)]
    pub struct Home;

    #[route("/{*rest}", any = relay)]
    pub struct Elsewhere<'a> {
        pub rest: &'a str,
    }

    #[route("/api/{*rest}", any = relay)]
    pub struct OldApi<'a> {
        pub rest: &'a str,
    }
}

/// An API's old version, relayed whole.
#[corten::routes(api = OldRoot)]
mod old {
    #[route("/v1")]
    pub struct OldRoot;

    #[route("/v1/{*rest}", any = relay)]
    pub struct OldV1<'a> {
        pub rest: &'a str,
    }
}

async fn call(method: &str, path: &str) -> (StatusCode, String) {
    call_app(App::new(url::router()), method, path).await
}

async fn call_app(app: App, method: &str, path: &str) -> (StatusCode, String) {
    let router = app.router();
    let req = axum::http::Request::builder().method(method).uri(path).body(Body::empty()).unwrap();
    let res = router.oneshot(req).await.unwrap();
    let status = res.status();
    (status, String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap())
}

#[tokio::test]
async fn any_method_reaches_the_relay_and_the_apps_own_routes_stay_its_own() {
    for method in ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "PROPFIND"] {
        assert_eq!(call(method, "/inventory/7").await, (StatusCode::OK, format!("{method} /inventory/7")), "{method}");
        assert_eq!(call(method, "/api/stock").await, (StatusCode::OK, format!("{method} /api/stock")), "{method}");
    }
    assert_eq!(call("GET", "/").await, (StatusCode::OK, "home".to_string()));
    assert_eq!(call("POST", "/").await.0, StatusCode::METHOD_NOT_ALLOWED, "a route without any = keeps its methods");
}

#[tokio::test]
async fn head_reaches_the_relay_and_an_api_relays_every_method_too() {
    let (status, body) = call("HEAD", "/inventory/7").await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, ""), "HEAD is relayed, and answered without a body");
    for method in ["GET", "POST", "DELETE", "OPTIONS", "PROPFIND"] {
        let api = App::new(Routes::new()).api(old::api);
        assert_eq!(call_app(api, method, "/v1/stock/7").await, (StatusCode::OK, format!("{method} /v1/stock/7")), "{method}");
    }
}
