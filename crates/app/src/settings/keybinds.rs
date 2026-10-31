//! The settings page for choosing which chords run each of the app's
//! [`commands`].
//!
//! Commands are listed under a heading per group, and can be searched by
//! name or chord. Each row shows:
//! - the command's name,
//! - its chords as key caps, wrapping at half the row,
//! - a `+` to add a chord, and a reset while it's changed.
//!
//! Clicking a cap records over it; `+` records a new one. While recording,
//! held keys show live and the chord is taken once they're all released.
//! Escape cancels before anything is pressed, and × removes the chord.

use std::collections::BTreeMap;

use crate::components::{IconState, TextField, icon_button, notice, rule, spacer, text_field};
use crate::keymap::{self, Chord, KeyContext, Keymap};
use gui::{
    input::{CursorIcon, Input, Key, Modifiers, PointerButton},
    ui::{Align, Border, Direction, Element, Length, Ui},
};

use serde::{Deserialize, Deserializer, Serialize};

use super::widgets::{DOT, DOT_GAP, dot, key_cap, key_cap_width, setting, sized};
use crate::assets::Icon;
use crate::commands::{self, Command};
use crate::theme;

/// How wide a cap is while recording, so it doesn't jump as keys go down.
const RECORDING: f32 = 140.;
/// Between a row's name, chords and buttons, and between chords on a line.
const GAP: f32 = 16.;
const OR_GAP: f32 = 6.;
/// A chord's room either side of its caps, and between them.
const PAD: f32 = 4.;
const CAP_GAP: f32 = 3.;

/// What's kept of the page in the settings file: the chords of each command
/// the user changed, by its id, as text.
#[derive(Serialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(transparent)]
pub struct Saved(BTreeMap<String, Vec<String>>);

pub struct Keybinds {
    /// Each command's chords, in [`commands::all`]'s order.
    chords: Vec<Vec<Chord>>,
    /// Whether `chords` changed since gui was handed a keymap of them.
    rebind: bool,
    /// Whether `chords` changed since the file was written.
    unsaved: bool,
    /// The cap recording last pass, so the keys pressed are its, by its row
    /// in [`commands::all`]'s order.
    live: Option<(usize, Cap)>,
    /// Set by a `+` until the cap it opened takes the keyboard.
    opening: bool,
    /// The chord the recording cap will take once the keys are let go.
    pending: Option<Chord>,
    search: TextField,
}

/// Which of a row's caps.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Cap {
    Chord(usize),
    /// The one being added after them.
    More,
}

/// What a row asked for.
#[derive(Clone, Copy)]
enum Change {
    Reset,
    /// Puts `chord` in place of the one at `slot`, or after them.
    Record {
        slot: Option<usize>,
        chord: Chord,
    },
    Drop(usize),
}

/// What a cap did this pass.
#[derive(Default)]
struct Recorded {
    chord: Option<Chord>,
    dropped: bool,
    recording: bool,
}

impl<'de> Deserialize<'de> for Saved {
    /// One command at a time, so one that's unreadable costs only itself.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let table = BTreeMap::<String, toml::Value>::deserialize(deserializer)?;
        let chords = table
            .into_iter()
            .filter_map(|(id, value)| {
                Vec::deserialize(value)
                    .inspect_err(|err| {
                        tracing::warn!("the keys for `{id}` aren't readable, so they're back to the defaults: {err}");
                    })
                    .ok()
                    .map(|chords| (id, chords))
            })
            .collect();
        Ok(Self(chords))
    }
}

impl Default for Keybinds {
    fn default() -> Self {
        Self::new(&Saved::default())
    }
}

impl Keybinds {
    pub fn new(saved: &Saved) -> Self {
        for id in saved.0.keys() {
            if !commands::all().any(|command| command.id.0 == id) {
                tracing::warn!("there's no command `{id}` any more, so its keys are dropped");
            }
        }
        let chords = commands::all()
            .map(|command| {
                let Some(texts) = saved.0.get(command.id.0) else {
                    return command.chords.to_vec();
                };
                texts
                    .iter()
                    .filter_map(|text| {
                        text.parse()
                            .inspect_err(|err| {
                                tracing::warn!("{} goes without it: {err}", command.id.0);
                            })
                            .ok()
                    })
                    .collect()
            })
            .collect();
        Self {
            chords,
            rebind: false,
            unsaved: false,
            live: None,
            opening: false,
            pending: None,
            search: TextField::default(),
        }
    }

    /// Only the commands whose chords aren't the defaults.
    pub fn saved(&self) -> Saved {
        let changed = commands::all()
            .zip(&self.chords)
            .filter(|&(command, chords)| command.chords != chords.as_slice())
            .map(|(command, chords)| {
                let texts = chords.iter().map(Chord::to_string).collect();
                (command.id.0.to_owned(), texts)
            });
        Saved(changed.collect())
    }

    pub fn keymap(&self) -> Keymap {
        commands::keymap(self.chords.iter().map(Vec::as_slice))
    }

    /// The keymap to hand gui, if a chord changed since the last call.
    pub fn take_keymap(&mut self) -> Option<Keymap> {
        std::mem::take(&mut self.rebind).then(|| self.keymap())
    }

    /// Whether a chord changed since the last call, so the file is behind.
    pub const fn take_unsaved(&mut self) -> bool {
        std::mem::replace(&mut self.unsaved, false)
    }

    /// Whether a cap is recording, so the keys pressed are its alone.
    pub const fn recording(&self) -> bool {
        self.live.is_some()
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let bar = Element {
            size: [Length::Grow, Length::Fit],
            padding: [0., 0., 0., 4.],
            ..Element::DEFAULT
        };
        let Self {
            chords: all,
            pending,
            search,
            live: was,
            opening,
            ..
        } = self;
        ui.element(bar, |ui| {
            let icon = Some(Icon::Search.into());
            text_field(ui, search, icon, "Search keybinds");
        });
        let query = search.text.trim().to_lowercase();
        let shown = |command: &Command, chords: &[Chord]| {
            query.is_empty()
                || command.name.to_lowercase().contains(&query)
                || chords
                    .iter()
                    .any(|chord| chord.to_string().contains(&query))
        };
        let mut change = None;
        let mut live = None;
        let mut added = None;
        let mut any = false;
        let mut rows = (0..).zip(all.iter());
        for &(group, commands) in &commands::GROUPS {
            let mut first = true;
            for (command, (at, chords)) in commands.iter().zip(&mut rows) {
                if !shown(command, chords) {
                    continue;
                }
                if first {
                    heading(ui, group);
                } else {
                    rule(ui);
                }
                first = false;
                any = true;
                let changed = command.chords != chords.as_slice();
                let clash = clash(all, command, chords);
                let desc = clash.map(|(name, _)| format!("Also {name}"));
                let here = was.filter(|&(row, _)| row == at).map(|(_, slot)| slot);
                setting(ui, |ui| {
                    let hot = ui.hovered();
                    let warned = clash.map(|(_, chord)| chord);
                    ui.element(line(), |ui| {
                        name(ui, command.name, desc.as_deref(), changed);
                        let (asked, held) = caps(ui, chords, pending, here, opening, warned);
                        let (add, reset) = actions(ui, hot, changed);
                        if let Some(slot) = held {
                            live = Some((at, slot));
                        }
                        if add {
                            added = Some(at);
                        } else if reset {
                            change = Some((at, Change::Reset));
                        } else if let Some(asked) = asked {
                            change = Some((at, asked));
                        }
                    });
                });
            }
        }
        if !any {
            notice(ui, "No keybinds match.");
        }
        // The cap a `+` opens takes the keyboard next pass, and one that
        // changed the chords gave it up.
        self.live = match (added, change) {
            (Some(at), _) => Some((at, Cap::More)),
            (None, Some(_)) => None,
            (None, None) => live,
        };
        self.opening = added.is_some() || self.opening && self.live.is_some();
        if let Some((at, change)) = change {
            self.apply(at, change);
        }
    }

    fn apply(&mut self, at: usize, change: Change) {
        let (Some(command), Some(chords)) = (commands::all().nth(at), self.chords.get_mut(at))
        else {
            return;
        };
        match change {
            Change::Reset => command.chords.clone_into(chords),
            Change::Record { slot, chord } if chords.contains(&chord) => {
                // Already one of them, so the one it was recorded over goes.
                if let Some(slot) = slot.filter(|&slot| chords.get(slot) != Some(&chord)) {
                    chords.remove(slot);
                }
            }
            Change::Record {
                slot: Some(slot),
                chord,
            } => {
                if let Some(old) = chords.get_mut(slot) {
                    *old = chord;
                }
            }
            Change::Record { slot: None, chord } => chords.push(chord),
            Change::Drop(slot) => {
                if slot < chords.len() {
                    chords.remove(slot);
                }
            }
        }
        self.rebind = true;
        self.unsaved = true;
    }
}

/// The first other command that shares one of `chords` with `command`
/// where both work: its name, and the chord.
fn clash(all: &[Vec<Chord>], command: &Command, chords: &[Chord]) -> Option<(&'static str, Chord)> {
    let global = KeyContext::GLOBAL;
    commands::all().zip(all).find_map(|(other, theirs)| {
        let near = other.context == command.context
            || other.context == global
            || command.context == global;
        if !near || std::ptr::eq(other, command) {
            return None;
        }
        let shared = theirs.iter().find(|chord| chords.contains(chord))?;
        Some((other.name, *shared))
    })
}

/// A group's name over its rows, lined up with their names.
fn heading(ui: &mut Ui, name: &str) {
    let theme = theme::current();
    let element = Element {
        size: [Length::Grow, Length::Fit],
        padding: [0., 18., 0., 2.],
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        ui.text(sized(&theme, -1., theme.color.accent), name);
    });
}

/// What the keys did while recording, as far as [`take`] has seen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Take {
    /// Still held, or nothing pressed yet.
    Waiting,
    /// Escape on its own, before any chord.
    Cancelled,
    /// Every key let go after a chord.
    Done(Chord),
}

/// Follows `input` while recording: the last chord pressed is `pending`,
/// and it's done once every key is up, so the modifiers can go down in any
/// order and a slip is fixed by pressing again before letting go.
fn take(pending: &mut Option<Chord>, input: &Input) -> Take {
    for press in input.keys().iter().filter(|press| !press.repeat) {
        let alone = press.modifiers == Modifiers::default();
        if press.key == Key::Escape && alone && pending.is_none() {
            return Take::Cancelled;
        }
        if let Some(chord) = Chord::pressed(*press) {
            *pending = Some(chord);
        }
    }
    let up = input.held_keys().next().is_none() && input.modifiers() == Modifiers::default();
    match *pending {
        Some(chord) if up => {
            *pending = None;
            Take::Done(chord)
        }
        _ => Take::Waiting,
    }
}

const fn line() -> Element {
    Element {
        direction: Direction::LeftToRight,
        size: [Length::Grow, Length::Fit],
        gap: GAP,
        align: [Align::Start, Align::Center],
        ..Element::DEFAULT
    }
}

/// A command's name after its changed dot, and what it clashes with under
/// it, cut off rather than pushing on its chords.
fn name(ui: &mut Ui, name: &str, clash: Option<&str>, changed: bool) {
    let theme = theme::current();
    let c = theme.color;
    let cell = Element {
        size: [Length::Grow, Length::Fit],
        gap: 1.,
        clip: true,
        ..Element::DEFAULT
    };
    let named = Element {
        direction: Direction::LeftToRight,
        gap: DOT_GAP,
        align: [Align::Start, Align::Center],
        ..Element::DEFAULT
    };
    let under = Element {
        padding: [DOT + DOT_GAP, 0., 0., 0.],
        ..Element::DEFAULT
    };
    ui.element(cell, |ui| {
        ui.element(named, |ui| {
            dot(ui, changed);
            ui.text(theme.ui_text(c.text), name);
        });
        if let Some(clash) = clash {
            ui.element(under, |ui| ui.text(sized(&theme, -1., c.warn), clash));
        }
    });
}

/// The `+` and the reset while `changed`, shown while the row is `hot`, and
/// holes their size otherwise so they stay put. Returns which was pressed.
fn actions(ui: &mut Ui, hot: bool, changed: bool) -> (bool, bool) {
    let row = theme::current().size.row;
    let hole = Element {
        size: [Length::Fixed(row); 2],
        ..Element::DEFAULT
    };
    let both = Element {
        direction: Direction::LeftToRight,
        align: [Align::Start, Align::Center],
        ..Element::DEFAULT
    };
    ui.element(both, |ui| {
        let add = if hot {
            icon_button(ui, Icon::Plus, IconState::Normal).pressed
        } else {
            ui.element(hole, |_| false)
        };
        let reset = if hot && changed {
            icon_button(ui, Icon::Reset, IconState::Normal).clicked
        } else {
            ui.element(hole, |_| false)
        };
        (add, reset)
    })
}

/// A row's chords in lines as wide as their half of it, with "or" between
/// those sharing one, then the one being added. `was` is the cap recording
/// last pass, if it's in this row. Returns what they asked for, and the cap
/// that's recording.
fn caps(
    ui: &mut Ui,
    chords: &[Chord],
    pending: &mut Option<Chord>,
    was: Option<Cap>,
    opening: &mut bool,
    warned: Option<Chord>,
) -> (Option<Change>, Option<Cap>) {
    let theme = theme::current();
    let c = theme.color;
    let cell = Element {
        size: [Length::Grow, Length::Fit],
        gap: PAD,
        clip: true,
        ..Element::DEFAULT
    };
    let strip = Element {
        direction: Direction::LeftToRight,
        gap: OR_GAP,
        align: [Align::Start, Align::Center],
        ..Element::DEFAULT
    };
    let adding = was == Some(Cap::More);
    ui.element(cell, |ui| {
        let room = ui.rect().map_or(f32::INFINITY, |rect| rect.w);
        let or = ui.measure(sized(&theme, -2., c.dim), "or")[0] + 2. * OR_GAP;
        let mut change = None;
        let mut recording = None;
        if chords.is_empty() && !adding {
            ui.text(sized(&theme, -1., c.dim), "Not bound");
        }
        let slots = (0..)
            .zip(chords)
            .map(|(slot, &chord)| (Some(slot), Some(chord)))
            .chain(adding.then_some((None, None)));
        let mut rest = slots.peekable();
        while rest.peek().is_some() {
            // A line takes one chord however narrow, so it can't stall.
            let mut used = 0.;
            let mut count = 0;
            for (slot, chord) in rest.clone() {
                let width = width(ui, chord, was == Some(cap(slot)));
                let need = if count == 0 { width } else { used + or + width };
                if count > 0 && need > room {
                    break;
                }
                used = need;
                count += 1;
            }
            ui.element(strip, |ui| {
                let mut first = true;
                for (slot, chord) in rest.by_ref().take(count) {
                    if !first {
                        ui.text(sized(&theme, -2., c.dim), "or");
                    }
                    first = false;
                    let warn = chord.is_some() && chord == warned;
                    let open = slot.is_none() && std::mem::take(opening);
                    let recorded = recorder(ui, chord, pending, warn, open);
                    if recorded.recording {
                        recording = Some(cap(slot));
                    }
                    if let Some(chord) = recorded.chord {
                        change = Some(Change::Record { slot, chord });
                    } else if let (true, Some(slot)) = (recorded.dropped, slot) {
                        change = Some(Change::Drop(slot));
                    }
                }
            });
        }
        (change, recording)
    })
}

const fn cap(slot: Option<usize>) -> Cap {
    match slot {
        Some(slot) => Cap::Chord(slot),
        None => Cap::More,
    }
}

/// How wide [`recorder`] makes `chord`, or its field while `live`.
fn width(ui: &mut Ui, chord: Option<Chord>, live: bool) -> f32 {
    let row = theme::current().size.row;
    let mut caps = 0.;
    let mut count = 0u16;
    for cap in chord.into_iter().flat_map(Chord::caps) {
        caps += key_cap_width(ui, &cap);
        count += 1;
    }
    let caps = caps + CAP_GAP * f32::from(count.saturating_sub(1));
    if live {
        // Its padding, and the × after a gap.
        RECORDING.max(caps + 2. * PAD + CAP_GAP + row)
    } else {
        (caps + 2. * PAD).max(row)
    }
}

/// `chord` in caps, or nothing for one more, that records a chord once
/// pressed or `open`: the keys show as they're held, and it's taken once
/// they're all let go.
#[track_caller]
fn recorder(
    ui: &mut Ui,
    chord: Option<Chord>,
    pending: &mut Option<Chord>,
    warn: bool,
    open: bool,
) -> Recorded {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let element = Element {
        direction: Direction::LeftToRight,
        size: [Length::Fit, Length::Fixed(s.row)],
        min: [s.row, 0.],
        padding: [PAD, 0., PAD, 0.],
        gap: CAP_GAP,
        align: [Align::Center; 2],
        radius: [s.radius; 4],
        cursor: Some(CursorIcon::Pointer),
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        let mut recorded = Recorded::default();
        if open || ui.pressed(PointerButton::Left) {
            ui.focus();
            *pending = None;
            recorded.recording = true;
        }
        if !ui.focused() {
            ui.style().background = ui.hovered().then_some(c.raised);
            let color = if warn { c.warn } else { c.text };
            if let Some(chord) = chord {
                chord.caps().for_each(|cap| key_cap(ui, &cap, color));
            }
            return recorded;
        }
        recorded.recording = true;
        match take(pending, ui.input()) {
            Take::Waiting => {}
            Take::Cancelled => ui.unfocus(),
            Take::Done(chord) => {
                recorded.chord = Some(chord);
                ui.unfocus();
            }
        }
        let style = ui.style();
        style.min[0] = RECORDING;
        style.padding = [8., 0., 2., 0.];
        style.align[0] = Align::Start;
        style.background = Some(c.sunken);
        style.border = Some(Border {
            color: c.accent,
            ..theme.border()
        });
        let held = ui.input().modifiers();
        if let Some(chord) = *pending {
            chord.caps().for_each(|cap| key_cap(ui, &cap, c.text));
        } else if held == Modifiers::default() {
            ui.text(sized(&theme, -1., c.dim), "Press keys…");
        } else {
            keymap::modifier_caps(held).for_each(|cap| key_cap(ui, cap, c.text));
        }
        spacer(ui);
        if chord.is_some() && icon_button(ui, Icon::Close, IconState::Normal).clicked {
            recorded.dropped = true;
            ui.unfocus();
        }
        recorded
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gui::input::Event;

    /// A pass of `events` after the last, as `take` sees it.
    fn pass(input: &mut Input, pending: &mut Option<Chord>, events: &[Event]) -> Take {
        input.clear();
        for &event in events {
            input.push(event);
        }
        input.trickle();
        take(pending, input)
    }

    fn down(key: Key) -> Event<'static> {
        Event::Key {
            key,
            code: None,
            repeat: false,
        }
    }

    fn up(key: Key) -> Event<'static> {
        Event::KeyReleased { key, code: None }
    }

    #[test]
    fn a_chord_is_taken_once_every_key_is_up() {
        let (ctrl, shift) = (Modifiers::CTRL, Modifiers::SHIFT);
        let mut input = Input::default();
        let mut pending = None;
        let mut pass = |events: &[Event]| pass(&mut input, &mut pending, events);
        assert_eq!(
            pass(&[Event::Modifiers(ctrl), down(Key::Ctrl)]),
            Take::Waiting
        );
        let both = ctrl.with(shift);
        assert_eq!(
            pass(&[Event::Modifiers(both), down(Key::Shift)]),
            Take::Waiting
        );
        assert_eq!(pass(&[down(Key::Char('c'))]), Take::Waiting);
        assert_eq!(pass(&[up(Key::Char('c'))]), Take::Waiting);
        // Pressed again before letting go, so the slip is fixed.
        assert_eq!(
            pass(&[down(Key::Char('d')), up(Key::Char('d'))]),
            Take::Waiting
        );
        assert_eq!(
            pass(&[Event::Modifiers(ctrl), up(Key::Shift)]),
            Take::Waiting
        );
        let done = pass(&[Event::Modifiers(Modifiers::default()), up(Key::Ctrl)]);
        assert_eq!(done, Take::Done(Chord::new(Key::Char('d'), both)));
    }

    #[test]
    fn escape_alone_cancels_only_before_a_chord() {
        let mut input = Input::default();
        let mut pending = None;
        assert_eq!(
            pass(&mut input, &mut pending, &[down(Key::Escape)]),
            Take::Cancelled
        );
        let mut input = Input::default();
        let ctrl = Modifiers::CTRL;
        let events = [
            Event::Modifiers(ctrl),
            down(Key::Ctrl),
            down(Key::Char('k')),
        ];
        assert_eq!(pass(&mut input, &mut pending, &events), Take::Waiting);
        let events = [
            Event::Modifiers(Modifiers::default()),
            up(Key::Ctrl),
            down(Key::Escape),
        ];
        assert_eq!(pass(&mut input, &mut pending, &events), Take::Waiting);
    }

    #[test]
    fn only_changed_commands_are_saved_and_read_back() {
        assert_eq!(Keybinds::default().saved(), Saved::default());
        let saved = Saved(BTreeMap::from([(
            "zoom.in".to_owned(),
            vec!["ctrl+shift+z".to_owned()],
        )]));
        let keybinds = Keybinds::new(&saved);
        assert_eq!(keybinds.saved(), saved);
    }

    #[test]
    fn an_unreadable_command_reads_as_its_defaults_alone() {
        let text = "\"zoom.in\" = 3\n\"zoom.out\" = [\"ctrl+shift+x\"]\n";
        let saved: Saved = toml::from_str(text).unwrap();
        let keybinds = Keybinds::new(&saved);
        let changed: Vec<_> = keybinds.saved().0.into_keys().collect();
        assert_eq!(changed, ["zoom.out"]);
    }

    #[test]
    fn recording_a_chord_a_command_has_drops_the_one_it_replaced() {
        let mut keybinds = Keybinds::default();
        let at = (0..)
            .zip(commands::all())
            .find_map(|(at, command)| (command.chords.len() == 2).then_some(at))
            .unwrap();
        let first = keybinds.chords[at][0];
        keybinds.apply(
            at,
            Change::Record {
                slot: Some(1),
                chord: first,
            },
        );
        assert_eq!(keybinds.chords[at], [first]);
        keybinds.apply(at, Change::Reset);
        assert!(keybinds.take_keymap().is_some());
        assert!(keybinds.saved().0.is_empty());
    }
}
