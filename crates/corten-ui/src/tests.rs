use corten::prelude::*;

use crate::*;

#[derive(Route)]
#[route("/items/{id}")]
struct Item<'a> {
    id: &'a str,
    tab: Option<&'a str>,
}

/// The fields the tests' controls post.
#[derive(Fields)]
#[allow(dead_code)]
struct Form {
    pick: String,
    n: f64,
}

#[test]
fn every_component_has_its_script_and_no_css() {
    let names: Vec<_> = COMPONENTS.iter().map(|c| c.script.expect("a script").name).collect();
    assert_eq!(
        names,
        [
            "ui.number_field",
            "ui.combo_box",
            "ui.tab_group",
            "ui.menu_button",
            "ui.toaster",
            "ui.copy_button",
            "ui.checkbox_group",
            "ui.required_when",
            "ui.dialog"
        ]
    );
    for c in COMPONENTS {
        let script = c.script.unwrap();
        assert!(c.style.is_none(), "{} ships no CSS", c.name);
        // The dialog module is handlers markup calls, not a component that sets itself up.
        let first = if script.name == "ui.dialog" { "modal" } else { "connect" };
        assert!(script.handlers.contains(&first), "{} has {first}", script.file);
    }
}

#[test]
fn the_combo_box_value_is_a_native_select_and_the_input_waits_for_the_script() {
    let options = [Choice { value: "a", label: "Alpha" }, Choice { value: "b", label: "Beta <b>" }];
    let field = Picker { required: true, ..Picker::new("pick", Form::FIELDS.pick, "Choose", "None", &options) };
    let html = ComboBox { field: &field }.to_html();
    assert!(
        html.starts_with(
            r#"<div data-scope="ui-combo-box" data-on="connect:ui.combo_box.connect($c,@select,@input,@popup,@list,@empty)">"#
        ),
        "{html}"
    );
    assert!(
        html.contains(r#"<select data-ref="select" id="pick" name="pick" required data-on="invalid:ui.combo_box.invalid($e,$c)"><option value="">Choose</option>"#),
        "{html}"
    );
    assert!(html.contains(r#"<option value="b">Beta &lt;b&gt;</option>"#), "labels are escaped: {html}");
    // The runtime's parser is checked against this string (a module name with a dot).
    let vectors = include_str!("../../corten/src/script/wire.json");
    assert!(vectors.contains(r#""attr": "invalid:ui.combo_box.invalid($e,$c)""#), "wire.json lists the library case");
    assert!(
        html.contains(r#" hidden data-on="input:ui.combo_box.filter($c) keydown:ui.combo_box.key($e,$c)"#),
        "the text input shows only with the script: {html}"
    );
    assert!(!html.contains(r#"value="Alpha""#), "no value, no text");
}

#[test]
fn tabs_without_links_are_buttons_and_hide_the_other_panels() {
    let tabs = [
        Tab { label: "One", panel: &"first", link: None },
        Tab { label: "Two", panel: &"second", link: Some(&Item { id: "x", tab: Some("two") }) },
    ];
    let html = TabGroup { id: "t", label: "Parts", tabs: &tabs, selected: 0 }.to_html();
    assert!(
        html.contains(
            r#"<button data-ref="tab" type="button" role="tab" id="t-tab-0" aria-controls="t-panel-0" aria-selected="true""#
        ),
        "{html}"
    );
    assert!(html.contains(r#"<a data-ref="tab" role="tab" id="t-tab-1" href="/items/x?tab=two""#), "{html}");
    assert!(
        html.contains(r#"data-on="click:ui.tab_group.choose($e,$t,*tab,*panel) keydown:ui.tab_group.step($e,$t,*tab,*panel)""#),
        "{html}"
    );
    assert!(
        html.contains(
            r#"<div data-ref="panel" role="tabpanel" id="t-panel-0" aria-labelledby="t-tab-0" tabindex="0">first</div>"#
        ),
        "{html}"
    );
    assert!(html.contains(r#"aria-labelledby="t-tab-1" tabindex="0" hidden>second</div>"#), "{html}");
    assert!(!html.contains("tabindex=\"-1\""), "every tab is reachable before the script runs");
}

#[test]
fn a_number_field_disables_the_button_at_each_bound() {
    let at_max = Number { value: Some(5.0), min: Some(0.0), max: Some(5.0), ..Number::new("n", Form::FIELDS.n, "Less", "More") };
    let html = NumberField { field: &at_max }.to_html();
    assert!(html.contains(r#"aria-label="Less" tabindex="-1" data-on="pointerdown:ui.number_field.press($e,$t,@input,-1) click:ui.number_field.click(@input,-1)">"#), "{html}");
    assert!(
        html.contains(r#"aria-label="More" tabindex="-1" disabled data-on="pointerdown:ui.number_field.press($e,$t,@input,1)"#),
        "{html}"
    );
    let off = Number { disabled: true, ..Number::new("n", Form::FIELDS.n, "Less", "More") };
    assert_eq!(NumberField { field: &off }.to_html().matches(" disabled").count(), 3, "both buttons and the input");
}

#[test]
fn a_menu_button_lists_links_and_actions() {
    #[derive(Route)]
    #[route("/out")]
    struct Out;
    let items = [MenuItem { label: "Out", link: &Out, post: true }];
    let html = MenuButton { id: "m", label: "Menu", items: &items }.to_html();
    assert!(
        html.starts_with(r#"<div data-scope="ui-menu-button" data-on="connect:ui.menu_button.connect(@trigger,@menu,*item)">"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<button data-ref="item" role="menuitem" data-on="click:ui.menu_button.used(@menu)">Out</button>"#),
        "{html}"
    );
}
