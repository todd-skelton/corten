use corten::prelude::*;

use crate::app::{SignInInput, door};

/// The sign-in form's id: a refused sign-in replaces it with its errors.
#[derive(Id)]
pub struct SignInFormId;

/// Signs in under a name. The house opens its part with the house key.
#[component(css)]
pub fn SignInForm<'a>(typed: &'a str, errors: &'a Errors) -> impl Patch {
    let f = SignInInput::FIELDS;
    let e = errors.within(SignInFormId);
    html! {
        <form id={SignInFormId} class={SignIn} method="post" action={door::SignIn} data-patch>
            <h1>"Sign in"</h1>
            <p>"Pick the name the other bidders see. There is no password in this demo."</p>
            <label>
                "Name"
                <input name={f.name} value={typed} autocomplete="nickname" maxlength="40" required {..e.aria(f.name)}>
            </label>
            {e.message(f.name)}
            <details open={errors.has(f.house_key)}>
                <summary>"I am the house"</summary>
                <label>
                    "House key"
                    <input name={f.house_key} type="password" autocomplete="off" {..e.aria(f.house_key)}>
                </label>
                {e.message(f.house_key)}
            </details>
            <button>"Sign in"</button>
        </form>
    }
}
