//! The family's tokens come from otherwise-ui (github.com/otherwisetech/otherwise-ui, by tag):
//! Inter, the type scale, the warm neutrals and status colours, with genealogy's forest-green
//! accent (`otherwise_ui::accents::GENEALOGY`, set in main.rs). `Tokens` adds only the tree's
//! own content colours (connectors, the dotted canvas, the sex tints); everything else derefs
//! to `otherwise_ui::Theme`, so `&Tokens` passes wherever the crate wants `&Theme`.
use gedcom::Sex;
use gpui::{App, Hsla, Pixels, Window, px};
use otherwise_ui::theme::{hex as c, hex_a as tint};
pub use otherwise_ui::tokens::SANS;

#[derive(Debug, Clone, Copy)]
pub struct Tokens {
    pub ui: otherwise_ui::Theme,
    pub connector: Hsla,
    pub slot_dashed: Hsla,
    pub highlight: Hsla,
    pub canvas_dots: Hsla,
    pub male_bg: Hsla,
    pub male_fg: Hsla,
    pub female_bg: Hsla,
    pub female_fg: Hsla,
    pub unknown_bg: Hsla,
    pub unknown_fg: Hsla,
}

impl std::ops::Deref for Tokens {
    type Target = otherwise_ui::Theme;
    fn deref(&self) -> &otherwise_ui::Theme {
        &self.ui
    }
}

impl Tokens {
    pub fn new(ui: otherwise_ui::Theme) -> Self {
        if ui.dark {
            Tokens {
                ui,
                connector: c(0x6B675F),
                slot_dashed: c(0x4A4741),
                highlight: tint(0xF8E7A8, 0.35),
                canvas_dots: c(0x2A2826),
                male_bg: tint(0xDCE8F7, 0.2),
                male_fg: c(0xA9C6EE),
                female_bg: tint(0xF7E1E8, 0.2),
                female_fg: c(0xEBA9C0),
                unknown_bg: tint(0xF3F1EC, 0.12),
                unknown_fg: c(0xCFCAC0),
            }
        } else {
            Tokens {
                ui,
                connector: c(0xA9A498),
                slot_dashed: c(0xC9C5BB),
                highlight: c(0xF8E7A8),
                canvas_dots: c(0xD6D2C8),
                male_bg: c(0xDCE8F7),
                male_fg: c(0x1E4C86),
                female_bg: c(0xF7E1E8),
                female_fg: c(0x8A2A4A),
                unknown_bg: c(0xF3F1EC),
                unknown_fg: c(0x4A4741),
            }
        }
    }

    /// (background, foreground) of an avatar / sex chip.
    pub fn sex(&self, sex: Sex) -> (Hsla, Hsla) {
        match sex {
            Sex::Male => (self.male_bg, self.male_fg),
            Sex::Female => (self.female_bg, self.female_fg),
            Sex::Unknown => (self.unknown_bg, self.unknown_fg),
        }
    }
}

pub fn tokens(cx: &App) -> Tokens {
    Tokens::new(otherwise_ui::theme(cx))
}

/// Follows the system's light / dark appearance (genealogy has no appearance setting) and sets
/// the family font on gpui-component.
pub fn apply(window: Option<&mut Window>, cx: &mut App) {
    otherwise_ui::apply(otherwise_ui::Appearance::System, window, cx);
}

/// typographic family of the bundled Fraunces 72pt instance (person names and dialog titles in
/// the content; the chrome is Inter)
pub const SERIF: &str = "Fraunces 72pt";

pub mod size {
    use super::*;
    use otherwise_ui::tokens::{radius, size as ui};
    pub const CARD_W: Pixels = px(220.);
    pub const CARD_H: Pixels = px(64.);
    pub const CARD_FOCUS_H: Pixels = px(88.);
    pub const CARD_RADIUS: Pixels = px(radius::CARD);
    pub const CARD_PAD: Pixels = px(12.);
    pub const GEN_STEP: f32 = 260.;
    pub const MIN_VGAP: f32 = 24.;
    pub const AVATAR_CARD: Pixels = px(36.);
    pub const AVATAR_SHEET: Pixels = px(80.);
    pub const AVATAR_ROW: Pixels = px(30.);
    pub const SHEET_W: Pixels = px(360.);
    pub const REVIEW_W: Pixels = px(380.);
    pub const SHEET_PAD: Pixels = px(24.);
    pub const CONTROL_H: Pixels = px(ui::CONTROL);
    pub const DOT_SPACING: f32 = 20.;
    pub const DIALOG_W: Pixels = px(560.);
    pub const PALETTE_W: Pixels = px(560.);
    pub const DIALOG_RADIUS: Pixels = px(radius::MODAL);
    pub const FIELD_H: Pixels = px(ui::CONTROL);
}

/// Initials for an avatar: first letter of the given name and of the surname, "?" if none.
pub fn initials(given: &str, surname: &str) -> String {
    let mut s: String = [given, surname].iter().filter_map(|w| w.trim().chars().next()).flat_map(char::to_uppercase).collect();
    if s.is_empty() {
        s.push('?');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::initials;

    #[test]
    fn initials_from_names() {
        assert_eq!(initials("Ramona", "Ordóñez Rubira"), "RO");
        assert_eq!(initials("", ""), "?");
        assert_eq!(initials("ángel", ""), "Á");
    }
}
