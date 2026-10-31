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
        let panel: Box<dyn Panel> = match event {
            Event::Pick(path) => Box::new(OpenFile::new(path)),
            Event::Inspect(page) => Box::new(Inspector::new(Some(page))),
            Event::Add(desc) => match (desc.open)(None) {
                Some(panel) => panel,
                None => continue,
            },
            Event::Unlocked | Event::Read(_) => continue,
        };
        workspace.add(panel.desc().opens_at, panel);
    }
}
