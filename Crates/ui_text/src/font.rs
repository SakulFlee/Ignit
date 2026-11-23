//! Font stack and font-system setup.

use cosmic_text::{Family, FontSystem};
use ignit_text::DefaultFont;
use serde::{Deserialize, Serialize};

/// Ordered family fallbacks used when a style does not name a family.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct FontStack {
    pub families: Vec<String>,
}

impl Default for FontStack {
    fn default() -> Self {
        Self {
            families: vec!["Inter".to_string()],
        }
    }
}

impl FontStack {
    pub fn new(families: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            families: families.into_iter().map(Into::into).collect(),
        }
    }

    pub fn primary(&self) -> Option<&str> {
        self.families.first().map(String::as_str)
    }
}

pub(crate) struct FontSetup {
    pub system: FontSystem,
    pub default_family: String,
}

pub(crate) fn build_font_system(fonts: &FontStack, system_fonts: bool) -> FontSetup {
    let mut system = FontSystem::new_with_fonts([]);
    system
        .db_mut()
        .load_font_data(DefaultFont::bytes().to_vec());
    if system_fonts {
        system.db_mut().load_system_fonts();
    }

    let wanted = fonts.primary();
    let family = system
        .db()
        .faces()
        .find(|face| {
            wanted.is_some_and(|wanted| {
                face.families
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case(wanted))
            })
        })
        .or_else(|| system.db().faces().next())
        .and_then(|face| face.families.first().map(|(name, _)| name.clone()))
        .unwrap_or_else(|| "Inter".to_string());
    system.db_mut().set_sans_serif_family(family.clone());
    FontSetup {
        system,
        default_family: family,
    }
}

pub(crate) fn family_for<'a>(stack: &'a FontStack, style: &'a ignit_ui::UiTextStyle) -> Family<'a> {
    style
        .family
        .as_deref()
        .or_else(|| stack.primary())
        .map_or(Family::SansSerif, Family::Name)
}
