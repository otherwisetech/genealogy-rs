//! Edit-person dialog (docs/DESIGN.md, artboard 2). CONTRACT STUB — the dialogs agent implements.
//! The host app renders it as an overlay and reacts to its events; it never mutates the tree.
use gedcom::{PersonEdit, Tree};
use gpui::*;

pub enum EditDialogEvent {
    /// user pressed «Guardar cambios» / ⌘↵
    Save(PersonEdit),
    /// «Cancelar», ×, esc, or backdrop click
    Cancel,
    /// «Eliminar persona» confirmed (the dialog asks for the second click itself)
    Delete(String),
    /// «Ver GEDCOM»
    ViewGedcom(String),
}

pub struct EditDialog {
    edit: PersonEdit,
    focus: FocusHandle,
}

impl EventEmitter<EditDialogEvent> for EditDialog {}

impl EditDialog {
    /// `tree` is read for the current values, place suggestions and the header subtitle.
    pub fn new(tree: &Tree, person_id: &str, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        EditDialog { edit: tree.person_edit(person_id).unwrap_or_default(), focus: cx.focus_handle() }
    }

    /// Focus the first field; the host calls this right after opening.
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
    }
}

impl Render for EditDialog {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // full-window overlay (backdrop + centred modal) — the host just adds it as a child
        div().absolute().inset_0().track_focus(&self.focus).child(format!("edit {}", self.edit.id))
    }
}
