use corten::prelude::*;

use crate::*;

#[derive(Route)]
#[route("/items/{id}")]
struct Item<'a> {
    id: &'a str,
    tab: Option<&'a str>,
}

#[test]
fn every_island_has_a_custom_element_tag_and_its_script() {
    let tags: Vec<_> = COMPONENTS.iter().map(|c| c.island.expect("an island").tag).collect();
    assert_eq!(tags, ["number-field", "combo-box", "tab-group", "menu-button"]);
    for c in COMPONENTS {
        let island = c.island.unwrap();
        assert!(c.style.is_none(), "{} ships no CSS", c.name);
        assert!(island.script.contains(&format!("island(\"{}\"", island.tag)), "{} defines its own tag", island.file);
        for action in island.actions {
            assert!(island.script.contains(&format!("{}(", action.name)), "{} has the action {}", island.file, action.name);
        }
    }
}

#[test]
fn the_combo_box_value_is_a_native_select_and_the_input_waits_for_the_script() {
    let options = [Choice { value: "a", label: "Alpha" }, Choice { value: "b", label: "Beta <b>" }];
    let field = Picker { required: true, ..Picker::new("pick", "pick", "Choose", "None", &options) };
    let html = ComboBox { field: &field }.to_html();
    assert!(html.contains(r#"<select id="pick" name="pick" required data-ref="select" data-on="invalid:invalid"><option value="">Choose</option>"#), "{html}");
    assert!(html.contains(r#"<option value="b">Beta &lt;b&gt;</option>"#), "labels are escaped: {html}");
    assert!(html.contains(r#" hidden data-ref="input""#), "the text input shows only with the script: {html}");
    assert!(!html.contains(r#"value="Alpha""#), "no value, no text");
}

#[test]
fn tabs_without_links_are_buttons_and_hide_the_other_panels() {
    let tabs = [
        Tab { label: &"One", panel: &"first", link: None },
        Tab { label: &"Two", panel: &"second", link: Some(&Item { id: "x", tab: Some("two") }) },
    ];
    let html = TabGroup { id: "t", label: "Parts", tabs: &tabs, selected: 0 }.to_html();
    assert!(
        html.contains(r#"<button type="button" role="tab" id="t-tab-0" aria-controls="t-panel-0" aria-selected="true""#),
        "{html}"
    );
    assert!(html.contains(r#"<a role="tab" id="t-tab-1" href="/items/x?tab=two""#), "{html}");
    assert!(html.contains(r#"aria-labelledby="t-tab-0" tabindex="0" data-ref="panel">first</div>"#), "{html}");
    assert!(html.contains(r#"aria-labelledby="t-tab-1" tabindex="0" hidden data-ref="panel">second</div>"#), "{html}");
    assert!(!html.contains("tabindex=\"-1\""), "every tab is reachable before the script runs");
}

#[test]
fn a_number_field_disables_the_button_at_each_bound() {
    let at_max = Number { value: Some(5.0), min: Some(0.0), max: Some(5.0), ..Number::new("n", "n", "Less", "More") };
    let html = NumberField { field: &at_max }.to_html();
    assert!(html.contains(r#"aria-label="Less" tabindex="-1" data-ref="decrease""#), "{html}");
    assert!(html.contains(r#"aria-label="More" tabindex="-1" disabled data-ref="increase""#), "{html}");
    let off = Number { disabled: true, ..Number::new("n", "n", "Less", "More") };
    assert_eq!(NumberField { field: &off }.to_html().matches(" disabled").count(), 3, "both buttons and the input");
}
