//! Where panels reach each other, as no panel knows another.
//!
//! Each event posted this pass is offered to every panel in turn. One no
//! panel shows opens a panel for it: an open file for a pick, an inspector
//! for a selection, and the kind asked for from the View menu.

use crate::workspace::Workspace;

use super::{Context, Event, Inspector, OpenFile, Panel};

/// Once a frame, after the panels draw: offers what `cx` was posted.
pub fn update(workspace: &mut Workspace, cx: Context) {
    for mut event in cx.posted {
        if workspace.offer(&mut event) {
            continue;
        }
        let panel = match event {
            Event::Pick(path) => Panel::OpenFile(OpenFile::new(path)),
            Event::Inspect(page) => Panel::Inspector(Inspector::new(Some(page))),
            Event::Add(kind) => match Panel::open(kind, None) {
                Some(panel) => panel,
                None => continue,
            },
            Event::Unlocked | Event::Read(_) => continue,
        };
        workspace.add(panel.kind().place(), panel);
    }
}
