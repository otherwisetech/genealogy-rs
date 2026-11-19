//! Small styled building blocks shared by the top bar, canvas and ficha.
use crate::theme::{SANS, Tokens, initials};
use gedcom::Person;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::menu::DropdownMenu;
use gpui_component::{Icon, IconName, Selectable};

/// Round avatar with initials tinted by sex.
pub fn avatar(t: &Tokens, p: Option<&Person>, size: f32) -> Div {
    let (bg, fg) = t.sex(p.map(|p| p.sex).unwrap_or_default());
    let text = p.map(|p| initials(&p.given, &p.surname)).unwrap_or_else(|| "?".into());
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(size))
        .rounded_full()
        .bg(bg)
        .text_color(fg)
        .text_size(px((size * 0.36).max(6.)))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text)
}

pub fn icon(name: IconName, size: f32, color: Hsla) -> Icon {
    Icon::new(name).size(px(size)).text_color(color)
}

/// Outlined 36 px control.
pub fn outline_button(t: &Tokens, id: impl Into<ElementId>) -> Stateful<Div> {
    let hover = t.border_strong;
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .h(px(36.))
        .px(px(12.))
        .rounded(px(10.))
        .border_1()
        .border_color(t.border)
        .bg(t.surface)
        .text_color(t.ink)
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(move |s| s.border_color(hover))
}

/// `outline_button` that opens a gpui-component dropdown menu (`.dropdown_menu(…)`), so menu
/// triggers match the app's own buttons instead of the library's sizing. Darker border while open.
pub struct MenuTrigger {
    base: Stateful<Div>,
    open: bool,
    open_border: Hsla,
}

pub fn menu_trigger(t: &Tokens, id: impl Into<ElementId>) -> MenuTrigger {
    MenuTrigger { base: outline_button(t, id), open: false, open_border: t.border_strong }
}

/// Outline menu trigger with a label and a small chevron.
pub fn menu_button(t: &Tokens, id: impl Into<ElementId>, label: impl Into<SharedString>) -> MenuTrigger {
    menu_trigger(t, id).child(div().child(label.into())).child(icon(IconName::ChevronDown, 14., t.ink_muted))
}

/// Square outline menu trigger with only an icon.
pub fn menu_icon_button(t: &Tokens, id: impl Into<ElementId>, name: IconName) -> MenuTrigger {
    let mut m = menu_trigger(t, id).child(icon(name, 16., t.ink));
    m.base = m.base.w(px(36.)).px_0().justify_center();
    m
}

impl Styled for MenuTrigger {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl InteractiveElement for MenuTrigger {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl ParentElement for MenuTrigger {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.base.extend(elements)
    }
}

impl Selectable for MenuTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.open = selected;
        self
    }
    fn is_selected(&self) -> bool {
        self.open
    }
}

impl IntoElement for MenuTrigger {
    type Element = Stateful<Div>;
    fn into_element(self) -> Self::Element {
        let border = self.open_border;
        self.base.when(self.open, |d| d.border_color(border))
    }
}

impl DropdownMenu for MenuTrigger {}

/// Filled 36 px control (accent or ink).
pub fn filled_button(t: &Tokens, id: impl Into<ElementId>, bg: Hsla) -> Stateful<Div> {
    let hover = bg.opacity(0.88);
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .h(px(36.))
        .px(px(14.))
        .rounded(px(10.))
        .bg(bg)
        .text_color(t.on_fill())
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
}

/// Square icon-only 36 px control.
pub fn icon_button(t: &Tokens, id: impl Into<ElementId>, name: IconName, enabled: bool) -> Stateful<Div> {
    let hover = t.canvas;
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(36.))
        .rounded(px(10.))
        .child(icon(name, 16., if enabled { t.ink } else { t.ink_muted.opacity(0.5) }))
        .when(enabled, |d| d.cursor_pointer().hover(move |s| s.bg(hover)))
}

/// Small rounded chip.
pub fn chip(bg: Hsla, fg: Hsla, text: impl Into<SharedString>) -> Div {
    div().flex_none().px(px(10.)).py(px(3.)).rounded_full().bg(bg).text_color(fg).text_size(px(12.)).font_weight(FontWeight::MEDIUM).child(text.into())
}

/// Section title in the ficha.
pub fn section_title(t: &Tokens, text: impl Into<SharedString>) -> Div {
    div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(t.ink_muted).font_family(SANS).child(text.into())
}

/// Floating pill container used for canvas overlays.
pub fn pill(t: &Tokens) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(4.))
        .h(px(36.))
        .px(px(6.))
        .rounded_full()
        .bg(t.surface)
        .border_1()
        .border_color(t.border)
        .shadow(vec![BoxShadow { color: hsla(0., 0., 0., 0.06), offset: point(px(0.), px(2.)), blur_radius: px(8.), spread_radius: px(0.), inset: false }])
}

/// "1790 – 1852", "n. 1901"… or «Sin fechas».
pub fn years(p: &Person) -> String {
    let s = p.lifespan();
    if s.is_empty() { i18n::t!("app.person.no_dates") } else { s }
}
