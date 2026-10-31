use corten::{prelude::*, view::Id, Href};

use crate::{choice as script, copy_button::plain_id, state};

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

/// A choice on the page: its group's `#[derive(Id)]` id and the value the handler read (from the
/// address, or the default). The [`ChoiceGroup`] and every [`ShownFor`] of it take the same one,
/// so a part's values are the group's type and the group's id, and none says the value again.
pub struct Chosen<I, V> {
    pub id: I,
    pub value: V,
}

/// One choice whose value shows and hides parts anywhere on the page: a performance window that
/// both the figures and, below a chart, the list worded from them follow. [`TabGroup`](crate::TabGroup)
/// switches only the panels inside it; this switches every [`ShownFor`] of its choice, wherever
/// the page draws it, also one a patch puts in later.
///
/// The WAI-ARIA radio group pattern: a `div[role=radiogroup]` named by `label`, an option per
/// [`ChoiceOption`] (`[role=radio]`, `[aria-checked=true]` for the chosen one), an `a` or a
/// `button`. The server checks the chosen value and draws every part for another value `hidden`,
/// so the page is right before the script loads and without it. The script (`choice.js`) adds the
/// roving focus (Tab reaches the chosen option, or the first when none is; the arrow keys, Home
/// and End move and choose; Space chooses) and switches in place on a click. A link option's
/// address follows only in the query keys the options differ in, so another control's keys (a
/// tab, a page) stay as they are.
///
/// Parts: the `div[role=radiogroup]` and its `[role=radio]` options; each [`ShownFor`]'s `div`.
///
/// ```
/// use corten::prelude::*;
/// use corten_ui::{ChoiceGroup, ChoiceOption, Chosen, ShownFor};
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
/// let window = Chosen { id: WindowId, value: Window::Days30 };
/// let (days30, days90) = (Performance { window: None }, Performance { window: Some(Window::Days90) });
/// let html = html! {
///     <ChoiceGroup
///         choice={&window}
///         label="Performance window"
///         options={&[
///             ChoiceOption { value: Window::Days30, label: "30 days", link: Some(&days30) },
///             ChoiceOption { value: Window::Days90, label: "90 days", link: Some(&days90) },
///         ]}
///     />
///     <ShownFor choice={&window} values={&[Window::Days30]}><p>"Sold 14"</p></ShownFor>
///     <p>"A chart"</p>
///     <ShownFor choice={&window} values={&[Window::Days90]}><p>"Sold 40"</p></ShownFor>
/// }
/// .to_html();
/// assert!(html.contains(r#"<div data-scope="ui-choice-group" id="ui-window" role="radiogroup" aria-label="Performance window""#), "{html}");
/// assert!(html.contains(r#"<a data-ref="radio" role="radio" href="/performance?window=90" aria-checked="false""#), "{html}");
/// // The part for the other window is drawn hidden; each says its group and its words.
/// assert!(html.contains(r#"<p>Sold 14</p></div><p>A chart</p><div hidden data-on="connect:ui.choice.part($t,&quot;ui-window&quot;,[&quot;90&quot;],$s)">"#), "{html}");
/// ```
///
/// A part's values are its choice's type, so a word of another enum does not compile:
///
/// ```compile_fail,E0308
/// use corten::prelude::*;
/// use corten_ui::{Chosen, ShownFor};
///
/// #[derive(Clone, Copy, PartialEq, UrlValue, serde::Deserialize)]
/// enum Window { Days30, Days90 }
/// #[derive(Clone, Copy, PartialEq, UrlValue, serde::Deserialize)]
/// enum Sort { Price }
/// #[derive(Id)]
/// struct WindowId;
///
/// let window = Chosen { id: WindowId, value: Window::Days30 };
/// let html = html! { <ShownFor choice={&window} values={&[Sort::Price]}>"x"</ShownFor> };
/// ```
///
/// And its id is a `#[derive(Id)]` type, not text:
///
/// ```compile_fail,E0277
/// use corten::prelude::*;
/// use corten_ui::{Chosen, ShownFor};
///
/// let window = Chosen { id: "ui-window", value: "30" };
/// let html = html! { <ShownFor choice={&window} values={&["90"]}>"Sold 40"</ShownFor> };
/// ```
#[component]
pub fn ChoiceGroup<'a, I: Id, V: AsRef<str> + PartialEq, L: Render>(
    choice: &'a Chosen<I, V>,
    label: &'a str,
    options: &'a [ChoiceOption<'a, V, L>],
) -> impl Render {
    let radio = refs();
    let group = plain_id(&choice.id);
    let words: Vec<&str> = options.iter().map(|o| o.value.as_ref()).collect();
    html! {
        <div
            id={&choice.id}
            role="radiogroup"
            aria-label={label}
            on:connect={script::connect(radio, group.as_str(), &words, signal)}
        >
            for o in options {
                match o.link {
                    Some(link) => <a
                        ref={radio as HTMLElement}
                        role="radio"
                        href={link}
                        aria-checked={state(o.value == choice.value)}
                        on:click={script::choose(event, target, radio, group.as_str(), &words)}
                        on:keydown={script::step(event, target, radio, group.as_str(), &words)}
                    >
                        {o.label}
                    </a>,
                    None => <button
                        ref={radio as HTMLElement}
                        type="button"
                        role="radio"
                        aria-checked={state(o.value == choice.value)}
                        on:click={script::choose(event, target, radio, group.as_str(), &words)}
                        on:keydown={script::step(event, target, radio, group.as_str(), &words)}
                    >
                        {o.label}
                    </button>,
                }
            }
        </div>
    }
}

/// A part of the page shown only while its choice has one of `values`: a `div` around the app's
/// `children`, drawn `hidden` unless the choice's value is one of them, and switched by the
/// [`ChoiceGroup`]'s script after that, wherever on the page it is. A part for each value the
/// figures can be read for is drawn once per value; a part that should keep its place in a grid or
/// a flex row is laid out with `display: contents`.
#[component]
pub fn ShownFor<'a, I: Id, V: AsRef<str> + PartialEq, C: Render>(
    choice: &'a Chosen<I, V>,
    values: &'a [V],
    children: C,
) -> impl Render {
    let group = plain_id(&choice.id);
    html! {
        <div hidden={!values.contains(&choice.value)} on:connect={script::part(target, group.as_str(), values, signal)}>
            {children}
        </div>
    }
}
