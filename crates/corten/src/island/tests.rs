use crate::{self as corten, view::Render};

/// The script next to this file is `price_board.js`.
#[corten::island]
fn PriceBoard<'a>(title: &'a str, prices: &'a [u32]) -> impl Render {
    corten::html! {
        <h2 ref={Title}>{title}</h2>
        <ul>for p in prices { <li ref={Price} on:click={Pick}>{p}</li> }</ul>
        <input ref={Filter} on:input={Narrow}>
    }
}

#[test]
fn an_island_renders_in_its_tag_with_names_from_its_markup() {
    let html = PriceBoard { title: "Prices", prices: &[3, 4] }.to_html();
    assert_eq!(
        html,
        r#"<price-board><h2 data-ref="title">Prices</h2><ul><li data-ref="price" data-on="click:pick">3</li><li data-ref="price" data-on="click:pick">4</li></ul><input data-ref="filter" data-on="input:narrow"></price-board>"#
    );
    let island = PriceBoard::ISLAND;
    assert_eq!((island.tag, island.file), ("price-board", "price_board.js"));
    assert!(island.script.contains("island(\"price-board\""));
    let refs: Vec<_> = island.refs.iter().map(|r| (r.name, r.tags)).collect();
    assert_eq!(refs, [("title", &["h2"][..]), ("price", &["li"]), ("filter", &["input"])]);
    let actions: Vec<_> = island.actions.iter().map(|a| (a.name, a.events, a.tags)).collect();
    assert_eq!(actions, [("pick", &["click"][..], &["li"][..]), ("narrow", &["input"], &["input"])]);
    assert!(island.props.is_empty());
}

/// `price_tag.js`.
#[cfg(feature = "json")]
#[corten::island]
fn PriceTag<'a>(#[prop] currency: &'a str, #[prop] cents: u32, label: &'a str) -> impl Render {
    corten::html! { <span>{label}</span> }
}

#[cfg(feature = "json")]
#[test]
fn props_are_one_json_object_that_cannot_close_its_script() {
    let html = PriceTag { currency: "USD", cents: 5, label: "Price" }.to_html();
    assert_eq!(
        html,
        r#"<price-tag><script type="application/json" data-props>{"currency":"USD","cents":5}</script><span>Price</span></price-tag>"#
    );
    assert_eq!(PriceTag::ISLAND.props.iter().map(|p| p.name).collect::<Vec<_>>(), ["currency", "cents"]);
    let html = PriceTag { currency: "</script><script>alert(1)", cents: 0, label: "" }.to_html();
    let json = html.split_once("data-props>").unwrap().1.split_once("</script>").unwrap().0;
    assert_eq!(json, r#"{"currency":"\u003c/script>\u003cscript>alert(1)","cents":0}"#);
    let back: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(back["currency"], "</script><script>alert(1)", "JSON.parse gets the original value back");
}

#[tokio::test]
async fn an_app_serves_its_islands_and_preloads_them_where_they_render() {
    use axum::{body::Body, http::Request, routing::get, Router};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let page = |req: crate::Req| async move { req.page("Prices", PriceBoard { title: "Prices", prices: &[1] }) };
    let app = crate::App::new(Router::new().route("/", get(page))).dev(false).islands(&[PriceBoard::ISLAND]).router();
    let text = |res: axum::response::Response| async move {
        String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
    };
    let html = text(app.clone().oneshot(Request::get("/").body(Body::empty()).unwrap()).await.unwrap()).await;
    let start = html.find(r#"<link rel="modulepreload" href=""#).expect("the island is preloaded") + 32;
    let url = &html[start..start + html[start..].find('"').unwrap()];
    assert!(url.starts_with("/assets/") && url.ends_with("/islands/price-board.js"), "{url}");
    let nonce = html.split_once(r#"<script type="importmap" nonce=""#).unwrap().1.split_once('"').unwrap().0;
    assert!(
        html.contains(&format!(r#"<style nonce="{nonce}">price-board{{display:contents}}</style>"#)),
        "layout-neutral, under the CSP"
    );
    let res = app.oneshot(Request::get(url).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.headers()["cache-control"], "public, max-age=31536000, immutable");
    assert_eq!(text(res).await, PriceBoard::ISLAND.script);
}
