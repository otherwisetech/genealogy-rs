//! Design tokens from docs/DESIGN.md. Read with `tokens(cx)`; follows the gpui-component theme
//! mode (light/dark), which itself follows the system appearance.
use gedcom::Sex;
use gpui::{App, Hsla, Pixels, px, rgb};
use gpui_component::ActiveTheme;

#[derive(Debug, Clone, Copy)]
pub struct Tokens {
    pub canvas: Hsla,
    pub surface: Hsla,
    pub border: Hsla,
    /// hover border for cards
    pub border_strong: Hsla,
    pub ink: Hsla,
    pub ink_muted: Hsla,
    pub accent: Hsla,
    pub accent_soft: Hsla,
    pub connector: Hsla,
    pub slot_dashed: Hsla,
    pub danger: Hsla,
    /// review warnings (amber)
    pub warning: Hsla,
    pub highlight: Hsla,
    pub canvas_dots: Hsla,
    pub male_bg: Hsla,
    pub male_fg: Hsla,
    pub female_bg: Hsla,
    pub female_fg: Hsla,
    pub unknown_bg: Hsla,
    pub unknown_fg: Hsla,
    pub dark: bool,
}

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

fn tint(hex: u32, alpha: f32) -> Hsla {
    let mut h = c(hex);
    h.a = alpha;
    h
}

impl Tokens {
    pub fn light() -> Self {
        Tokens {
            canvas: c(0xEFEDE8),
            surface: c(0xFFFFFF),
            border: c(0xE2DFD8),
            border_strong: c(0xC9C5BB),
            ink: c(0x1C1B19),
            ink_muted: c(0x6B675F),
            accent: c(0x2F6B4F),
            accent_soft: c(0xF4F8F5),
            connector: c(0xA9A498),
            slot_dashed: c(0xC9C5BB),
            danger: c(0xA3382A),
            warning: c(0xA86A12),
            highlight: c(0xF8E7A8),
            canvas_dots: c(0xD6D2C8),
            male_bg: c(0xDCE8F7),
            male_fg: c(0x1E4C86),
            female_bg: c(0xF7E1E8),
            female_fg: c(0x8A2A4A),
            unknown_bg: c(0xF3F1EC),
            unknown_fg: c(0x4A4741),
            dark: false,
        }
    }

    pub fn dark() -> Self {
        Tokens {
            canvas: c(0x141413),
            surface: c(0x1F1E1C),
            border: c(0x34322E),
            border_strong: c(0x4A4741),
            ink: c(0xF2F0EA),
            ink_muted: c(0xA8A39A),
            accent: c(0x4F9A75),
            accent_soft: tint(0x2F6B4F, 0.2),
            connector: c(0x6B675F),
            slot_dashed: c(0x4A4741),
            danger: c(0xD0604F),
            warning: c(0xD9A441),
            highlight: tint(0xF8E7A8, 0.35),
            canvas_dots: c(0x2A2826),
            male_bg: tint(0xDCE8F7, 0.2),
            male_fg: c(0xA9C6EE),
            female_bg: tint(0xF7E1E8, 0.2),
            female_fg: c(0xEBA9C0),
            unknown_bg: tint(0xF3F1EC, 0.12),
            unknown_fg: c(0xCFCAC0),
            dark: true,
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
    if cx.theme().mode.is_dark() { Tokens::dark() } else { Tokens::light() }
}

/// typographic family of the bundled Fraunces 72pt instance
pub const SERIF: &str = "Fraunces 72pt";
pub const SANS: &str = "Inter";

pub mod size {
    use super::*;
    pub const CARD_W: Pixels = px(220.);
    pub const CARD_H: Pixels = px(64.);
    pub const CARD_FOCUS_H: Pixels = px(88.);
    pub const CARD_RADIUS: Pixels = px(14.);
    pub const CARD_PAD: Pixels = px(12.);
    pub const GEN_STEP: f32 = 260.;
    pub const MIN_VGAP: f32 = 24.;
    pub const AVATAR_CARD: Pixels = px(36.);
    pub const AVATAR_SHEET: Pixels = px(80.);
    pub const AVATAR_ROW: Pixels = px(30.);
    pub const SHEET_W: Pixels = px(360.);
    pub const REVIEW_W: Pixels = px(380.);
    pub const SHEET_PAD: Pixels = px(24.);
    pub const TOPBAR_H: Pixels = px(56.);
    pub const CONTROL_H: Pixels = px(36.);
    pub const DOT_SPACING: f32 = 20.;
    pub const DIALOG_W: Pixels = px(560.);
    pub const PALETTE_W: Pixels = px(560.);
    pub const DIALOG_RADIUS: Pixels = px(16.);
    pub const FIELD_H: Pixels = px(32.);
}

impl Tokens {
    /// dimmed layer behind dialogs and the palette
    pub fn backdrop(&self) -> Hsla {
        let mut h = c(0x1C1B19);
        h.a = if self.dark { 0.55 } else { 0.32 };
        h
    }

    /// text on accent/ink filled controls
    pub fn on_fill(&self) -> Hsla {
        if self.dark { c(0x141413) } else { c(0xFFFFFF) }
    }
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
