//! Questions for whoever is at the terminal. With no one there, every answer is no.

use std::io::{self, IsTerminal, Write};

/// One of `options`, if someone at a terminal picks one.
pub fn choose(question: &str, options: &[&str]) -> Option<usize> {
    if !io::stdin().is_terminal() {
        return None;
    }
    eprintln!("{question}");
    for (n, option) in (1..).zip(options) {
        eprintln!("  {n}) {option}");
    }
    eprint!("[1-{}] ", options.len());
    let _ = io::stderr().flush();
    let mut answer = String::new();
    io::stdin().read_line(&mut answer).ok()?;
    let i = answer.trim().parse::<usize>().ok()?.checked_sub(1)?;
    (i < options.len()).then_some(i)
}

/// No, unless someone at a terminal answers yes.
pub fn confirm(question: &str) -> bool {
    if !io::stdin().is_terminal() {
        return false;
    }
    eprint!("{question} [y/N] ");
    let _ = io::stderr().flush();
    let mut answer = String::new();
    io::stdin().read_line(&mut answer).is_ok() && answer.trim().eq_ignore_ascii_case("y")
}
