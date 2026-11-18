//! ⌘K palette (docs/DESIGN.md, artboard 3). CONTRACT STUB — the dialogs agent implements.
use gedcom::Tree;
use gpui::*;
use std::rc::Rc;

pub enum PaletteEvent {
    /// ↵ on a person: make them the focus of the tree
    Focus(String),
    /// ⌘↵ on a person: open the edit dialog
    Edit(String),
    /// ↵ on a place: host may filter/highlight; at minimum focus the first person with an event there
    Place(String),
    /// «Crear persona «query»» / ⌘N — the raw query text, host splits it into names
    CreatePerson(String),
    /// esc or backdrop click
    Dismiss,
}

pub struct Palette {
    tree: Rc<Tree>,
    focus_person: Option<String>,
    focus: FocusHandle,
}

impl EventEmitter<PaletteEvent> for Palette {}

impl Palette {
    pub fn new(tree: Rc<Tree>, focus_person: Option<String>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        Palette { tree, focus_person, focus: cx.focus_handle() }
    }

    /// Focus the query input; the host calls this right after opening.
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
    }
}

impl Render for Palette {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let _ = (&self.tree, &self.focus_person);
        div().absolute().inset_0().track_focus(&self.focus).child("palette")
    }
}
