//! Market news: stories from other publishers about the companies on the market, newest first,
//! filtered by sector and by the stocks the reader watches, a page at a time.
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::{Duration, UNIX_EPOCH},
};

use corten::{axum::http::Uri, i18n::DateStyle, prelude::*, route::Elsewhere, view::Append};
use serde::Deserialize;

use crate::{
    account::Viewer,
    app::{Msg, Shared},
    layout::Layout,
    lists::Watching,
    theme::*,
};

/// The news pages: an area, so the stylesheets of the components only they show (`NewsFilter`,
/// `Disclaimer`) load with them alone.
#[corten::routes(area = News, state = Shared)]
pub mod url {
    /// The stories, filtered: a link writes the filter's parameters, which the page reads back
    /// with the same struct (`NewsQuery`).
    #[route("/news", get = page)]
    #[derive(Default)]
    pub struct News {
        #[route(query)]
        pub filter: super::NewsQuery,
    }

    /// A story's link: the app sends the browser on to the publisher.
    #[route("/news/{id}/read", get = read)]
    pub struct Read<'a> {
        pub id: &'a str,
    }

    #[route("/news/briefing", post = subscribe)]
    pub struct Briefing;
}

/// A part of the market. In an address it is the word serde reads (`UrlValue`).
#[derive(Clone, Copy, PartialEq, Debug, UrlValue, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Sector {
    Technology,
    Energy,
    Health,
    Industrials,
    Consumer,
}

impl Sector {
    const ALL: [Sector; 5] = [Sector::Technology, Sector::Energy, Sector::Health, Sector::Industrials, Sector::Consumer];

    fn label(self) -> &'static str {
        match self {
            Sector::Technology => "Technology",
            Sector::Energy => "Energy",
            Sector::Health => "Health",
            Sector::Industrials => "Industrials",
            Sector::Consumer => "Consumer",
        }
    }
}

/// A story another site published, as the app lists it.
pub struct Story {
    id: &'static str,
    headline: &'static str,
    summary: &'static str,
    source: &'static str,
    ticker: &'static str,
    sector: Sector,
    /// When it was published, in seconds since 1970.
    published: u64,
    /// The publisher's page, and the id of the part of it the story is about.
    url: &'static str,
    place: &'static str,
}

/// The stories, newest first. A real app reads them from its news provider.
const STORIES: [Story; 9] = [
    Story {
        id: "acme-outlook",
        headline: "Acme Corp raises its full-year outlook",
        summary: "Orders for industrial anvils ran 12% ahead of plan in the third quarter.",
        source: "Market Wire",
        ticker: "ACME",
        sector: Sector::Industrials,
        published: 1_793_107_800,
        url: "https://news.example.com/markets/acme-outlook",
        place: "guidance",
    },
    Story {
        id: "globex-pipeline",
        headline: "Globex wins approval for its northern pipeline",
        summary: "Construction starts in the spring; the company expects the first flows in 2028.",
        source: "The Ledger",
        ticker: "GLBX",
        sector: Sector::Energy,
        published: 1_793_100_600,
        url: "https://ledger.example.com/energy/globex-pipeline",
        place: "approval",
    },
    Story {
        id: "initech-subscriptions",
        headline: "Initech moves its reporting software to subscriptions",
        summary: "Licenses sold before October stay valid until they expire.",
        source: "Market Wire",
        ticker: "INIT",
        sector: Sector::Technology,
        published: 1_793_048_700,
        url: "https://news.example.com/tech/initech-subscriptions",
        place: "pricing",
    },
    Story {
        id: "halcyon-trial",
        headline: "Halcyon Health's flu vaccine enters its final trial",
        summary: "Twelve thousand volunteers in four countries; results are due next autumn.",
        source: "Health Desk",
        ticker: "HLTH",
        sector: Sector::Health,
        published: 1_793_025_600,
        url: "https://health.example.com/trials/halcyon-flu",
        place: "phase-3",
    },
    Story {
        id: "pantry-recall",
        headline: "Pantry & Co recalls one batch of cereal over a labeling error",
        summary: "The boxes list the wrong allergens; no illnesses have been reported.",
        source: "The Ledger",
        ticker: "PNTR",
        sector: Sector::Consumer,
        published: 1_792_944_000,
        url: "https://ledger.example.com/consumer/pantry-recall",
        place: "batches",
    },
    Story {
        id: "vertigo-warehouses",
        headline: "Vertigo Robotics signs a warehouse automation deal",
        summary: "Four hundred picking robots over three years, the company's largest order yet.",
        source: "Market Wire",
        ticker: "VTGO",
        sector: Sector::Technology,
        published: 1_792_844_100,
        url: "https://news.example.com/tech/vertigo-warehouses",
        place: "the-deal",
    },
    Story {
        id: "northgate-buyback",
        headline: "Northgate Rail announces a $2 billion buyback",
        summary: "The board also raised the quarterly dividend by four cents a share.",
        source: "The Ledger",
        ticker: "NRTH",
        sector: Sector::Industrials,
        published: 1_792_788_300,
        url: "https://ledger.example.com/markets/northgate-buyback",
        place: "buyback",
    },
    Story {
        id: "sunreach-prices",
        headline: "Sunreach Solar cuts panel prices for the second time this year",
        summary: "Cheaper silicon lets the company undercut imports by about 8%.",
        source: "Energy Brief",
        ticker: "SOLR",
        sector: Sector::Energy,
        published: 1_792_682_400,
        url: "https://energy.example.com/solar/sunreach-prices",
        place: "prices",
    },
    Story {
        id: "globex-dividend",
        headline: "Globex holds its dividend despite lower gas prices",
        summary: "Cash from the refining unit covered the shortfall, the finance chief said.",
        source: "Energy Brief",
        ticker: "GLBX",
        sector: Sector::Energy,
        published: 1_792_580_400,
        url: "https://energy.example.com/gas/globex-dividend",
        place: "dividend",
    },
];

/// The components only the news pages show, for the area.
pub const COMPONENTS: &[corten::Component] = &[Disclaimer, NewsFilter];

/// Stories a page shows at a time.
const PAGE: usize = 3;

/// The readers of the morning briefing, by email. A real app keeps them in its database and
/// sends the briefing from a task; this one keeps at most `READERS` in memory and sends nothing.
#[derive(Clone, Default)]
pub struct Readers(Arc<Mutex<BTreeSet<String>>>);

/// Readers kept at most, so posts from anyone cannot grow the set without bound.
const READERS: usize = 10_000;

/// The news page's parameters, one struct for the links that write them and the page that reads
/// them: the sectors, the watchlist filter, how many stories the page has (Load more), and
/// whether a signup sent the reader back. A field renamed here is renamed in every link.
#[derive(Clone, Default, Deserialize, Route, Fields)]
#[route(query)]
pub struct NewsQuery {
    #[serde(default)]
    pub sector: Vec<Sector>,
    pub mine: Option<bool>,
    pub shown: Option<usize>,
    pub subscribed: Option<bool>,
}

impl NewsQuery {
    /// The stories of one sector.
    fn sector(sector: Sector) -> Self {
        NewsQuery { sector: vec![sector], ..NewsQuery::default() }
    }
}

async fn page(req: Req, viewer: Option<Caller<Viewer>>, watching: Watching, Query(q): Query<NewsQuery>) -> HtmlResult {
    news_page(&req, viewer.as_deref(), &watching, &q, &BriefingForm::default(), &Errors::new()).await
}

/// The news page, or Load more's next stories as a patch.
async fn news_page(
    req: &Req,
    viewer: Option<&Viewer>,
    watching: &Watching,
    q: &NewsQuery,
    briefing: &BriefingForm,
    errors: &Errors,
) -> HtmlResult {
    let mine = q.mine.unwrap_or(false);
    let stories: Vec<&Story> = STORIES
        .iter()
        .filter(|s| (q.sector.is_empty() || q.sector.contains(&s.sector)) && (!mine || watching.has(s.ticker)))
        .collect();
    let (stories, total) = (&stories[..], stories.len());
    let shown = q.shown.unwrap_or(0).min(total);
    let next = (shown + PAGE).min(total);
    let t = req.t();
    // Load more with JavaScript: only the next stories, after the ones the page has, and the
    // button for the ones after them.
    let more = html! {
        {Append(StoriesId, html! { for story in &stories[shown..next] { <StoryItem story={story} t={t} /> } })}
        <LoadMore sectors={&q.sector} mine={mine} shown={next} total={total} />
    };
    req.patch_or(more, || async { whole_page(req, viewer, q, briefing, errors, stories, next) }).await
}

/// The whole news page, with every story shown so far: a first load, a filter, no JavaScript.
fn whole_page(
    req: &Req,
    viewer: Option<&Viewer>,
    q: &NewsQuery,
    briefing: &BriefingForm,
    errors: &Errors,
    stories: &[&Story],
    next: usize,
) -> HtmlResult {
    let (mine, total, t) = (q.mine.unwrap_or(false), stories.len(), req.t());
    let title = if mine { "Stories about your stocks" } else { "Latest stories" };
    req.page(html! {
        <Layout req={req} title="News" viewer={viewer} head="">
            <div class={(stack(Space::_1), mb_5())}>
                <h1>"Market news"</h1>
                <p class={muted()}>"What the companies on the market are doing, newest first."</p>
            </div>
            <div
                class={(display_grid(), columns(Responsive::new(Columns::_1).md(Columns::Side)), gap_5(), items_start())}
            >
                <aside class={card()}>
                    <NewsFilter sectors={&q.sector} mine={mine} />
                </aside>
                <section class={stack(Space::_4)} aria-labelledby={StoriesTitleId}>
                    <h2 id={StoriesTitleId}>{title}</h2>
                    if stories.is_empty() {
                        <p class={muted()}>"No stories match the filter."</p>
                    }
                    <ol id={StoriesId} class={stack(Space::_3)}>
                        for story in &stories[..next] { <StoryItem story={story} t={t} /> }
                    </ol>
                    <LoadMore sectors={&q.sector} mine={mine} shown={next} total={total} />
                    <Disclaimer />
                    <Subscribe form={briefing} errors={errors} subscribed={q.subscribed.unwrap_or(false)} />
                </section>
            </div>
        </Layout>
    })
}

/// The list of stories: Load more adds to it.
#[derive(Id)]
struct StoriesId;

/// The list's heading, which names it.
#[derive(Id)]
struct StoriesTitleId;

#[derive(Id)]
struct LoadMoreId;

/// One story: its headline links through the app (`url::Read`), and its sector to the stories
/// of that sector.
#[component]
fn StoryItem<'a>(story: &'a Story, t: Msg) -> impl Render {
    let published = UNIX_EPOCH + Duration::from_secs(story.published);
    html! {
        <li class={(card(), stack(Space::_2))}>
            <h3><a href={url::Read { id: story.id }}>{story.headline}</a></h3>
            <p>{story.summary}</p>
            <p class={(row(Space::_3), flex_wrap(), muted())}>
                <span>{story.source}</span>
                <span>{t.date(published, DateStyle::Medium)}</span>
                <span class={weight_semibold()}>{story.ticker}</span>
                <a href={url::News { filter: NewsQuery::sector(story.sector) }}>{story.sector.label()}</a>
            </p>
        </li>
    }
}

/// Loads the next stories: a GET form, so without JavaScript it is the page with more stories,
/// and with it (`data-patch`) the answer adds only the new ones (`view::Append`). It carries
/// the filter, so the next stories are the filter's.
#[component]
fn LoadMore<'a>(sectors: &'a [Sector], mine: bool, shown: usize, total: usize) -> impl Patch {
    let f = NewsQuery::FIELDS;
    html! {
        <form id={LoadMoreId} method="get" action={url::News::default()} data-patch>
            for sector in sectors {
                <input type="hidden" name={f.sector} value={sector}>
            }
            if mine {
                <input type="hidden" name={f.mine} value="on">
            }
            <input type="hidden" name={f.shown} value={shown}>
            <button class={(secondary(), w_full())} hidden={shown >= total}>"Load more stories"</button>
        </form>
    }
}

/// The filter, applied as soon as it changes: its handler submits the form, and
/// `data-keep-place` keeps the reader where they were while the page changes under them: the
/// rail keeps its scroll, the groups they opened stay open (the server draws a group open only
/// when one of its boxes is checked), and focus stays on the box they changed. Without
/// JavaScript, Apply submits it.
#[component(css, js)]
fn NewsFilter<'a>(sectors: &'a [Sector], mine: bool) -> impl Render {
    let f = NewsQuery::FIELDS;
    html! {
        <form method="get" action={url::News::default()} data-keep-place on:change={js::apply(target)} class={stack(Space::_3)}>
            <h2>"Filter"</h2>
            <div class={Rail}>
                <details open={!sectors.is_empty()}>
                    <summary>"Sectors"</summary>
                    for sector in Sector::ALL {
                        <label>
                            <input type="checkbox" name={f.sector} value={sector} checked={sectors.contains(&sector)}>
                            {sector.label()}
                        </label>
                    }
                </details>
                <details open={mine}>
                    <summary>"Watchlist"</summary>
                    <label><input type="checkbox" name={f.mine} checked={mine}>"Only stocks I watch"</label>
                </details>
            </div>
            <button class={secondary()}>"Apply"</button>
        </form>
    }
}

/// A note only the news pages show. Its stylesheet, `disclaimer.css`, is in the `/news` area,
/// so the watchlist never downloads it; a client navigation here loads it before the page is
/// swapped in.
#[component(css)]
fn Disclaimer() -> impl Render {
    html! {
        <p class={Note}>
            "Stories are short summaries of other publishers' reporting; follow a headline for the whole story. Nothing here is investment advice."
        </p>
    }
}

/// The briefing's form, at the end of a long page.
#[derive(Id)]
struct BriefingId;

#[derive(Deserialize, Fields, Default)]
struct BriefingForm {
    email: String,
}

/// The signup: what the reader typed and the server's messages after a refused submit, and a
/// note once they are on the list.
#[component]
fn Subscribe<'a>(form: &'a BriefingForm, errors: &'a Errors, subscribed: bool) -> impl Patch {
    let f = BriefingForm::FIELDS;
    let e = errors.within(BriefingId);
    html! {
        <form id={BriefingId} method="post" action={url::Briefing} data-patch class={(card(), stack(Space::_3))}>
            <h2>"The morning briefing"</h2>
            {errors.form_message()}
            if subscribed {
                <p role="status">"You're on the list for the morning briefing."</p>
            } else {
                <p class={muted()}>"The day's top stories by email, before the market opens."</p>
            }
            <label class={stack(Space::_1)}>
                <span class={muted()}>"Email"</span>
                <input type="email" name={f.email} value={&form.email} required autocomplete="email" {..e.aria(f.email)}>
            </label>
            {e.message(f.email)}
            <button>"Subscribe"</button>
        </form>
    }
}

/// Signs a reader up, then sends the browser back to the form it came from, at the end of the
/// page: `News { .. }.at(BriefingId)` is `/news?subscribed=true#briefing`, the id the form's
/// markup prints, and the page says it worked. An address the server refuses comes back with
/// the message next to the field: the form, as a patch, or the page without JavaScript.
async fn subscribe(
    req: Req,
    viewer: Option<Caller<Viewer>>,
    watching: Watching,
    State(readers): State<Readers>,
    Form(form): Form<BriefingForm>,
) -> HtmlResult {
    let f = BriefingForm::FIELDS;
    let mut errors = Errors::new();
    let email = form.email.trim();
    let (name, domain) = email.split_once('@').unwrap_or_default();
    if name.is_empty() || !domain.contains('.') || email.len() > 254 || email.contains(char::is_whitespace) {
        errors.add(f.email, "Enter an email address, such as ada@example.com.");
    } else {
        let mut readers = readers.0.lock().unwrap_or_else(|e| e.into_inner());
        if readers.len() >= READERS && !readers.contains(email) {
            errors.add_form("The briefing is full for now. Please try again tomorrow.");
        } else {
            readers.insert(email.to_string());
        }
    }
    if !errors.is_empty() {
        let patch = Subscribe { form: &form, errors: &errors, subscribed: false };
        let q = NewsQuery::default();
        let page = || async { news_page(&req, viewer.as_deref(), &watching, &q, &form, &errors).await };
        return req.invalid(&errors, patch, page).await;
    }
    let back = url::News { filter: NewsQuery { subscribed: Some(true), ..NewsQuery::default() } };
    req.redirect(&back.at(BriefingId))
}

/// Where a story is: another site. A client navigation's `fetch` cannot follow a redirect there,
/// so the runtime is told the address and the browser goes there itself, to the part of the
/// page the story is about (`Elsewhere::at`: an id that page's markup owns). Linking through
/// the app keeps a story's address in one place.
async fn read(req: Req, Path(id): Path<String>) -> HtmlResult {
    let story = STORIES.iter().find(|s| s.id == id).ok_or_else(corten::Error::not_found)?;
    req.redirect(&Elsewhere::at(&Uri::from_static(story.url), story.place))
}

#[cfg(test)]
mod tests {
    use corten::axum::{
        body::Body,
        http::{Request, StatusCode},
    };

    use super::*;
    use crate::testing::{VISITOR, call, call_on, get, get_with, post, post_with, router, send, text};

    #[tokio::test]
    async fn only_the_news_pages_link_their_area_stylesheet() {
        let links = |body: &str| body.matches("data-corten-style").count();
        let (_, home) = send(get("/")).await;
        assert_eq!(links(&home), 0, "{home}");
        let (_, news) = send(get("/news")).await;
        assert!(news.contains(r#"/sections/news.css" data-corten-style>"#), "{news}");
        assert!(news.contains(r#"<p class="disclaimer__note">"#), "{news}");
    }

    /// The headlines of the stories a page or patch shows, in order.
    fn headlines(body: &str) -> Vec<&str> {
        body.split(r#"/read">"#).skip(1).map(|s| &s[..s.find('<').unwrap()]).collect()
    }

    #[tokio::test]
    async fn load_more_sends_only_the_next_stories() {
        let more =
            Request::get("/news?sector=energy&sector=technology&shown=3").header("corten", "action").body(Body::empty()).unwrap();
        let (_, patch) = send(more).await;
        assert!(patch.starts_with(r#"<template id="stories" data-append><li class="#), "{patch}");
        // Five stories are about energy or technology: the page had three, these are the last two.
        assert_eq!(
            headlines(&patch),
            [
                "Sunreach Solar cuts panel prices for the second time this year",
                "Globex holds its dividend despite lower gas prices"
            ]
        );
        assert!(
            patch.ends_with(concat!(
                r#"<form id="load-more" method="get" action="/news" data-patch><input type="hidden" name="sector" value="energy">"#,
                r#"<input type="hidden" name="sector" value="technology"><input type="hidden" name="shown" value="5">"#,
                r#"<button class="bg-surface fg-accent w-full" hidden>Load more stories</button></form>"#
            )),
            "the button for what is left, with the filter: {patch}"
        );
        // Without JavaScript the same form loads the page with every story so far.
        let (_, page) = send(get("/news?shown=3")).await;
        assert_eq!(headlines(&page).len(), 6, "{page}");
    }

    #[tokio::test]
    async fn a_sector_link_writes_the_parameters_the_page_reads() {
        let link = url::News { filter: NewsQuery { sector: vec![Sector::Energy, Sector::Health], ..NewsQuery::default() } };
        assert_eq!(link.href(), "/news?sector=energy&sector=health");
        let (_, page) = send(get(&link.href())).await;
        assert_eq!(headlines(&page).len(), 3, "the first page of the four energy and health stories: {page}");
        assert!(page.contains(r#"<input type="checkbox" name="sector" value="energy" checked>"#), "{page}");
        assert!(page.contains(r#"<details open><summary>Sectors</summary>"#), "a group with a box checked is drawn open: {page}");
        assert!(page.contains(r#"<a href="/news?sector=energy">Energy</a>"#), "a story's sector links to its stories: {page}");
        let mine = url::News { filter: NewsQuery { mine: Some(true), shown: Some(3), ..NewsQuery::default() } };
        assert_eq!(mine.href(), "/news?mine=true&shown=3");
        assert_eq!(url::News::default().href(), "/news");
    }

    #[tokio::test]
    async fn the_watchlist_filter_shows_stories_about_the_stocks_watched() {
        let app = router();
        call_on(&app, post_with("/unwatch", Some("action"), None, Some(VISITOR), "remove=ACME")).await;
        // The checkbox sends `on`; a link writes `true`.
        for uri in ["/news?mine=on", "/news?mine=true"] {
            let page = text(call_on(&app, get_with(uri, VISITOR)).await).await;
            assert!(page.contains(r#"<h2 id="stories-title">Stories about your stocks</h2>"#), "{page}");
            assert_eq!(
                headlines(&page),
                [
                    "Globex wins approval for its northern pipeline",
                    "Initech moves its reporting software to subscriptions",
                    "Globex holds its dividend despite lower gas prices"
                ],
                "{uri}"
            );
        }
    }

    #[tokio::test]
    async fn a_story_link_goes_to_its_place_on_the_publishers_page() {
        let res = call(get("/news/acme-outlook/read")).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(res.headers()["location"], "https://news.example.com/markets/acme-outlook#guidance");
        assert_eq!(call(get("/news/nope/read")).await.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn subscribing_goes_back_to_the_form_which_says_so() {
        let res = call(post("/news/briefing", None, None, "email=ada%40example.com")).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(res.headers()["location"], "/news?subscribed=true#briefing");
        let (_, page) = send(get("/news?subscribed=true")).await;
        assert!(page.contains("You&#39;re on the list for the morning briefing."), "{page}");
    }

    #[tokio::test]
    async fn an_address_the_server_refuses_is_answered_next_to_the_field() {
        let res = call(post("/news/briefing", Some("action"), None, "email=nobody")).await;
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let patch = text(res).await;
        assert!(patch.starts_with(r#"<form id="briefing""#), "{patch}");
        assert!(
            patch.contains(
                r#"value="nobody" required autocomplete="email" aria-invalid="true" aria-describedby="briefing-email-error">"#
            ),
            "{patch}"
        );
        assert!(patch.contains("Enter an email address, such as ada@example.com."), "{patch}");
        // Without JavaScript, the page with the form as the reader left it.
        let res = call(post("/news/briefing", None, None, "email=a%40b")).await;
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let page = text(res).await;
        assert!(page.contains("<h1>Market news</h1>") && page.contains(r#"value="a@b""#), "{page}");
    }
}
