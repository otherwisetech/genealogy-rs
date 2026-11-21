//! The window's side of otherwise-link: links from the OS, the Today feed and registry push
//! after a tree is loaded or saved, the two switches in the preferences menu, and "Track a
//! document in Vigía". The logic lives in `otherwise.rs`.
use crate::app::GenApp;
use crate::otherwise::{self, Action, SyncState};
use crate::settings::Settings;
use gpui::*;
use otherwise_link::Root;
use otherwise_link::today;
use std::time::Duration;

/// The Today feed is rebuilt this often (the day moves on even if the tree doesn't).
const REFRESH: Duration = Duration::from_secs(60 * 60);

impl GenApp {
    /// Link pump (every 300 ms), hourly Today refresh, and which apps are installed.
    pub(crate) fn start_shared(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(300)).await;
                let actions = otherwise::take();
                if actions.is_empty() {
                    continue;
                }
                let alive = this.update_in(cx, |this, window, cx| {
                    for a in actions {
                        this.handle_link(a, window, cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(REFRESH).await;
                if this.update(cx, |this, cx| this.publish_shared(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        let task = cx.background_executor().spawn(async { otherwise_link::apps::discover(&otherwise_link::apps::Locations::system()).iter().any(|(s, _)| s.id == "vigia") });
        cx.spawn(async move |this, cx| {
            let vigia = task.await;
            this.update(cx, |this, cx| {
                this.vigia_installed = vigia;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn handle_link(&mut self, a: Action, window: &mut Window, cx: &mut Context<Self>) {
        window.activate_window();
        cx.activate(true);
        match a {
            Action::Focus => {}
            Action::Open(p) => self.open_path(p, window, cx),
            Action::Person { file, id } => {
                let same = |a: &std::path::Path, b: &std::path::Path| std::fs::canonicalize(a).ok().zip(std::fs::canonicalize(b).ok()).is_some_and(|(x, y)| x == y) || a == b;
                match file {
                    Some(f) if !self.path.as_deref().is_some_and(|p| same(p, &f)) => {
                        self.pending_person = Some(id);
                        self.open_path(f, window, cx);
                    }
                    _ => self.focus_person(id, window, cx),
                }
            }
            Action::Entity(uuid) => {
                let tree = (*self.tree).clone();
                let key = self.path.as_deref().map(otherwise::tree_key);
                let task = cx.background_executor().spawn(async move {
                    let reg = otherwise::open_shared(&Root::system()).ok().flatten()?;
                    otherwise::person_for_entity(&reg, uuid, &tree, &key?)
                });
                cx.spawn_in(window, async move |this, cx| {
                    let found = task.await;
                    this.update_in(cx, |this, window, cx| match found {
                        Some(id) => this.focus_person(id, window, cx),
                        None => this.toast("app.toast.entity_not_here", "", cx),
                    })
                    .ok();
                })
                .detach();
            }
        }
    }

    fn focus_person(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.tree.person(&id).is_some() {
            self.set_focus(id, true, window, cx);
        } else {
            self.toast("app.toast.person_not_found", id, cx);
        }
    }

    /// A tree finished loading: a person a link asked for, then Today / registry.
    pub(crate) fn after_load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.pending_person.take()
            && self.tree.person(&id).is_some()
        {
            self.set_focus(id, true, window, cx);
        }
        self.publish_shared(cx);
    }

    /// Rewrites our Today feed and, when sharing is on, pushes changed people. Uses the tree as
    /// it is in memory; called after load and save, so it reflects the file.
    pub(crate) fn publish_shared(&mut self, cx: &mut Context<Self>) {
        let (share, today_on) = (self.share_people, self.today_on);
        let tree = (*self.tree).clone();
        let path = self.path.clone();
        cx.background_executor()
            .spawn(async move {
                let root = Root::system();
                let Some(file) = otherwise::state_file() else { return };
                let mut state = SyncState::load(&file);
                if let (true, Some(p)) = (share, path.as_deref()) {
                    let key = otherwise::tree_key(p);
                    match otherwise::open_shared(&root) {
                        Ok(Some(mut reg)) => match otherwise::push_tree(&mut reg, &tree, &key, &mut state) {
                            Ok(n) if n > 0 => {
                                let _ = state.save(&file);
                            }
                            Ok(_) => {}
                            Err(e) => eprintln!("genealogy-rs: {e}"), // i18n-ok: log only
                        },
                        Ok(None) => {}
                        Err(e) => eprintln!("genealogy-rs: {e}"), // i18n-ok: log only
                    }
                }
                let _ = match (today_on, path.as_deref()) {
                    (true, Some(p)) => today::publish(&root, otherwise::APP_ID, &otherwise::upcoming(&tree, Some(p), &otherwise::tree_key(p), chrono::Local::now().date_naive(), &state)),
                    (true, None) => Ok(()),
                    (false, _) => today::clear(&root, otherwise::APP_ID),
                };
            })
            .detach();
    }

    pub(crate) fn set_share_people(&mut self, on: bool, cx: &mut Context<Self>) {
        self.share_people = on;
        let mut s = Settings::load();
        s.share_people = on;
        s.save();
        if on {
            let enabled = otherwise_link::settings::HubSettings::load(&Root::system()).entities;
            self.toast(if enabled { "app.toast.sharing_on" } else { "app.toast.registry_off" }, "", cx);
        }
        self.publish_shared(cx);
        cx.notify();
    }

    pub(crate) fn set_today(&mut self, on: bool, cx: &mut Context<Self>) {
        self.today_on = on;
        let mut s = Settings::load();
        s.today = on;
        s.save();
        self.publish_shared(cx);
        cx.notify();
    }

    /// Shares this one person (an explicit action, so even with sharing off) and opens Vigía's
    /// new-item form with them as the owner.
    pub(crate) fn track_in_vigia(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(p) = self.tree.person(&id).cloned() else { return };
        // the shared card's ref is tied to the file
        let Some(key) = self.path.as_deref().map(otherwise::tree_key) else {
            self.toast("app.toast.vigia_save_first", "", cx);
            return;
        };
        let tree = (*self.tree).clone();
        let task = cx.background_executor().spawn(async move {
            let mut reg = match otherwise::open_shared(&Root::system()) {
                Ok(Some(r)) => r,
                Ok(None) => return Err("app.toast.registry_off"),
                Err(_) => return Err("app.toast.vigia_failed"),
            };
            let file = otherwise::state_file();
            let mut state = file.as_deref().map(SyncState::load).unwrap_or_default();
            let entity = otherwise::push_person(&mut reg, &tree, &key, &p, &mut state).ok().flatten().ok_or("app.toast.vigia_failed")?;
            if let Some(f) = &file {
                let _ = state.save(f);
            }
            otherwise_link::open::open_link(&otherwise::vigia_link(entity)).map_err(|_| "app.toast.vigia_failed")?;
            Ok(())
        });
        cx.spawn(async move |this, cx| {
            if let Err(key) = task.await {
                this.update(cx, |this, cx| this.toast(key, "", cx)).ok();
            }
        })
        .detach();
    }
}
