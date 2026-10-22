use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes, A},
    hooks::use_params_map,
    path, SsrMode,
};

use crate::model::{sort_listings, ContentLine, ItemView, Listing, Offer};

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[server]
pub async fn load_item(id: String) -> Result<Option<ItemView>, ServerFnError> {
    api::fetch_item(&id).await.map_err(ServerFnError::new)
}

#[server]
pub async fn create_alert(item_id: String, title: String, email: String, target_price: String) -> Result<String, ServerFnError> {
    let _ = item_id;
    Ok(crate::model::alert_message(&title, &email, &target_price))
}

#[cfg(feature = "ssr")]
pub fn warm_fixture() {
    let _ = api::fixture();
}

/// `GET /api/items/{id}`: the item as JSON for other services, the same data the page renders.
/// A fixture item is serialized where it is.
#[cfg(feature = "ssr")]
pub async fn item_json(axum::extract::Path(id): axum::extract::Path<String>) -> axum::response::Response {
    use axum::{http::StatusCode, response::IntoResponse, Json};
    if let Some(items) = api::fixture() {
        return match items.get(&id) {
            Some(item) => Json(item).into_response(),
            None => StatusCode::NOT_FOUND.into_response(),
        };
    }
    match api::fetch_item(&id).await {
        Ok(Some(item)) => Json(item).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (StatusCode::BAD_GATEWAY, error).into_response(),
    }
}

#[cfg(feature = "ssr")]
mod api {
    use crate::model::ItemView;
    use std::{collections::HashMap, sync::OnceLock};

    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    static FIXTURE: OnceLock<Option<HashMap<String, ItemView>>> = OnceLock::new();

    /// ITEM_SOURCE=fixture: API responses from FIXTURE_PATH, converted once, so the benchmark
    /// measures the web server alone.
    pub fn fixture() -> Option<&'static HashMap<String, ItemView>> {
        FIXTURE
            .get_or_init(|| {
                if std::env::var("ITEM_SOURCE").as_deref() != Ok("fixture") {
                    return None;
                }
                let path = std::env::var("FIXTURE_PATH").unwrap_or_else(|_| "fixtures/items.json".into());
                let text = std::fs::read_to_string(path).expect("fixture file");
                // One item at a time, so the whole file's parsed JSON never sits in memory at once.
                let raw: HashMap<String, &serde_json::value::RawValue> = serde_json::from_str(&text).expect("fixture JSON");
                let items = raw
                    .into_iter()
                    .map(|(id, v)| (id, ItemView::from_api(&serde_json::from_str(v.get()).expect("fixture item"))))
                    .collect();
                Some(items)
            })
            .as_ref()
    }

    pub async fn fetch_item(id: &str) -> Result<Option<ItemView>, String> {
        if let Some(items) = fixture() {
            return Ok(items.get(id).cloned());
        }
        let base = std::env::var("API_BASE").unwrap_or_else(|_| "http://localhost:8080".into());
        let client = CLIENT.get_or_init(reqwest::Client::new);
        let res = client
            .get(format!("{base}/api/discovery/items/{id}"))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if res.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !res.status().is_success() {
            return Err(format!("item API returned {}", res.status()));
        }
        let value: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
        Ok(Some(ItemView::from_api(&value)))
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    view! {
        <Stylesheet id="leptos" href="/pkg/item_page.css"/>
        <Router>
            <header class="site">
                <A href="/">"Card Market"</A>
            </header>
            <main>
                <Routes fallback=|| view! { <p>"Not found"</p> }>
                    <Route path=path!("/") view=Home/>
                    <Route path=path!("/items/:id") view=ItemPage ssr=SsrMode::Async/>
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn Home() -> impl IntoView {
    view! {
        <Title text="Card Market"/>
        <h1>"Popular items"</h1>
        <ul class="home-list">
            {(1..=50)
                .map(|n| view! { <li><A href=format!("/items/item_{n}")>{format!("Card Name {n}")}</A></li> })
                .collect_view()}
        </ul>
    }
}

#[component]
fn ItemPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.read().get("id").unwrap_or_default();
    let item = Resource::new(id, load_item);
    view! {
        <Suspense fallback=|| view! { <p class="loading">"Loading..."</p> }>
            {move || Suspend::new(async move {
                match item.await {
                    Ok(Some(item)) => view! { <ItemDetail item/> }.into_any(),
                    Ok(None) => {
                        #[cfg(feature = "ssr")]
                        if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
                            response.set_status(axum::http::StatusCode::NOT_FOUND);
                        }
                        view! { <Title text="Not found"/><p>"Not found"</p> }.into_any()
                    }
                    Err(error) => view! { <p class="error">{error.to_string()}</p> }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn ItemDetail(item: ItemView) -> impl IntoView {
    let descending = RwSignal::new(false);
    let show_all_offers = RwSignal::new(false);
    let alert = ServerAction::<CreateAlert>::new();
    let listings = item.listings.clone();
    let offers = item.offers.clone();
    let offer_count = offers.len();
    let title = item.title.clone();
    view! {
        <Title text=format!("{} | Card Market", item.title)/>
        <article class="item">
            <nav class="crumbs">
                {item.categories.iter().map(|c| view! { <a href=format!("/categories/{}", c.slug)>{c.name.clone()}</a> }).collect_view()}
            </nav>
            <h1>{item.title.clone()}</h1>
            {item.subtitle.clone().map(|s| view! { <p class="subtitle">{s}</p> })}
            <ul class="badges">{item.badges.iter().map(|b| view! { <li>{b.clone()}</li> }).collect_view()}</ul>
            <div class="gallery">
                {item.images.iter().enumerate().map(|(i, url)| view! {
                    <div class="img" role="img" aria-label=format!("{} image {}", title, i + 1) data-src=url.clone()></div>
                }).collect_view()}
            </div>
            <section class="summary">
                {match item.summary.clone() {
                    Some(s) => view! {
                        <span><strong>{s.lowest.unwrap_or_else(|| "—".into())}</strong>" lowest"</span>
                        <span>{format!("{} listings", s.count)}</span>
                        <span>{format!("{} available", s.quantity)}</span>
                    }.into_any(),
                    None => view! { <span>"No listings yet"</span> }.into_any(),
                }}
            </section>
            <section class="listings">
                <h2>"Listings"</h2>
                <button type="button" class="sort" on:click=move |_| descending.update(|d| *d = !*d)>
                    {move || if descending.get() { "Price: high to low" } else { "Price: low to high" }}
                </button>
                <table>
                    <thead><tr><th>"Seller"</th><th>"Condition"</th><th>"Ships from"</th><th>"Qty"</th><th>"Price"</th></tr></thead>
                    <tbody>
                        <For
                            each=move || sort_listings(&listings, descending.get())
                            key=|l: &Listing| l.id.clone()
                            children=move |l: Listing| {
                                let seller = match &l.rating {
                                    Some(r) => format!("{} · {} ★ ({})", l.seller, r, l.reviews),
                                    None => l.seller.clone(),
                                };
                                view! {
                                    <tr>
                                        <td>{seller}</td>
                                        <td>{l.condition}</td>
                                        <td>{l.ships_from}</td>
                                        <td>{l.quantity}</td>
                                        <td class="price">{l.price}</td>
                                    </tr>
                                }
                            }
                        />
                    </tbody>
                </table>
            </section>
            <section class="offers">
                <h2>"Offers"</h2>
                <table>
                    <thead><tr><th>"Buyer"</th><th>"Condition"</th><th>"Qty"</th><th>"Price"</th></tr></thead>
                    <tbody>
                        <For
                            each=move || {
                                let n = if show_all_offers.get() { offers.len() } else { offers.len().min(5) };
                                offers[..n].to_vec()
                            }
                            key=|o: &Offer| o.id.clone()
                            children=move |o: Offer| view! {
                                <tr>
                                    <td>{o.buyer}</td>
                                    <td>{o.condition}</td>
                                    <td>{o.quantity}</td>
                                    <td class="price">{o.price}</td>
                                </tr>
                            }
                        />
                    </tbody>
                </table>
                {move || (offer_count > 5 && !show_all_offers.get()).then(|| view! {
                    <button type="button" class="more" on:click=move |_| show_all_offers.set(true)>
                        {format!("Show all {offer_count} offers")}
                    </button>
                })}
            </section>
            <section class="alert">
                <h2>"Price alert"</h2>
                <div class="alert-form">
                    <ActionForm action=alert>
                        <input type="hidden" name="item_id" value=item.id.clone()/>
                        <input type="hidden" name="title" value=item.title.clone()/>
                        <label>"Email"<input type="email" name="email" required/></label>
                        <label>"Alert me below"<input type="text" name="target_price" inputmode="decimal" required/></label>
                        <button type="submit">"Set alert"</button>
                    </ActionForm>
                </div>
                <p class="alert-message" aria-live="polite">
                    {move || alert.value().get().map(|r| r.unwrap_or_else(|e| e.to_string())).unwrap_or_default()}
                </p>
            </section>
            <section class="details">
                <h2>"Details"</h2>
                <p>{item.description.clone()}</p>
                <dl>{item.fields.iter().map(|f| view! { <dt>{f.name.clone()}</dt><dd>{f.value.clone()}</dd> }).collect_view()}</dl>
                <ul class="tags">{item.tags.iter().map(|t| view! { <li>{t.clone()}</li> }).collect_view()}</ul>
            </section>
            {(!item.contents.is_empty()).then(|| view! { <ContentList title="Contents" class_name="contents" lines=item.contents.clone()/> })}
            {(!item.included_in.is_empty()).then(|| view! { <ContentList title="Included in" class_name="included" lines=item.included_in.clone()/> })}
        </article>
    }
}

#[component]
fn ContentList(title: &'static str, class_name: &'static str, lines: Vec<ContentLine>) -> impl IntoView {
    view! {
        <section class=class_name>
            <h2>{title}</h2>
            <ul>
                {lines.into_iter().map(|c| view! {
                    <li><A href=format!("/items/{}", c.item_id)>{c.title}</A>{format!(" × {}", c.quantity)}</li>
                }).collect_view()}
            </ul>
        </section>
    }
}
