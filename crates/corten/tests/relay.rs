//! A relay behind catch-all routes: `any = ..` serves every method, and one generic handler
//! serves several routes while each still takes its own `Params`.
use axum::{body::Body, extract::Request, http::StatusCode};
use corten::{
    prelude::*,
    route::{FromUrl, Local},
    App, Relay,
};
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

/// An area relayed whole: `any` serves every method under its prefix, as at the root.
#[corten::routes(area = Legacy)]
mod legacy {
    #[route("/legacy")]
    pub struct Legacy;

    #[route("/legacy/{*rest}", any = relay)]
    pub struct LegacyAll<'a> {
        pub rest: &'a str,
    }
}

/// Whether the app serves the path in `?p=`, as a sign-in's `next` is checked.
async fn served(req: Req, uri: axum::http::Uri) -> &'static str {
    let p = uri.query().and_then(|q| q.strip_prefix("p=")).unwrap_or("");
    match Local::parse(p) {
        Some(local) if req.serves(&local) => "served",
        _ => "not served",
    }
}

async fn write() -> &'static str {
    "written"
}

#[corten::routes]
mod checked {
    #[route("/check", get = served)]
    pub struct Check;

    #[route("/write", post = write)]
    pub struct Write;

    #[route("/relay/{*rest}", any = relay)]
    pub struct Relayed<'a> {
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

#[tokio::test]
async fn an_area_relays_every_method_under_its_prefix() {
    for method in ["GET", "POST", "PUT", "DELETE", "PROPFIND"] {
        let app = App::new(Routes::new()).area(legacy::area);
        assert_eq!(
            call_app(app, method, "/legacy/orders/7").await,
            (StatusCode::OK, format!("{method} /legacy/orders/7")),
            "{method}"
        );
    }
}

#[tokio::test]
async fn a_relayed_path_is_served_as_a_page_is() {
    for (path, want) in [("/relay/items/7", "served"), ("/check", "served"), ("/write", "not served"), ("/nowhere", "not served")]
    {
        let app = App::new(checked::router());
        assert_eq!(call_app(app, "GET", &format!("/check?p={path}")).await, (StatusCode::OK, want.to_string()), "{path}");
    }
}
