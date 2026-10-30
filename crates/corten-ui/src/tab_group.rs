use corten::prelude::*;

use crate::state;
use corten::Href;

/// One tab of a [`TabGroup`].
pub struct Tab<'a, L: Render> {
    /// What the tab shows: text, or a view with text in it. Every tab of a group has one label
    /// type; labels of different types (a message with arguments beside one without) are each a
    /// reference:
    ///
    /// ```
    /// use corten::prelude::*;
    /// use corten_ui::{Tab, TabGroup};
    ///
    /// let offers = format!("Offers ({})", 2);
    /// let tabs: [Tab<&dyn Render>; 2] = [
    ///     Tab { label: &"Listings", panel: &"3 listings", link: None },
    ///     Tab { label: &offers, panel: &"2 offers", link: None },
    /// ];
    /// let html = TabGroup { id: "book", label: "Market", tabs: &tabs, selected: 0 }.to_html();
    /// assert!(html.contains(">Offers (2)</button>"), "{html}");
    /// ```
    pub label: L,
    /// The panel's content. Each panel is a view of its own type, so it is a reference to one.
    pub panel: &'a dyn Render,
    /// The page with this tab selected (`ItemRoute { id, tab: Some("offers") }`). Without
    /// JavaScript the tab is a link to it; with JavaScript the tab switches in place and the
    /// address bar follows, so a reload or a shared link opens the same tab. `None`: a button.
    pub link: Option<&'a dyn Href>,
}

/// Tabs and their panels: the WAI-ARIA tabs pattern with automatic activation.
///
/// The server renders every panel and hides all but `selected`. The script (`tab_group.js`)
/// adds the roving focus (Tab reaches the selected tab, the arrow keys, Home and End move and
/// select) and switches panels on click. Tabs that are links work without it: each loads its
/// page, where the server selects it. Tabs that are buttons need it.
///
/// Parts: a `div` around `[role=tablist]`; `[role=tab]` (`[aria-selected=true]` for the selected one), an
/// `a` or a `button`; `[role=tabpanel]`, hidden unless selected.
///
/// ```
/// use corten::prelude::*;
/// use corten_ui::{Tab, TabGroup};
///
/// #[derive(Route)]
/// #[route("/items/{id}")]
/// struct Item<'a> { id: &'a str, tab: Option<&'a str> }
///
/// let (listings, offers) = (Item { id: "x", tab: None }, Item { id: "x", tab: Some("offers") });
/// let tabs = [
///     Tab { label: "Listings", panel: &html! { <p>"3 listings"</p> }, link: Some(&listings) },
///     Tab { label: "Offers", panel: &html! { <p>"No offers"</p> }, link: Some(&offers) },
/// ];
/// let html = TabGroup { id: "book", label: "Market", tabs: &tabs, selected: 1 }.to_html();
/// assert!(html.contains(r#"<a data-ref="tab" role="tab" id="book-tab-1" href="/items/x?tab=offers" aria-controls="book-panel-1" aria-selected="true""#));
/// assert!(html.contains(r#"<div data-ref="panel" role="tabpanel" id="book-panel-0" aria-labelledby="book-tab-0" tabindex="0" hidden"#));
/// ```
#[component(js)]
pub fn TabGroup<'a, L: Render>(id: &'a str, label: &'a str, tabs: &'a [Tab<'a, L>], selected: usize) -> impl Render {
    let (tab, panel) = refs();
    html! {
        <div on:connect={js::connect(tab)}>
            <div role="tablist" aria-label={label}>
                for (i, t) in tabs.iter().enumerate() {
                    match t.link {
                        Some(link) => <a
                            ref={tab as HTMLElement}
                            role="tab"
                            id={(id, "-tab-", i)}
                            href={link}
                            aria-controls={(id, "-panel-", i)}
                            aria-selected={state(i == selected)}
                            on:click={js::choose(event, target, tab, panel)}
                            on:keydown={js::step(event, target, tab, panel)}
                        >
                            {t.label}
                        </a>,
                        None => <button
                            ref={tab as HTMLElement}
                            type="button"
                            role="tab"
                            id={(id, "-tab-", i)}
                            aria-controls={(id, "-panel-", i)}
                            aria-selected={state(i == selected)}
                            on:click={js::choose(event, target, tab, panel)}
                            on:keydown={js::step(event, target, tab, panel)}
                        >
                            {t.label}
                        </button>,
                    }
                }
            </div>
            for (i, t) in tabs.iter().enumerate() {
                <div
                    ref={panel}
                    role="tabpanel"
                    id={(id, "-panel-", i)}
                    aria-labelledby={(id, "-tab-", i)}
                    tabindex="0"
                    hidden={i != selected}
                >
                    {t.panel}
                </div>
            }
        </div>
    }
}
