//! Basic components with no data model, drawn from the
//! [`Theme`](crate::ui::Theme).
//!
//! Each is a function that declares its boxes into the open one and returns
//! what the user did, plus a state struct for the ones that remember
//! something, which the caller owns.
//!
//! Each takes `#[track_caller]`, so two calls from different lines are two
//! boxes. In a list that can insert or reorder, call them inside
//! [`Ui::keyed`](crate::ui::Ui::keyed).

mod button;
mod dropdown;
mod field;
mod menu;
mod notice;
mod popover;
mod row;
mod rule;
mod scroll;
mod slider;
mod toolbar;
mod window;

pub use button::{Clicks, IconState, button, icon_button};
pub use dropdown::{Choice, Chosen, Dropdown, dropdown};
pub use field::{TextField, text_field};
pub use menu::{ContextMenu, MenuItem, context_menu};
pub use notice::notice;
pub use popover::popover;
pub use row::row;
pub use rule::rule;
pub use scroll::{ListScroll, Scroll, ThumbShow, indexed_list, list, scroll};
pub use slider::{Slid, slider};
pub use toolbar::{spacer, toolbar};
pub use window::{title_bar, window_edges};

#[cfg(test)]
mod tests;
