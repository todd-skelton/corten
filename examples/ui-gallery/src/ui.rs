//! The UI kit, from the kit's own files (`crates/corten-cli/kit`, what `corten ui add` copies into an
//! app), so the browser test covers each component as an app gets it.
//! Each is `rustfmt::skip`: the kit is laid out at rustfmt's default width, as an app formats it,
//! not this repository's.

use corten::Component;

#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/accordion.rs"]
pub mod accordion;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/alert.rs"]
pub mod alert;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/avatar.rs"]
pub mod avatar;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/badge.rs"]
pub mod badge;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/breadcrumb.rs"]
pub mod breadcrumb;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/button.rs"]
pub mod button;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/card.rs"]
pub mod card;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/checkbox.rs"]
pub mod checkbox;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/combobox.rs"]
pub mod combobox;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/dialog.rs"]
pub mod dialog;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/dropdown_menu.rs"]
pub mod dropdown_menu;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/field.rs"]
pub mod field;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/input.rs"]
pub mod input;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/pagination.rs"]
pub mod pagination;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/popover.rs"]
pub mod popover;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/radio_group.rs"]
pub mod radio_group;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/select.rs"]
pub mod select;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/separator.rs"]
pub mod separator;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/sheet.rs"]
pub mod sheet;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/skeleton.rs"]
pub mod skeleton;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/slider.rs"]
pub mod slider;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/switch.rs"]
pub mod switch;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/table.rs"]
pub mod table;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/tabs.rs"]
pub mod tabs;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/textarea.rs"]
pub mod textarea;
#[rustfmt::skip]
#[path = "../../../crates/corten-cli/kit/toasts.rs"]
pub mod toasts;

corten::style::tokens!("../../crates/corten-cli/kit/tokens/ui.resolver.json");

/// The tokens, then each component with a stylesheet, as `corten ui add` lists them.
pub const COMPONENTS: &[Component] = &[
    TOKENS,
    accordion::Accordion,
    alert::Alert,
    avatar::Avatar,
    badge::Badge,
    breadcrumb::Breadcrumb,
    button::Button,
    card::Card,
    checkbox::Checkbox,
    combobox::Combobox,
    dialog::Dialog,
    dropdown_menu::DropdownMenu,
    field::Field,
    input::Input,
    pagination::Pagination,
    popover::Popover,
    radio_group::RadioGroup,
    select::Select,
    separator::Separator,
    sheet::Sheet,
    skeleton::Skeleton,
    slider::Slider,
    switch::Switch,
    table::Table,
    tabs::Tabs,
    textarea::Textarea,
    toasts::Toasts,
];

#[cfg(test)]
mod tests {
    /// Every component the kit has is here, so the browser test covers it.
    #[test]
    fn the_gallery_has_every_kit_component() {
        let kit = concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates/corten-cli/kit");
        let here = include_str!("ui.rs");
        for entry in std::fs::read_dir(kit).unwrap() {
            let name = entry.unwrap().file_name().into_string().unwrap();
            if name.ends_with(".rs") && name != "ui.rs" {
                let module = name.trim_end_matches(".rs");
                assert!(here.contains(&format!("kit/{name}\"]\npub mod {module};")), "src/ui.rs lacks the kit's {name}");
            }
        }
    }

    /// An alert's role follows its variant, so a screen reader interrupts only for what needs the
    /// user now.
    #[test]
    fn an_alerts_role_follows_its_variant() {
        use super::alert::{Alert, AlertVariant};
        use corten::view::Render;
        for (variant, role) in [
            (AlertVariant::Neutral, None),
            (AlertVariant::Info, None),
            (AlertVariant::Success, Some("status")),
            (AlertVariant::Warning, Some("alert")),
            (AlertVariant::Destructive, Some("alert")),
        ] {
            let html = Alert { variant, children: "" }.to_html();
            match role {
                Some(role) => assert!(html.contains(&format!(r#" role="{role}""#)), "{variant:?}: {html}"),
                None => assert!(!html.contains("role="), "{variant:?}: {html}"),
            }
        }
    }
}
