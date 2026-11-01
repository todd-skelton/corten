// corten ui {{version}}
//! The UI kit: the theme tokens (`tokens/ui.resolver.json`, light and dark) and the components
//! `corten ui add` copied into `src/ui/`. They are the app's to change; `corten ui list` names
//! the others.

use corten::Component;

corten::style::tokens!("tokens/ui.resolver.json");

/// The tokens, then each component with a stylesheet: `App::components(ui::COMPONENTS)`.
pub const COMPONENTS: &[Component] = &[TOKENS];
