use corten::prelude::*;

use crate::{model::AlertInput, routes, text::Msg};

/// The price-alert form's values and errors. The item page shows the form; a submit that fails
/// validation patches it on its own. Field names come from `AlertInput::FIELDS`.
#[derive(Clone, Default)]
pub struct AlertFormView<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub email: &'a str,
    pub target_price: &'a str,
    pub errors: Errors,
}

/// The form's id: a submit that fails validation replaces it.
#[derive(Id)]
pub struct AlertFormId;

/// The result's id: a submit that succeeds replaces it.
#[derive(Id)]
pub struct AlertMessageId;

#[component(css)]
pub fn AlertForm<'a>(t: Msg, form: AlertFormView<'a>) -> impl Patch {
    let f = AlertInput::FIELDS;
    let e = form.errors.within(AlertFormId);
    html! {
        <div class={Form} id={AlertFormId}>
            <form method="post" action={routes::Item { id: form.id }} data-patch>
                {form.errors.form_message()}
                <input type="hidden" name={f.title} value={form.title}>
                <label>
                    {t.alert_email()}<input type="email" name={f.email} value={form.email} required {..e.aria(f.email)}>
                </label>
                {e.message(f.email)}
                <label>
                    {t.alert_below()}
                    <input
                        type="text"
                        name={f.target_price}
                        value={form.target_price}
                        inputmode="decimal"
                        required
                        {..e.aria(f.target_price)}
                    >
                </label>
                {e.message(f.target_price)}
                <button type="submit">{t.alert_submit()}</button>
            </form>
        </div>
    }
}

/// The price-alert result, inside the item page and on its own as a patch. It shares the
/// form's stylesheet.
#[component(css = AlertForm)]
pub fn AlertMessage(message: &str) -> impl Patch {
    html! { <p class={Message} id={AlertMessageId} aria-live="polite">{message}</p> }
}
