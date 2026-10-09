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
mod row;
mod scroll;

pub use button::{button, icon_button};
pub use row::{Row, row};
pub use scroll::{ListScroll, Scroll, list, scroll};

#[cfg(test)]
mod tests;
