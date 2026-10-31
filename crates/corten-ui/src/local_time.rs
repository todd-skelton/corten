use corten::{i18n::When, prelude::*};

/// A date or time in the visitor's zone, right on the first page too.
///
/// The server writes it as `t.date`, `t.time` and `t.date_time` do: in the visitor's zone
/// once their browser has reported it (`App::zoned_dates`, Corten's `zones` feature), inside a
/// `<time datetime>` that says the instant. A browser's first page comes before that report, so
/// there the server writes UTC, and its script (`local_time.js`) writes the same text again in the
/// browser's zone, with the server's own format for the locale and style (so a later page, written
/// by the server, reads the same). A `Public` page, the same for everyone, is written that way on
/// every visit. Only those pages load the script. Without JavaScript they stay in UTC. Without
/// `App::zoned_dates` it is the date as written, on every page: Corten writes no zone, and the
/// script never runs.
///
/// Where the script runs, each date carries its format to the browser: about 850 bytes before
/// compression against about 70 for the date alone (the gallery's medium date and short time,
/// `en`), so a first or `Public` page with many dates (a list) is the place for one zone the
/// server chooses (`.zone(..)`) or the date alone, rather than one `LocalTime` per row.
///
/// Parts: the `time` element alone.
///
/// ```
/// use corten::i18n::{DateStyle, TimeStyle};
/// use corten::prelude::*;
/// use corten_ui::LocalTime;
/// use std::time::{Duration, UNIX_EPOCH};
///
/// corten::i18n::messages! { struct Msg { "en" => "../corten/src/i18n/en.json" } }
/// use corten::i18n::Messages as _;
///
/// let t = Msg::for_locale("en");
/// let at = UNIX_EPOCH + Duration::from_secs(1_774_191_845); // 2026-03-22T15:04:05Z
/// let html = html! { <p>"Created "<LocalTime when={t.date_time(at, DateStyle::Short, TimeStyle::Short)} /></p> }.to_html();
/// // Rendered outside a request (no visitor to ask), it is the date as written: no script.
/// assert_eq!(html, r#"<p>Created <time datetime="2026-03-22T15:04:05Z">3/22/2026, 3:04 PM UTC</time></p>"#);
/// ```
///
/// It takes what `t.date` returns, so a date written another way does not compile:
///
/// ```compile_fail
/// use corten::prelude::*;
/// use corten_ui::LocalTime;
///
/// let html = html! { <LocalTime when="3/22/26, 3:04 PM" /> };
/// ```
#[component(js)]
pub fn LocalTime(when: When) -> impl Render {
    html! {
        if when.zone_unknown() {
            <time datetime={when.datetime()} on:connect={js::connect(target, when.format())}>{when}</time>
        } else {
            <time datetime={when.datetime()}>{when}</time>
        }
    }
}
