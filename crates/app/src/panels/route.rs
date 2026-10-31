//! Where panels reach each other, as no panel knows another.
//!
//! What an open file last had clicked goes to the unlocked inspector, or a
//! new one when every inspector is locked. At most one is unlocked, so
//! unlocking one closes the unlocked one before it.

use crate::workspace::Workspace;

use super::inspector::Page;
use super::{Inspector, Panel, PanelKind};

/// Once a frame, after the panels draw: settles a takeover, then shows what
/// was clicked.
pub fn update(workspace: &mut Workspace) {
    take_over(workspace);
    let page = workspace.panels_mut().find_map(|panel| match panel {
        Panel::OpenFile(file) => file.take_inspected(),
        _ => None,
    });
    if let Some(page) = page {
        inspect(workspace, page);
    }
}

/// Brings the unlocked inspector to the front. Returns whether there is one.
pub fn show_unlocked(workspace: &mut Workspace) -> bool {
    workspace.show(is_unlocked)
}

/// Shows `page` in the unlocked inspector, or a new one when every
/// inspector is locked.
fn inspect(workspace: &mut Workspace, page: Page) {
    let unlocked = workspace.panels_mut().find_map(|panel| match panel {
        Panel::Inspector(inspector) if !inspector.is_locked() => Some(inspector),
        _ => None,
    });
    match unlocked {
        Some(inspector) => {
            inspector.show(page);
            show_unlocked(workspace);
        }
        None => workspace.add(
            PanelKind::Inspector.place(),
            Panel::Inspector(Inspector::new(Some(page))),
        ),
    }
}

/// Closes the unlocked inspector an inspector just unlocked takes over
/// from.
fn take_over(workspace: &mut Workspace) {
    let taking_over = workspace
        .panels_mut()
        .any(|panel| matches!(panel, Panel::Inspector(inspector) if inspector.is_taking_over()));
    if taking_over {
        workspace.close(|panel| match panel {
            Panel::Inspector(inspector) => inspector.replaced(),
            _ => false,
        });
    }
}

fn is_unlocked(panel: &Panel) -> bool {
    matches!(panel, Panel::Inspector(inspector) if !inspector.is_locked())
}
