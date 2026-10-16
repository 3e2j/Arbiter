//! Basic components with no data model, drawn from the current theme.
//!
//! Each is a function that declares its boxes into the open one and returns
//! what the user did, plus a state struct for the ones that remember
//! something, which the caller owns.
//!
//! Each takes `#[track_caller]`, so two calls from different lines are two
//! boxes. In a list that can insert or reorder, call them inside
//! [`Ui::keyed`](gui::ui::Ui::keyed).

mod controls;
mod layout;
mod overlay;
mod title_bar;

pub use controls::*;
pub use layout::*;
pub use overlay::*;
pub use title_bar::title_bar;

#[cfg(test)]
mod tests;
