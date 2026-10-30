use corten::{prelude::*, view::Id, Href};

use crate::{choice, copy_button::plain_id, state};

/// One option of a [`ChoiceGroup`]: the word it chooses, what it shows, and the page with it
/// chosen.
pub struct ChoiceOption<'a, V, L: Render> {
    /// The word, one of the values [`ShownFor`] parts are shown for: a `#[derive(UrlValue)]`
    /// enum's variant, or any `AsRef<str>`.
    pub value: V,
    /// What the option shows: text, or a view with text in it.
    pub label: L,
    /// The page with this option chosen (`Performance { window: Some(Window::Days90) }`). Without
    /// JavaScript the option is a link to it, where the server draws that choice; with
    /// JavaScript the choice switches in place and the address bar follows, so a reload or a
    /// shared link keeps it. `None`: a button, which needs the script.
    pub link: Option<&'a dyn Href>,
}

/// One choice whose value shows and hides parts anywhere on the page: a performance window that
/// both the figures and, below a chart, the list worded from them follow. [`TabGroup`](crate::TabGroup)
/// switches only the panels inside it; this switches every [`ShownFor`] of its group, wherever
/// the page draws it, also one a patch puts in later.
///
/// The WAI-ARIA radio group pattern: a `div[role=radiogroup]` named by `label`, an option per
/// [`ChoiceOption`] (`[role=radio]`, `[aria-checked=true]` for the chosen one), an `a` or a
/// `button`. The server draws `selected` checked and every part for another value `hidden`, so
/// the page is right before the script loads and without it. The script (`choice.js`) adds the
/// roving focus (Tab reaches the chosen option; the arrow keys, Home and End move and choose;
/// Space chooses) and switches in place on a click.
///
/// The group is named by its `#[derive(Id)]` id, which its parts name too.
///
/// Parts: the `div[role=radiogroup]` and its `[role=radio]` options; each [`ShownFor`]'s `div`.
///
/// ```
/// use corten::prelude::*;
/// use corten_ui::{ChoiceGroup, ChoiceOption, ShownFor};
///
/// #[derive(Clone, Copy, PartialEq, UrlValue, serde::Deserialize)]
/// enum Window { #[serde(rename = "30")] Days30, #[serde(rename = "90")] Days90 }
///
/// #[derive(Route)]
/// #[route("/performance")]
/// struct Performance { window: Option<Window> }
///
/// #[derive(Id)]
/// struct WindowId; // ui-window: ids in this crate's examples start with its prefix
///
/// let window = Window::Days30;
/// let (days30, days90) = (Performance { window: None }, Performance { window: Some(Window::Days90) });
/// let html = html! {
///     <ChoiceGroup
///         id={WindowId}
///         label="Performance window"
///         options={&[
///             ChoiceOption { value: Window::Days30, label: "30 days", link: Some(&days30) },
///             ChoiceOption { value: Window::Days90, label: "90 days", link: Some(&days90) },
///         ]}
///         selected={window}
///     />
///     <ShownFor group={WindowId} values={&[Window::Days30]} selected={window}><p>"Sold 14"</p></ShownFor>
///     <p>"A chart"</p>
///     <ShownFor group={WindowId} values={&[Window::Days90]} selected={window}><p>"Sold 40"</p></ShownFor>
/// }
/// .to_html();
/// assert!(html.contains(r#"<div data-scope="ui-choice-group" id="ui-window" role="radiogroup" aria-label="Performance window""#), "{html}");
/// assert!(html.contains(r#"<a data-ref="radio" role="radio" href="/performance?window=90" aria-checked="false""#), "{html}");
/// // The part for the other window is drawn hidden; each says its group and its words.
/// assert!(html.contains(r#"<p>Sold 14</p></div><p>A chart</p><div hidden data-on="connect:ui.choice.part($t,&quot;ui-window&quot;,[&quot;90&quot;],$s)">"#), "{html}");
/// ```
///
/// A part names its group by the group's id type, so text in its place does not compile:
///
/// ```compile_fail,E0277
/// use corten::prelude::*;
/// use corten_ui::ShownFor;
///
/// let html = html! { <ShownFor group="ui-window" values={&["90"]} selected="30">"Sold 40"</ShownFor> };
/// ```
///
/// And its values are the group's kind of word, so a part for another enum's word does not
/// compile either:
///
/// ```compile_fail,E0308
/// use corten::prelude::*;
/// use corten_ui::ShownFor;
///
/// #[derive(Clone, Copy, PartialEq, UrlValue, serde::Deserialize)]
/// enum Window { Days30, Days90 }
/// #[derive(Clone, Copy, PartialEq, UrlValue, serde::Deserialize)]
/// enum Sort { Price }
/// #[derive(Id)]
/// struct WindowId;
///
/// let html = html! { <ShownFor group={WindowId} values={&[Sort::Price]} selected={Window::Days30}>"x"</ShownFor> };
/// ```
#[component]
pub fn ChoiceGroup<'a, I: Id, V: AsRef<str> + PartialEq, L: Render>(
    id: I,
    label: &'a str,
    options: &'a [ChoiceOption<'a, V, L>],
    selected: V,
) -> impl Render {
    let radio = refs();
    let group = plain_id(&id);
    let words: Vec<&str> = options.iter().map(|o| o.value.as_ref()).collect();
    html! {
        <div id={id} role="radiogroup" aria-label={label} on:connect={choice::connect(radio, group.as_str(), signal)}>
            for o in options {
                match o.link {
                    Some(link) => <a
                        ref={radio as HTMLElement}
                        role="radio"
                        href={link}
                        aria-checked={state(o.value == *selected)}
                        on:click={choice::choose(event, target, radio, group.as_str(), &words)}
                        on:keydown={choice::step(event, target, radio, group.as_str(), &words)}
                    >
                        {o.label}
                    </a>,
                    None => <button
                        ref={radio as HTMLElement}
                        type="button"
                        role="radio"
                        aria-checked={state(o.value == *selected)}
                        on:click={choice::choose(event, target, radio, group.as_str(), &words)}
                        on:keydown={choice::step(event, target, radio, group.as_str(), &words)}
                    >
                        {o.label}
                    </button>,
                }
            }
        </div>
    }
}

/// A part of the page shown only while its [`ChoiceGroup`] has one of `values` chosen: a `div`
/// around the app's `children`, drawn `hidden` unless `selected` (the group's choice, as the
/// handler read it) is one of them, and switched by the group's script after that, wherever on
/// the page it is. A part for each value the figures can be read for is drawn once per value; a
/// part that should keep its place in a grid or a flex row is laid out with `display: contents`.
#[component]
pub fn ShownFor<'a, I: Id, V: AsRef<str> + PartialEq, C: Render>(
    group: I,
    values: &'a [V],
    selected: V,
    children: C,
) -> impl Render {
    let group = plain_id(&group);
    html! {
        <div hidden={!values.contains(selected)} on:connect={choice::part(target, group.as_str(), values, signal)}>
            {children}
        </div>
    }
}
