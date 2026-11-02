//! `#[component]`: a function becomes a struct with one field per parameter, and a
//! `corten::view::Render` impl that runs the function body when the struct renders.
//!
//! With `css`, the component has a stylesheet next to it (see `crate::style`); with `js`, a script
//! of handlers (see `crate::script`); with `view`, a class scripts write its holes through (see
//! `crate::html::view_of`). Any of them gives it a constant with its name in the value
//! namespace, `OfferList: corten::Component`, which the app lists once in `App::components`.

use proc_macro::{Delimiter, Group, Ident, Punct, Spacing, Span, TokenStream, TokenTree};

use crate::{
    style::Css,
    tokens::{err, is_ident, is_punct, literal, q, stream, Error},
};

/// `css` (the component's own `{name}.css`) or `css = path::Owner` (another component's, from
/// any module that can see it).
pub enum CssArg {
    Own(Span),
    /// `css = path::Owner`: the tokens before the owner (`super::screen::`, or none), the owner,
    /// and its stylesheet's path once the owner's module has given it (see `Css::items`).
    Of {
        path: TokenStream,
        owner: Ident,
        sheet: Option<String>,
    },
}

/// `js` (the component's own `{name}.js`) or `js = path::Owner` (another component's handlers).
pub enum JsArg {
    Own(Span),
    /// `js = path::Owner`: the tokens before the owner, and the owner.
    Of {
        path: TokenStream,
        owner: Ident,
    },
}

/// What `#[component(..)]` takes: `css` or `css = Owner`, and `js` or `js = Owner`.
#[derive(Default)]
pub struct Args {
    pub css: Option<CssArg>,
    /// `js`: the handlers are the script next to the component, `{name}.js`; `js = Owner`:
    /// another component's, for a part of it.
    pub js: Option<JsArg>,
    /// `view`: scripts get a class with the component's holes, in `corten/views`.
    pub view: Option<Span>,
}

/// Parses `#[component]`, `#[component(css)]`, `#[component(css = Owner)]`, `#[component(js)]`,
/// `#[component(view)]` or several, comma-separated.
pub fn args(args: TokenStream) -> Result<Args, Error> {
    let usage = "#[component] takes css (a stylesheet named after the component, next to it), css = OtherComponent or css = path::to::OtherComponent (its stylesheet, from another module of the crate), js (a script named after it), js = OtherComponent or js = path::to::OtherComponent (its handlers, for a part of it, from another module of the crate) and view (a class scripts write its holes through)";
    let mut out = Args::default();
    let mut sheet = None;
    for (i, part) in split(args, ',').into_iter().enumerate() {
        match part.as_slice() {
            // `@ "/path/owner.css"`, first: put there by the owner's `__corten_sheet!`.
            [TokenTree::Punct(at), TokenTree::Literal(path)] if at.as_char() == '@' && i == 0 => {
                sheet = crate::tokens::string_value(path);
            }
            [TokenTree::Ident(css)] if css.to_string() == "css" && out.css.is_none() => out.css = Some(CssArg::Own(css.span())),
            [TokenTree::Ident(css), TokenTree::Punct(eq), path @ .., TokenTree::Ident(owner)]
                if css.to_string() == "css" && eq.as_char() == '=' && out.css.is_none() && owner_path(path) =>
            {
                out.css = Some(CssArg::Of { path: stream(path.iter().cloned()), owner: owner.clone(), sheet: None })
            }
            [TokenTree::Ident(js)] if js.to_string() == "js" && out.js.is_none() => out.js = Some(JsArg::Own(js.span())),
            [TokenTree::Ident(js), TokenTree::Punct(eq), path @ .., TokenTree::Ident(owner)]
                if js.to_string() == "js" && eq.as_char() == '=' && out.js.is_none() && owner_path(path) =>
            {
                out.js = Some(JsArg::Of { path: stream(path.iter().cloned()), owner: owner.clone() })
            }
            [TokenTree::Ident(view)] if view.to_string() == "view" && out.view.is_none() => out.view = Some(view.span()),
            [first, ..] => return err(first.span(), usage),
            [] => {}
        }
    }
    match (&mut out.css, sheet) {
        (Some(CssArg::Of { sheet: given, .. }), Some(path)) => *given = Some(path),
        (_, Some(_)) => return err(Span::call_site(), usage),
        _ => {}
    }
    Ok(out)
}

/// `__corten_js_Sheet`: the handlers of `Sheet`'s script, beside it, which `js = Sheet` uses.
fn js_module(owner: &Ident) -> Ident {
    Ident::new(&format!("__corten_js_{owner}"), owner.span())
}

/// A path to a component, without the component: nothing, or `a::b::`, `super::`, `crate::`.
fn owner_path(path: &[TokenTree]) -> bool {
    path.chunks(3).all(|part| match part {
        [TokenTree::Ident(_), TokenTree::Punct(a), TokenTree::Punct(b)] => a.as_char() == ':' && b.as_char() == ':',
        _ => false,
    })
}

pub fn expand(args: TokenStream, item: TokenStream) -> Result<TokenStream, Error> {
    let tokens = args.clone();
    let args = self::args(args)?;
    // `css = path::Owner`: the owner's module knows where its stylesheet is, so it is asked, and
    // calls this attribute again with the path first. The module is the owner's, so Rust's own
    // visibility decides which components can share the sheet.
    if let Some(CssArg::Of { path, owner, sheet: None }) = &args.css {
        // The owner itself, imported so its visibility decides who may share its sheet, and a
        // missing one is reported by the name written. The module has the owner's span, so an
        // owner without a stylesheet is reported at its name.
        let mut owner_path = path.clone();
        owner_path.extend([TokenTree::Ident(owner.clone())]);
        let mut call = q("#[allow(unused_imports)] use __A0 as _;", &[owner_path]);
        let module = crate::style::module(owner);
        call.extend(path.clone());
        call.extend(q("__A0::__corten_sheet! { [__A1] __A2 }", &[TokenTree::Ident(module).into(), tokens, item]));
        return Ok(call);
    }
    build(item, args)
}

/// The struct, its `Render` impl and, for a component with a script or stylesheet, the constant
/// the app lists.
pub fn build(item: TokenStream, args: Args) -> Result<TokenStream, Error> {
    let what = "#[component]";
    let tokens: Vec<TokenTree> = item.into_iter().collect();
    let mut i = 0;
    let mut attrs = Vec::new();
    let mut vis = Vec::new();
    // Attributes and doc comments, then the visibility.
    loop {
        match tokens.get(i) {
            Some(TokenTree::Punct(p)) if p.as_char() == '#' => {
                attrs.extend(tokens[i..i + 2].iter().cloned());
                i += 2;
            }
            Some(TokenTree::Ident(id)) if id.to_string() == "pub" => {
                vis.push(tokens[i].clone());
                i += 1;
                if let Some(TokenTree::Group(g)) = tokens.get(i) {
                    if g.delimiter() == Delimiter::Parenthesis {
                        vis.push(tokens[i].clone());
                        i += 1;
                    }
                }
            }
            _ => break,
        }
    }
    let span = tokens.get(i).map_or(Span::call_site(), TokenTree::span);
    if !is_ident(tokens.get(i), "fn") {
        return err(span, format!("{what} goes on a function: {what} fn Card(title: &str) -> impl Render {{ html! {{ ... }} }}"));
    }
    i += 1;
    let name = match tokens.get(i) {
        Some(TokenTree::Ident(name)) => name.clone(),
        _ => return err(span, "expected the component's name"),
    };
    if !name.to_string().starts_with(|c: char| c.is_ascii_uppercase()) {
        return err(name.span(), "a component's name starts with an uppercase letter, so html! can tell <Card> from <div>");
    }
    i += 1;
    // Generics, kept as written for the struct; the impl needs the same list without defaults,
    // and the type needs only the parameter names.
    let mut generics: Vec<TokenTree> = Vec::new();
    if is_punct(tokens.get(i), '<') {
        let mut depth = 0;
        let mut previous_dash = false;
        loop {
            let Some(token) = tokens.get(i) else { return err(span, "unclosed generics") };
            i += 1;
            if let TokenTree::Punct(p) = token {
                match p.as_char() {
                    '<' => depth += 1,
                    '>' if !previous_dash => depth -= 1,
                    _ => {}
                }
                previous_dash = p.as_char() == '-' && p.spacing() == Spacing::Joint;
            } else {
                previous_dash = false;
            }
            generics.push(token.clone());
            if depth == 0 {
                break;
            }
        }
    }
    let params = match tokens.get(i) {
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => g.clone(),
        _ => return err(span, "expected the parameter list"),
    };
    i += 1;
    let mut where_clause = Vec::new();
    let mut in_where = false;
    // `-> impl Document`: a layout, which renders a whole document. `-> impl Patch`: markup a
    // `data-patch` answer can send, which must be a patch (see `corten::view::Patch`).
    let mut returns_doc = false;
    let mut returns_patch = false;
    // `-> impl ClassList`: a look, the classes it puts on an element the caller writes.
    let mut returns_classes = false;
    let body = loop {
        match tokens.get(i) {
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace && i + 1 == tokens.len() => break g.clone(),
            Some(TokenTree::Ident(id)) if id.to_string() == "where" => {
                in_where = true;
                where_clause.push(tokens[i].clone());
            }
            Some(token) if in_where => where_clause.push(token.clone()),
            Some(TokenTree::Ident(id)) if id.to_string() == "Document" => returns_doc = true,
            Some(TokenTree::Ident(id)) if id.to_string() == "Patch" => returns_patch = true,
            Some(TokenTree::Ident(id)) if id.to_string() == "ClassList" => returns_classes = true,
            Some(_) => {}
            None => return err(span, "expected the function body"),
        }
        i += 1;
    };

    let mut fields = TokenStream::new();
    let mut lets = TokenStream::new();
    let mut elided = false;
    // `impl Trait` parameters, each a type parameter named after its field, with its bounds.
    let mut impl_params: Vec<(Ident, TokenStream)> = Vec::new();
    // Each parameter's name and type as written, for a view's holes.
    let mut typed: Vec<(String, Vec<TokenTree>)> = Vec::new();
    for param in split(params.stream(), ',') {
        let mut p = 0;
        let mut field_attrs = Vec::new();
        while is_punct(param.get(p), '#') {
            if is_prop(&param[p..p + 2]) {
                return err(
                    param[p].span(),
                    "#[prop] is gone: pass the value to the handler that uses it, on:connect={js::start(signal, value)}",
                );
            }
            field_attrs.extend(param[p..p + 2].iter().cloned());
            p += 2;
        }
        if is_ident(param.get(p), "mut") {
            p += 1;
        }
        let (Some(TokenTree::Ident(field)), true) = (param.get(p), is_punct(param.get(p + 1), ':')) else {
            let span = param.first().map_or(params.span(), TokenTree::span);
            return err(span, "component parameters are `name: Type`");
        };
        typed.push((field.to_string(), param[p + 2..].to_vec()));
        let mut ty = name_elided_lifetimes(param[p + 2..].to_vec(), &mut elided);
        // `children: impl Render` is a type parameter of the struct, as `C: Render` would be.
        let written: Vec<TokenTree> = ty.clone().into_iter().collect();
        if is_ident(written.first(), "impl") {
            let param_name = Ident::new(&format!("__CortenParam{}", impl_params.len()), field.span());
            impl_params.push((param_name.clone(), stream(written[1..].iter().cloned())));
            ty = TokenTree::Ident(param_name).into();
        }
        fields.extend(stream(field_attrs));
        fields.extend(stream(vis.iter().cloned()));
        fields.extend(q("__A0: __A1,", &[TokenTree::Ident(field.clone()).into(), ty]));
        lets.extend(q(
            "#[allow(unused_variables)] let __A0 = (&::corten::view::__private::Param(&self.__A0)).__param();",
            &[TokenTree::Ident(field.clone()).into()],
        ));
    }

    if elided {
        // `title: &str` needs a named lifetime in a struct field; every elided one gets the same.
        let mut named = vec![TokenTree::Punct(Punct::new('<', Spacing::Alone))];
        named.extend(lifetime());
        if generics.len() > 2 {
            named.push(TokenTree::Punct(Punct::new(',', Spacing::Alone)));
            named.extend(generics[1..].iter().cloned());
        } else {
            named.push(TokenTree::Punct(Punct::new('>', Spacing::Alone)));
        }
        generics = named;
    }
    if !impl_params.is_empty() {
        let mut list: Vec<TokenTree> = if generics.len() > 2 {
            let mut list = generics[..generics.len() - 1].to_vec();
            list.push(TokenTree::Punct(Punct::new(',', Spacing::Alone)));
            list
        } else {
            vec![TokenTree::Punct(Punct::new('<', Spacing::Alone))]
        };
        for (i, (param_name, bounds)) in impl_params.iter().enumerate() {
            if i > 0 {
                list.push(TokenTree::Punct(Punct::new(',', Spacing::Alone)));
            }
            list.push(TokenTree::Ident(param_name.clone()));
            list.push(TokenTree::Punct(Punct::new(':', Spacing::Alone)));
            list.extend(bounds.clone());
        }
        list.push(TokenTree::Punct(Punct::new('>', Spacing::Alone)));
        generics = list;
    }
    let (impl_generics, type_generics) = split_generics(&generics);

    // Refs (`let (rows, rest) = refs();`) and the scope they belong to: the component's one root
    // element, marked `data-scope` by its html!.
    let scope_name = match crate::script::prefix() {
        Some(prefix) => format!("{prefix}-{}", crate::names::kebab(&name.to_string())),
        None => crate::names::kebab(&name.to_string()),
    };
    let view = match args.view {
        Some(span) => Some(crate::html::view_of(&body, &name.to_string(), &typed, span)?),
        None => None,
    };
    // A view's number property is a JavaScript number in the page: debug builds stop a value it
    // cannot hold exactly (a u64 past 2^53), as for a handler's number argument.
    for field in view.iter().flat_map(|v| &v.numbers) {
        lets.extend(q(
            "::corten::script::__private::ViewNumber::check(&self.__A0, __A1);",
            &[TokenTree::Ident(Ident::new(field, Span::call_site())).into(), literal(field)],
        ));
    }
    let (script, js_owner) = match args.js {
        Some(JsArg::Own(span)) => {
            let file = format!("{}.js", crate::names::snake(&name.to_string()));
            let path = crate::style::locate(&file, span)?;
            (Some(crate::script::Module::read(&path, span)?), None)
        }
        Some(JsArg::Of { path, owner }) => {
            if owner.to_string() == name.to_string() || owner.to_string() == "Self" {
                return err(owner.span(), "js = names another component; for this component's own script write js");
            }
            (None, Some((path, owner)))
        }
        None => (None, None),
    };
    let (body, has_refs) = refs(&body, &scope_name)?;
    // A script that keeps state (`keep`) keeps it on the component's root, one per instance.
    // In a part (`js = Owner`), the `scope` its owner's handlers take is the owner's root, so they
    // do not make the part a root; any other handler's `scope` does, as in any component.
    let takes_scope = handlers_take_scope(&body.stream(), js_owner.as_ref().map(|_| false));
    let scoped = has_refs || takes_scope || script.as_ref().is_some_and(|s| s.keeps);
    let owner_scope = js_owner.is_some() && handlers_take_scope(&body.stream(), Some(true));
    if returns_classes && (script.is_some() || js_owner.is_some() || view.is_some() || scoped) {
        return err(
            name.span(),
            format!("{name} returns classes, which a caller puts on its own element, so it has no markup to give a script, refs, a scope or a view"),
        );
    }
    let body = if scoped { mark_root(&body, &scope_name)? } else { body };

    let css = args.css;
    let css_owner = match &css {
        Some(CssArg::Of { path, owner, .. }) => Some((path.clone(), owner.clone())),
        _ => None,
    };
    let (css, own_css) = match &css {
        None => (None, false),
        Some(CssArg::Own(span)) => (Some(Css::read(&name, *span)?), true),
        Some(CssArg::Of { owner, sheet, .. }) => {
            if owner.to_string() == name.to_string() {
                return err(owner.span(), "css = names another component; for this component's own stylesheet write css");
            }
            let Some(sheet) = sheet else { unreachable!("expand asks the owner's module for its sheet first") };
            (Some(Css::read_at(owner, std::path::PathBuf::from(sheet), owner.span())?), false)
        }
    };
    let mut used = Vec::new();
    let body = match &css {
        Some(css) => {
            let mut copy = Group::new(Delimiter::Brace, css.rewrite(body.stream(), false, &mut used)?);
            copy.set_span(body.span());
            copy
        }
        None => body,
    };
    let mut out = stream(attrs);
    out.extend(stream(vis.iter().cloned()));
    out.extend(q(
        "struct __A0 __A1 __A2",
        &[TokenTree::Ident(name.clone()).into(), stream(generics.clone()), stream(where_clause.clone())],
    ));
    out.extend([TokenTree::Group(Group::new(Delimiter::Brace, fields))]);
    let mut scope = TokenStream::new();
    let mut items = TokenStream::new();
    // The names this component's constant takes for the whole program (`crate::id::guards`).
    let mut guarded: Vec<(&str, String, bool)> = Vec::new();
    // The constant the app lists: the component's script and its own stylesheet.
    if let Some(css) = &css {
        // A helper (`css = Owner`) uses the owner's constants, so its uses count for the owner.
        let (path, style_owner) = css_owner.clone().unwrap_or_else(|| (TokenStream::new(), name.clone()));
        if own_css {
            items.extend(css.items(&name));
            // Two components that style `.{prefix}__*` fail to build, not the app when it starts.
            // A library's classes are not prefixed, so its `Card` and the app's clash at any version.
            guarded.push(("two_components_style", css.prefix.clone(), false));
        }
        scope.extend(css.scope(&path, &style_owner, &used));
    }
    let has_constant = own_css || script.is_some() || view.is_some();
    // `corten/views` exports one class of a name, unprefixed, so two views of one name fail to
    // build, at any version.
    if view.is_some() {
        guarded.push(("two_views_are", name.to_string(), false));
    }
    if let Some(module) = &script {
        // Two components whose scripts are served at one URL fail to build.
        guarded.push(("two_scripts_are", module.name.clone(), true));
        // Beside the component, named after it, so two components in one file each have their
        // own, and a part of it (`js = Owner`) can use it; `js` inside `render`.
        let module_name = js_module(&name);
        items.extend(module.module(
            q("#[doc(hidden)] pub(crate)", &[]),
            module_name.clone(),
            q("::core::option::Option::None", &[]),
            // A scoped component's handlers are its own wherever they render, children a child
            // renders included, and a part's.
            scoped.then_some(scope_name.as_str()),
            // Whether it has a root, which a part's handler taking `scope` needs.
            q(if scoped { "pub const SCOPED: bool = true;" } else { "pub const SCOPED: bool = false;" }, &[]),
        ));
        scope.extend(q("#[allow(unused_imports)] use __A0 as js;", &[TokenTree::Ident(module_name).into()]));
    }
    if let Some((path, owner)) = &js_owner {
        // The owner's handlers, through its module: Rust's visibility of it decides which
        // components can use them, and an owner without a script is an error at its name.
        let mut module = path.clone();
        module.extend([TokenTree::Ident(js_module(owner))]);
        let mut owner_path = path.clone();
        owner_path.extend([TokenTree::Ident(owner.clone())]);
        scope.extend(q(
            "#[allow(unused_imports)] use __A0 as _; #[allow(unused_imports)] use __A1 as js;",
            &[owner_path, module.clone()],
        ));
        if owner_scope {
            let message = format!(
                "a handler of {owner}'s here takes scope, {owner}'s root, but {owner} has none: give {owner} a ref, a handler that takes scope, or a script that keeps state, so its markup has a root"
            );
            scope.extend(q("const _: () = ::core::assert!(__A0::SCOPED, __A1);", &[module, literal(&message)]));
        }
    }
    // A part (`css = Owner`, `js = Owner`) renders as its owner, which the app lists.
    let owner = if has_constant {
        Some(TokenTree::Ident(name.clone()).into())
    } else {
        css_owner.or(js_owner.clone()).map(|(path, owner)| {
            let mut out = path;
            out.extend([TokenTree::Ident(owner)]);
            out
        })
    };
    if has_constant {
        let names: Vec<_> = guarded.iter().map(|(what, n, v)| (*what, n.as_str(), *v)).collect();
        items.extend(crate::id::guards(&name.to_string(), &names));
        let some = |t: Option<TokenStream>| match t {
            Some(t) => q("::core::option::Option::Some(__A0)", &[t]),
            None => q("::core::option::Option::None", &[]),
        };
        let style = if own_css { css.as_ref().map(Css::style) } else { None };
        let doc = format!(" The script, stylesheet and view of the `{name}` component, for `App::components`.");
        let view = view.as_ref().map(|v| {
            q(
                "::corten::script::ViewDef { class: __A0, js: __A1, ts: __A2 }",
                &[literal(&name.to_string()), literal(&v.js), literal(&v.ts)],
            )
        });
        items.extend(q(
            "#[doc = __A0] #[allow(non_upper_case_globals, dead_code)] __A5 const __A1: ::corten::Component = ::corten::Component {
                name: __A2,
                script: __A3,
                style: __A4,
                event: ::core::option::Option::None,
                view: __A6,
                anchor: __A7,
            };",
            &[
                literal(&doc),
                TokenTree::Ident(name.clone()).into(),
                literal(&name.to_string()),
                some(script.as_ref().map(|s| s.literal(""))),
                some(style),
                stream(vis.iter().cloned()),
                some(view),
                crate::id::anchor(&name.to_string()),
            ],
        ));
    }
    let used = match owner {
        Some(owner) => q("#[cfg(debug_assertions)] ::corten::style::__private::rendered(&__A0);", &[owner]),
        None => TokenStream::new(),
    };
    if returns_classes {
        if css.is_none() {
            return err(
                name.span(),
                format!(
                    "{name} returns classes, so it is a look: #[component(css)] (or css = Owner), its classes from a stylesheet"
                ),
            );
        }
        // The classes, with the stylesheet's constants in scope, written where `class={..}` is.
        let mut markup = scope;
        markup.extend(q(
            "::corten::style::ClassList::write_classes(&__A0, __corten_component_out);",
            &[TokenTree::Group(body).into()],
        ));
        let markup = TokenTree::Group(Group::new(Delimiter::Brace, markup));
        let write = q(
            "fn write_classes(&self, __corten_component_out: &mut ::std::string::String) {
                #[allow(unused_imports)]
                use ::corten::view::__private::{ParamCopy as _, ParamRef as _};
                __A0
                __A1
                __A2
            }",
            &[lets, markup.into(), used],
        );
        out.extend(q(
            "#[automatically_derived] impl __A0 ::corten::style::ClassList for __A1 __A2 __A3",
            &[
                impl_generics,
                TokenTree::Ident(Ident::new(&name.to_string(), name.span())).into(),
                type_generics,
                stream(where_clause),
            ],
        ));
        out.extend([TokenTree::Group(Group::new(Delimiter::Brace, write))]);
        out.extend(items);
        return Ok(out);
    }
    let doc_body = body.clone();
    let render_fn = if returns_patch { "::corten::view::__private::render_patch" } else { "::corten::view::Render::render" };
    let draw = if scoped {
        q(
            &format!("::corten::script::__private::scoped(__A1, || {render_fn}(&__A0, __corten_component_out));"),
            &[TokenTree::Group(body).into(), literal(&scope_name)],
        )
    } else {
        q(&format!("{render_fn}(&__A0, __corten_component_out);"), &[TokenTree::Group(body).into()])
    };
    // The stylesheet's classes are in scope only around the markup: a class named like the
    // component (`.card` in `Card`'s stylesheet) is what `{Card}` means there, while the
    // component's own `Component` constant stays reachable outside, for `rendered`.
    let mut markup = scope;
    markup.extend(draw);
    let markup = TokenTree::Group(Group::new(Delimiter::Brace, markup));
    if returns_patch {
        for path in ["::corten::view::__private::PatchSealed", "::corten::view::Patch"] {
            out.extend(q(
                &format!("#[automatically_derived] impl __A0 {path} for __A1 __A2 __A3 {{}}"),
                &[
                    impl_generics.clone(),
                    TokenTree::Ident(Ident::new(&name.to_string(), name.span())).into(),
                    type_generics.clone(),
                    stream(where_clause.clone()),
                ],
            ));
        }
    }
    if returns_doc {
        let render_part = q(
            "fn render_part(&self, __corten_component_out: &mut ::std::string::String, __corten_part: ::corten::view::Part) {
                #[allow(unused_imports)]
                use ::corten::view::__private::{ParamCopy as _, ParamRef as _};
                __A0
                ::corten::view::Document::render_part(&__A1, __corten_component_out, __corten_part);
            }",
            &[lets.clone(), TokenTree::Group(doc_body).into()],
        );
        out.extend(q(
            "#[automatically_derived] impl __A0 ::corten::view::Document for __A1 __A2 __A3",
            &[
                impl_generics.clone(),
                TokenTree::Ident(Ident::new(&name.to_string(), name.span())).into(),
                type_generics.clone(),
                stream(where_clause.clone()),
            ],
        ));
        out.extend([TokenTree::Group(Group::new(Delimiter::Brace, render_part))]);
    }
    let render = q(
        "fn render(&self, __corten_component_out: &mut ::std::string::String) {
            #[allow(unused_imports)]
            use ::corten::view::__private::{ParamCopy as _, ParamRef as _};
            __A0
            __A1
            __A2
        }",
        &[lets, markup.into(), used],
    );
    out.extend(q(
        "#[automatically_derived] impl __A0 ::corten::view::Render for __A1 __A2 __A3",
        &[
            impl_generics,
            TokenTree::Ident(Ident::new(&name.to_string(), name.span())).into(),
            type_generics,
            stream(where_clause),
        ],
    ));
    out.extend([TokenTree::Group(Group::new(Delimiter::Brace, render))]);
    out.extend(items);
    Ok(out)
}

/// True for the parameter attribute `#[prop]` (no longer supported).
fn is_prop(attr: &[TokenTree]) -> bool {
    matches!(attr, [TokenTree::Punct(p), TokenTree::Group(g)] if p.as_char() == '#' && {
        let inner: Vec<TokenTree> = g.stream().into_iter().collect();
        inner.len() == 1 && is_ident(inner.first(), "prop")
    })
}

/// Replaces `let (rows, rest) = refs();` at the top level of the body with each ref, named and
/// scoped to this component. Returns whether there were any.
fn refs(body: &Group, scope: &str) -> Result<(Group, bool), Error> {
    let tokens: Vec<TokenTree> = body.stream().into_iter().collect();
    let mut out: Vec<TokenTree> = Vec::with_capacity(tokens.len());
    let mut found = false;
    let mut i = 0;
    while i < tokens.len() {
        let is_refs_call = |at: usize| {
            is_ident(tokens.get(at), "refs")
                && matches!(tokens.get(at + 1), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis && g.stream().is_empty())
        };
        if is_ident(tokens.get(i), "let") && is_punct(tokens.get(i + 2), '=') && is_refs_call(i + 3) {
            let names: Vec<Ident> = match &tokens[i + 1] {
                TokenTree::Ident(one) => vec![one.clone()],
                TokenTree::Group(g) if g.delimiter() == Delimiter::Parenthesis => {
                    let mut list = Vec::new();
                    for part in split(g.stream(), ',') {
                        match part.as_slice() {
                            [TokenTree::Ident(n)] => list.push(n.clone()),
                            [first, ..] => return err(first.span(), "refs() names plain variables: let (rows, rest) = refs();"),
                            [] => {}
                        }
                    }
                    list
                }
                other => return err(other.span(), "refs() names plain variables: let (rows, rest) = refs();"),
            };
            for n in &names {
                let text = n.to_string();
                if !text.starts_with(|c: char| c.is_ascii_lowercase())
                    || !text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    return err(n.span(), format!("{text}: a ref is a snake_case variable, its name in the page"));
                }
            }
            let make = |n: &Ident| {
                q("::corten::script::__private::new_ref::<_, _>(__A0, __A1)", &[literal(scope), literal(&n.to_string())])
            };
            let value = if names.len() == 1 && matches!(tokens[i + 1], TokenTree::Ident(_)) {
                make(&names[0])
            } else {
                let mut list = TokenStream::new();
                for n in &names {
                    list.extend(make(n));
                    list.extend(q(",", &[]));
                }
                crate::tokens::parens(list)
            };
            out.extend(tokens[i..i + 3].iter().cloned());
            out.extend(value);
            found = true;
            i += 5;
            continue;
        }
        if is_refs_call(i) {
            return err(tokens[i].span(), "refs() goes at the top of a component's body: let (rows, rest) = refs();");
        }
        out.push(tokens[i].clone());
        i += 1;
    }
    let mut copy = Group::new(Delimiter::Brace, stream(out));
    copy.set_span(body.span());
    Ok((copy, found))
}

/// True when a handler in the markup (`on:click={js::close(scope)}`) takes the keyword `scope`.
/// A variable, field or call that happens to be named `scope` elsewhere does not count. `js`:
/// only the calls through `js::` (`Some(true)`), only the others (`Some(false)`), or any.
fn handlers_take_scope(tokens: &TokenStream, js: Option<bool>) -> bool {
    let tokens: Vec<TokenTree> = tokens.clone().into_iter().collect();
    tokens.iter().enumerate().any(|(i, token)| match token {
        TokenTree::Ident(on) if on.to_string() == "on" => {
            is_punct(tokens.get(i + 1), ':')
                && matches!(tokens.get(i + 2), Some(TokenTree::Ident(_)))
                && is_punct(tokens.get(i + 3), '=')
                && matches!(tokens.get(i + 4), Some(TokenTree::Group(value))
                    if crate::html::passes_keyword(value.stream(), "scope")
                        && js.is_none_or(|js| js == through_js(&value.stream())))
        }
        TokenTree::Group(g) => handlers_take_scope(&g.stream(), js),
        _ => false,
    })
}

/// A handler written `js::name(..)`: the component's own script's, or its owner's in a part.
fn through_js(value: &TokenStream) -> bool {
    let tokens: Vec<TokenTree> = value.clone().into_iter().take(3).collect();
    is_ident(tokens.first(), "js") && is_punct(tokens.get(1), ':') && is_punct(tokens.get(2), ':')
}

/// Tells the body's final `html! { .. }` to mark its root element with the component's scope.
fn mark_root(body: &Group, scope: &str) -> Result<Group, Error> {
    let mut tokens: Vec<TokenTree> = body.stream().into_iter().collect();
    let n = tokens.len();
    let at_end = n >= 3
        && is_ident(tokens.get(n - 3), "html")
        && is_punct(tokens.get(n - 2), '!')
        && matches!(tokens.get(n - 1), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace);
    if !at_end {
        return err(
            body.span(),
            "a component with refs, a handler taking scope, or a script that keeps state (keep) ends with html! { <root> .. </root> }: the root element is where its refs and state end",
        );
    }
    let TokenTree::Group(g) = &tokens[n - 1] else { unreachable!() };
    let mut inner = q("@ scope __A0 ;", &[literal(scope)]);
    inner.extend(g.stream());
    let mut marked = Group::new(Delimiter::Brace, inner);
    marked.set_span(g.span());
    tokens[n - 1] = TokenTree::Group(marked);
    let mut copy = Group::new(Delimiter::Brace, stream(tokens));
    copy.set_span(body.span());
    Ok(copy)
}

fn lifetime() -> [TokenTree; 2] {
    [TokenTree::Punct(Punct::new('\'', Spacing::Joint)), TokenTree::Ident(Ident::new("__corten", Span::call_site()))]
}

/// `&T` becomes `&'__corten T` and `'_` becomes `'__corten`, except inside `Fn(..)` sugar, where
/// elision means something else.
fn name_elided_lifetimes(tokens: Vec<TokenTree>, elided: &mut bool) -> TokenStream {
    let mut out: Vec<TokenTree> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            TokenTree::Punct(p) if p.as_char() == '&' => {
                out.push(tokens[i].clone());
                if !is_punct(tokens.get(i + 1), '\'') {
                    out.extend(lifetime());
                    *elided = true;
                }
            }
            TokenTree::Punct(p) if p.as_char() == '\'' && is_ident(tokens.get(i + 1), "_") => {
                out.extend(lifetime());
                *elided = true;
                i += 1;
            }
            TokenTree::Group(g) => {
                let after_fn = matches!(out.last(), Some(TokenTree::Ident(id)) if matches!(id.to_string().as_str(), "Fn" | "FnMut" | "FnOnce"));
                let inner = if after_fn { g.stream() } else { name_elided_lifetimes(g.stream().into_iter().collect(), elided) };
                let mut copy = Group::new(g.delimiter(), inner);
                copy.set_span(g.span());
                out.push(TokenTree::Group(copy));
            }
            other => out.push(other.clone()),
        }
        i += 1;
    }
    stream(out)
}

/// Splits tokens at top-level `sep`; `<..>` nests (`->` does not close it).
pub(crate) fn split(tokens: TokenStream, sep: char) -> Vec<Vec<TokenTree>> {
    let mut parts = vec![Vec::new()];
    let mut depth = 0i32;
    let mut previous_dash = false;
    for token in tokens {
        if let TokenTree::Punct(p) = &token {
            match p.as_char() {
                '<' => depth += 1,
                '>' if !previous_dash => depth -= 1,
                c if c == sep && depth == 0 => {
                    parts.push(Vec::new());
                    previous_dash = false;
                    continue;
                }
                _ => {}
            }
            previous_dash = p.as_char() == '-' && p.spacing() == Spacing::Joint;
        } else {
            previous_dash = false;
        }
        parts.last_mut().expect("never empty").push(token);
    }
    parts.retain(|p| !p.is_empty());
    parts
}

/// `<'a, T: Render = Foo, const N: usize>` gives the impl list without defaults
/// (`<'a, T: Render, const N: usize>`) and the type's arguments (`<'a, T, N>`).
pub(crate) fn split_generics(generics: &[TokenTree]) -> (TokenStream, TokenStream) {
    if generics.len() < 2 {
        return (TokenStream::new(), TokenStream::new());
    }
    let inner: TokenStream = generics[1..generics.len() - 1].iter().cloned().collect();
    let mut impl_params = Vec::new();
    let mut type_params = Vec::new();
    for param in split(inner, ',') {
        // A default is the `=` outside any angle brackets: `T: Iterator<Item = u8>` has none.
        // The `>` of an arrow (`Fn() -> u8`) closes nothing.
        let (mut depth, mut arrow) = (0i32, false);
        let default = param.iter().position(|t| {
            match t {
                TokenTree::Punct(p) if p.as_char() == '<' => depth += 1,
                TokenTree::Punct(p) if p.as_char() == '>' && !arrow => depth -= 1,
                _ => {}
            }
            arrow = matches!(t, TokenTree::Punct(p) if p.as_char() == '-' && p.spacing() == Spacing::Joint);
            depth == 0 && is_punct(Some(t), '=')
        });
        let without_default: Vec<TokenTree> = param[..default.unwrap_or(param.len())].to_vec();
        let name: Vec<TokenTree> = match param.first() {
            Some(TokenTree::Punct(p)) if p.as_char() == '\'' => param[..2].to_vec(),
            Some(TokenTree::Ident(id)) if id.to_string() == "const" => param[1..2].to_vec(),
            _ => param[..1].to_vec(),
        };
        impl_params.push(stream(without_default));
        type_params.push(stream(name));
    }
    let join = |parts: Vec<TokenStream>| {
        let mut out = q("<", &[]);
        for (i, part) in parts.into_iter().enumerate() {
            if i > 0 {
                out.extend(q(",", &[]));
            }
            out.extend(part);
        }
        out.extend(q(">", &[]));
        out
    };
    (join(impl_params), join(type_params))
}
