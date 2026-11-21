//! The open tree as a file: lock against a second computer, conflicted copies from sync
//! clients, recent trees (with «Locate…» when one moved), Move to… and Show in Finder. The
//! paths and the recent list are in `storage.rs`; the moves use otherwise-link's verified copies.
use crate::app::GenApp;
use crate::storage::{self, Recent};
use crate::theme::tokens;
use gpui::*;
use otherwise_link::lock::{self, Holder, Lock, Scope};
use std::path::{Path, PathBuf};

/// Something about the open file the user should see, shown under the top bar until dismissed.
#[derive(Debug, Clone, PartialEq)]
pub enum FileNotice {
    /// Fresh lock from another computer: read-only here until «Use here anyway».
    Elsewhere { host: String },
    /// Another window or process on this computer has it open.
    Here,
    /// Another computer took it over while it was open here.
    TakenOver { host: String },
    /// Sync clients left conflicted copies next to it.
    Conflicts { first: PathBuf, count: usize },
    /// A recent tree that isn't where it was.
    Missing { path: PathBuf },
}

pub(crate) fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn dir_name(p: &Path) -> String {
    p.parent().map(|d| d.display().to_string()).unwrap_or_default()
}

/// «tree.ged — ~/Documents», or the missing marker.
pub(crate) fn recent_label(p: &Path) -> String {
    if p.exists() { i18n::t!("app.file.recent_item", name = file_name(p), dir = dir_name(p)) } else { i18n::t!("app.file.recent_missing", name = file_name(p), dir = dir_name(p)) }
}

impl GenApp {
    /// After a tree was read from or written to `path`: lock it, look for conflicted copies,
    /// remember it.
    pub(crate) fn attach_file(&mut self, path: &Path, cx: &mut Context<Self>) {
        if self.lock.as_ref().is_none_or(|l| l.target() != path) {
            self.lock = None;
            self.read_only = false;
            self.file_notice = None;
            match Lock::acquire(path, crate::otherwise::APP_ID, Scope::Process) {
                Ok(l) => self.lock = Some(l),
                Err(Holder::Elsewhere(i)) => {
                    self.read_only = true;
                    self.file_notice = Some(FileNotice::Elsewhere { host: i.host });
                }
                Err(Holder::Here(_)) => {
                    self.read_only = true;
                    self.file_notice = Some(FileNotice::Here);
                }
                // couldn't write the lock (read-only folder): nothing to guard
                Err(_) => {}
            }
        }
        if self.file_notice.is_none() {
            let st = otherwise_link::storage::status(path);
            if let Some(first) = st.conflicts.first() {
                self.file_notice = Some(FileNotice::Conflicts { first: first.clone(), count: st.conflicts.len() });
            }
        }
        Recent::update(|r| r.push(path));
        cx.notify();
    }

    /// A new, unsaved tree: nothing to lock.
    pub(crate) fn detach_file(&mut self) {
        self.lock = None;
        self.read_only = false;
        self.file_notice = None;
    }

    /// Heartbeat of the open file's lock, every `lock::HEARTBEAT_EVERY`.
    pub(crate) fn start_lock_heartbeat(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(lock::HEARTBEAT_EVERY).await;
                let alive = this.update(cx, |this, cx| {
                    let Some(l) = this.lock.as_mut() else { return };
                    if let Err(h) = l.heartbeat() {
                        if let Some(l) = this.lock.take() {
                            l.forget();
                        }
                        this.read_only = true;
                        this.file_notice = Some(FileNotice::TakenOver { host: h.info().map(|i| i.host.clone()).unwrap_or_default() });
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn take_over(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else { return };
        match Lock::take_over(&path, crate::otherwise::APP_ID, Scope::Process) {
            Ok(l) => {
                self.lock = Some(l);
                self.read_only = false;
                self.file_notice = None;
            }
            Err(e) => self.toast("app.toast.save_failed", e.to_string(), cx),
        }
        cx.notify();
    }

    pub(crate) fn reveal_file(&mut self, cx: &mut Context<Self>) {
        match self.path.as_deref() {
            Some(p) if p.exists() => cx.reveal_path(p),
            Some(p) => {
                if let Some(d) = p.parent() {
                    cx.reveal_path(d);
                }
            }
            None => self.toast("app.toast.save_first", "", cx),
        }
    }

    /// Moves the open file (verified copy, then the original is deleted) and keeps working on it
    /// there. Unsaved changes stay unsaved.
    pub(crate) fn move_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(from) = self.path.clone() else { return self.toast("app.toast.save_first", "", cx) };
        if self.read_only {
            return self.toast("app.toast.read_only", "", cx);
        }
        let dir = storage::save_dir(None, Recent::load().last_dir().as_deref());
        let rx = cx.prompt_for_new_path(&dir, Some(&file_name(&from)));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(to))) = rx.await {
                this.update_in(cx, |this, window, cx| this.move_to(from, to, window, cx)).ok();
            }
        })
        .detach();
    }

    fn move_to(&mut self, from: PathBuf, to: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if to == from {
            return;
        }
        if to.exists() {
            return self.toast("app.toast.move_exists", file_name(&to), cx);
        }
        // the lock belongs to the old place
        self.lock = None;
        match otherwise_link::storage::move_any(&from, &to) {
            Ok(()) => {
                Recent::update(|r| r.remove(&from));
                self.path = Some(to.clone());
                self.attach_file(&to, cx);
                self.toast("app.toast.moved", to.display().to_string(), cx);
                self.update_title(window);
                self.publish_shared(cx);
            }
            Err(e) => {
                self.lock = Lock::acquire(&from, crate::otherwise::APP_ID, Scope::Process).ok();
                self.toast("app.toast.move_failed", e.to_string(), cx);
            }
        }
        cx.notify();
    }

    pub(crate) fn open_recent(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if path.exists() {
            self.open_path(path, window, cx);
        } else {
            self.file_notice = Some(FileNotice::Missing { path });
            cx.notify();
        }
    }

    fn locate(&mut self, old: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some(i18n::t!("app.file.locate").into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(p) = paths.into_iter().next()
            {
                this.update_in(cx, |this, window, cx| {
                    Recent::update(|r| r.remove(&old));
                    this.file_notice = None;
                    this.open_path(p, window, cx);
                })
                .ok();
            }
        })
        .detach();
    }

    fn dismiss_notice(&mut self, cx: &mut Context<Self>) {
        self.file_notice = None;
        cx.notify();
    }

    pub(crate) fn render_file_notice(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let notice = self.file_notice.clone()?;
        let t = tokens(cx);
        let top = if self.has_update_notice() { px(112.) } else { px(68.) };
        let (text, warn) = match &notice {
            FileNotice::Elsewhere { host } => (i18n::t!("app.file.locked_elsewhere", host = host), true),
            FileNotice::Here => (i18n::t!("app.file.open_here"), true),
            FileNotice::TakenOver { host } => (i18n::t!("app.file.taken_over", host = host), true),
            FileNotice::Conflicts { count, .. } => (i18n::tn!("app.file.conflicts", *count), true),
            FileNotice::Missing { path } => (i18n::t!("app.file.missing", name = file_name(path), dir = dir_name(path)), false),
        };
        let action = |id: &'static str, label: String| crate::ui::outline_button(&t, id).h(px(30.)).child(label);
        let buttons = match notice {
            FileNotice::Elsewhere { .. } | FileNotice::TakenOver { .. } => vec![action("file-use-here", i18n::t!("app.file.use_here")).on_click(cx.listener(|this, _, _, cx| this.take_over(cx))).into_any_element()],
            FileNotice::Here => vec![],
            FileNotice::Conflicts { first, .. } => vec![action("file-reveal-conflict", storage::reveal_label()).on_click(move |_, _, cx| cx.reveal_path(&first)).into_any_element()],
            FileNotice::Missing { path } => {
                let p2 = path.clone();
                vec![
                    action("file-locate", i18n::t!("app.file.locate")).on_click(cx.listener(move |this, _, window, cx| this.locate(path.clone(), window, cx))).into_any_element(),
                    action("file-forget", i18n::t!("app.file.forget"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            Recent::update(|r| r.remove(&p2));
                            this.dismiss_notice(cx);
                        }))
                        .into_any_element(),
                ]
            }
        };
        Some(
            div().absolute().top(top).left_0().right_0().flex().justify_center().child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .max_w(px(760.))
                    .pl(px(16.))
                    .pr(px(8.))
                    .py(px(6.))
                    .rounded(px(12.))
                    .bg(t.surface)
                    .border_1()
                    .border_color(if warn { t.accent } else { t.border })
                    .shadow_md()
                    .text_size(px(13.))
                    .child(div().flex_1().min_w_0().child(text))
                    .children(buttons)
                    .child(
                        crate::ui::icon_button(&t, "file-notice-dismiss", gpui_component::IconName::Close, true)
                            .tooltip(|w, cx| gpui_component::tooltip::Tooltip::new(i18n::t!("app.update.dismiss")).build(w, cx))
                            .on_click(cx.listener(|this, _, _, cx| this.dismiss_notice(cx))),
                    ),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::recent_label;
    use otherwise_link::lock::{self, Holder, Lock, LockInfo, Me, Scope};

    #[test]
    fn another_computer_keeps_the_tree_read_only() {
        let t = tempfile::tempdir().unwrap();
        let tree = t.path().join("family.ged");
        std::fs::write(&tree, b"0 HEAD\n0 TRLR\n").unwrap();
        let other = Me { host: "other-mac".into(), pid: 1 };
        let now = lock::now();
        let held = Lock::acquire_as(&tree, "genealogy", Scope::Process, other, now, |_| true).unwrap();
        let me = Me { host: "this-mac".into(), pid: 2 };
        match Lock::acquire_as(&tree, "genealogy", Scope::Process, me.clone(), now + 5, |_| true) {
            Err(Holder::Elsewhere(LockInfo { host, .. })) => assert_eq!(host, "other-mac"),
            r => panic!("{r:?}"),
        }
        // a stale lock (the other computer slept) is free to take
        assert!(Lock::acquire_as(&tree, "genealogy", Scope::Process, me, now + lock::STALE_AFTER.as_secs() as i64 + 5, |_| true).is_ok());
        held.forget();
    }

    #[test]
    fn move_keeps_the_tree_and_drops_the_original() {
        let t = tempfile::tempdir().unwrap();
        let (from, to) = (t.path().join("a.ged"), t.path().join("Dropbox").join("a.ged"));
        std::fs::write(&from, b"0 HEAD\n0 TRLR\n").unwrap();
        otherwise_link::storage::move_any(&from, &to).unwrap();
        assert!(!from.exists());
        assert_eq!(std::fs::read(&to).unwrap(), b"0 HEAD\n0 TRLR\n");
        // never over an existing file
        std::fs::write(&from, b"x").unwrap();
        assert!(otherwise_link::storage::move_any(&from, &to).is_err());
        assert!(from.exists());
    }

    #[test]
    fn recent_labels() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("tree.ged");
        let missing = recent_label(&p);
        std::fs::write(&p, b"").unwrap();
        let there = recent_label(&p);
        assert!(there.contains("tree.ged") && missing.contains("tree.ged") && there != missing);
    }
}
