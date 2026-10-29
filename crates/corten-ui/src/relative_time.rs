use corten::{i18n::Relative, prelude::*};

/// A time relative to now that stays current: "resets in 10 hours", "updated 3 min. ago".
///
/// The server writes it in the request's locale (`t.relative(at, now, style)`), so it is right
/// on the first paint and without JavaScript, inside a `<time datetime>` that says the instant.
/// Its script (`relative_time.js`) writes it again, with the browser's `Intl.RelativeTimeFormat`
/// in the same locale and style, each time its text would change: every second under a minute,
/// then at each minute, hour or day. One timer serves every relative time on the page, set for the
/// next change, so a live part only needs to send when its data changes, not to move a countdown.
/// The browser counts from its own clock.
///
/// Parts: the `time` element alone.
///
/// ```
/// use corten::i18n::RelativeStyle;
/// use corten::prelude::*;
/// use corten_ui::RelativeTime;
/// use std::time::{Duration, UNIX_EPOCH};
///
/// corten::i18n::messages! { struct Msg { "en" => "../corten/src/i18n/en.json" } }
/// use corten::i18n::Messages as _;
///
/// let t = Msg::for_locale("en");
/// let now = UNIX_EPOCH + Duration::from_secs(1_774_191_845); // 2026-03-22T15:04:05Z
/// let reset = now + Duration::from_secs(10 * 3600 + 35 * 60);
/// let html = html! { <p>"Resets "<RelativeTime when={t.relative(reset, now, RelativeStyle::Long)} /></p> }.to_html();
/// assert!(html.contains(r#"<time datetime="2026-03-23T01:39:05Z" data-on="connect:ui.relative_time.connect($t,&quot;en&quot;,&quot;long&quot;,$s)">in 10 hours</time>"#), "{html}");
/// ```
///
/// It takes what `t.relative` returns, so a time written another way does not compile:
///
/// ```compile_fail
/// use corten::prelude::*;
/// use corten_ui::RelativeTime;
///
/// let html = html! { <RelativeTime when="in 10 hours" /> };
/// ```
#[component(js)]
pub fn RelativeTime(when: Relative) -> impl Render {
    html! {
        <time datetime={when.datetime()} on:connect={js::connect(target, when.locale(), when.style().name(), signal)}>
            {when}
        </time>
    }
}
