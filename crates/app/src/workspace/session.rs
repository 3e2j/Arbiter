//! The workspace as the project's session keeps it: each dock's tabs, one
//! panel each by kind, and each band's size, split and whether it's hidden.

use serde::{Deserialize, Serialize};

use crate::panels::Panel;

use super::dock::Tab;
use super::place::{Band, Place};
use super::{MIN_DOCK, Workspace};

/// What [`Workspace::session`] saves and [`Workspace::new`] lays out again.
/// The panels it lists are all there are, so one that's closed is left out,
/// and a kind can show more than once.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct Session {
    /// Only the docks with tabs.
    #[serde(default)]
    docks: Vec<SavedDock>,
    /// Bands left out keep their default size and split.
    #[serde(default)]
    bands: Vec<SavedBand>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct SavedDock {
    place: Place,
    /// By index in `tabs`.
    #[serde(default)]
    active: usize,
    /// Left to right.
    tabs: Vec<SavedTab>,
}

/// One panel. Where its own state goes, once panels save any.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct SavedTab {
    kind: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct SavedBand {
    band: Band,
    size: f32,
    ratio: f32,
    /// Ignored once the band has no tabs, so an empty band can't open hidden.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    hidden: bool,
}

impl Session {
    /// A panel of each kind in `layout` in its place, in order, every band
    /// at its default size.
    pub fn of(layout: &[(&str, Place)]) -> Self {
        let mut docks: Vec<SavedDock> = Vec::new();
        for &(kind, place) in layout {
            let tab = SavedTab {
                kind: kind.to_owned(),
            };
            match docks.iter_mut().find(|dock| dock.place == place) {
                Some(dock) => dock.tabs.push(tab),
                None => docks.push(SavedDock {
                    place,
                    active: 0,
                    tabs: vec![tab],
                }),
            }
        }
        Self {
            docks,
            bands: Vec::new(),
        }
    }

    /// # Errors
    ///
    /// When `text` isn't a session.
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// # Errors
    ///
    /// Never for a session the workspace made, whose sizes are all finite.
    pub fn to_text(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(self)
    }
}

impl Workspace {
    /// Opens each panel in `session` where it sat. One whose kind is gone,
    /// or that may no longer sit there, is left out.
    pub fn new(session: &Session) -> Self {
        let mut workspace = Self {
            docks: Default::default(),
            sizes: Band::ALL.map(Band::default_size),
            ratios: [0.5; Band::COUNT],
            hidden: [false; Band::COUNT],
            drag: None,
            grip: None,
            tab_drag: None,
            menu: None,
        };
        for saved in &session.docks {
            let dock = &mut workspace.docks[saved.place.index()];
            for (at, tab) in (0..).zip(&saved.tabs) {
                let Some(panel) = Panel::open(&tab.kind) else {
                    tracing::warn!("the session has a {} panel, which isn't one", tab.kind);
                    continue;
                };
                if !panel.places().contains(saved.place) {
                    tracing::warn!("a {} panel can't sit at {:?}", tab.kind, saved.place);
                    continue;
                }
                if at == saved.active {
                    dock.active = dock.tabs.len();
                }
                dock.tabs.push(Tab { panel });
            }
        }
        for saved in &session.bands {
            let index = saved.band.index();
            if saved.size.is_finite() {
                workspace.sizes[index] = saved.size.max(MIN_DOCK[saved.band.axis()]);
            }
            if saved.ratio.is_finite() {
                workspace.ratios[index] = saved.ratio.clamp(0., 1.);
            }
            workspace.hidden[index] = saved.hidden && workspace.band_has_tabs(saved.band);
        }
        workspace
    }

    /// As it's laid out now, for [`Workspace::new`] to bring back. Panels
    /// that aren't [`Panel::saved`] are left out.
    pub fn session(&self) -> Session {
        let docks = Place::ALL.into_iter().zip(&self.docks);
        Session {
            docks: docks
                .filter_map(|(place, dock)| {
                    let saved = |tab: &&Tab| tab.panel.saved();
                    let tabs: Vec<_> = dock
                        .tabs
                        .iter()
                        .filter(saved)
                        .map(|tab| SavedTab {
                            kind: tab.panel.kind().to_owned(),
                        })
                        .collect();
                    // Past an active tab that isn't kept is the next that is.
                    let active = dock.tabs.iter().take(dock.active).filter(saved).count();
                    (!tabs.is_empty()).then_some(SavedDock {
                        place,
                        active,
                        tabs,
                    })
                })
                .collect(),
            bands: Band::ALL
                .into_iter()
                .map(|band| SavedBand {
                    band,
                    size: self.sizes[band.index()],
                    ratio: self.ratios[band.index()],
                    hidden: self.hidden[band.index()],
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::{Files, Hex, Output};
    use crate::workspace::Side;

    const LAYOUT: [(&str, Place); 3] = [
        ("files", Place::LeftInnerTop),
        ("Search", Place::LeftInnerTop),
        ("Inspector", Place::RightInnerTop),
    ];

    fn titles(workspace: &Workspace, place: Place) -> Vec<&str> {
        let dock = &workspace.docks[place.index()];
        dock.tabs.iter().map(|tab| tab.panel.title()).collect()
    }

    #[test]
    fn a_layout_opens_its_panels_in_order() {
        let workspace = Workspace::new(&Session::of(&LAYOUT));
        assert_eq!(titles(&workspace, Place::LeftInnerTop), ["Files", "Search"]);
        assert_eq!(titles(&workspace, Place::RightInnerTop), ["Inspector"]);
    }

    #[test]
    fn a_session_round_trips_through_text() {
        let mut workspace = Workspace::new(&Session::of(&LAYOUT));
        workspace.docks[Place::RightInnerTop.index()].remove(0);
        let below = &mut workspace.docks[Place::BelowMain.index()];
        below.tabs.extend([
            Tab {
                panel: Panel::Files(Files::new()),
            },
            Tab {
                panel: Panel::Output(Output::new()),
            },
        ]);
        below.active = 1;
        workspace.sizes[Band::BelowMain.index()] = 300.;
        workspace.ratios[Band::LeftInner.index()] = 0.25;
        workspace.toggle_side(Side::Left);

        let text = workspace.session().to_text().unwrap();
        let session = Session::parse(&text).unwrap();
        assert_eq!(session, workspace.session());
        let restored = Workspace::new(&session);
        assert_eq!(titles(&restored, Place::LeftInnerTop), ["Files", "Search"]);
        assert_eq!(titles(&restored, Place::BelowMain), ["Files", "Output"]);
        assert_eq!(titles(&restored, Place::RightInnerTop), [""; 0]);
        assert_eq!(restored.docks[Place::BelowMain.index()].active, 1);
        assert_eq!(
            restored.sizes[Band::BelowMain.index()].to_bits(),
            300_f32.to_bits()
        );
        assert_eq!(
            restored.ratios[Band::LeftInner.index()].to_bits(),
            0.25_f32.to_bits()
        );
        assert!(!restored.side_shown(Side::Left));
        assert!(restored.side_shown(Side::Bottom));
    }

    #[test]
    fn a_file_isnt_saved_and_the_active_tab_stays_put() {
        let mut workspace = Workspace::new(&Session::of(&[("Messages", Place::Main)]));
        workspace.add(Place::Main, Panel::Hex(Hex::new("a.bin".into())));
        let main = &mut workspace.docks[Place::Main.index()];
        main.tabs.push(Tab {
            panel: Panel::open("Messages").unwrap(),
        });
        main.active = 2;
        let restored = Workspace::new(&workspace.session());
        assert_eq!(titles(&restored, Place::Main), ["Messages", "Messages"]);
        assert_eq!(restored.docks[Place::Main.index()].active, 1);

        workspace.docks[Place::Main.index()].remove(0);
        workspace.docks[Place::Main.index()].remove(1);
        assert_eq!(workspace.session().docks, Session::default().docks);
    }

    #[test]
    fn a_band_without_tabs_doesnt_come_back_hidden() {
        let text = r#"
            [[bands]]
            band = "left-inner"
            size = 240.0
            ratio = 0.5
            hidden = true
        "#;
        let workspace = Workspace::new(&Session::parse(text).unwrap());
        assert!(!workspace.hidden[Band::LeftInner.index()]);
    }

    #[test]
    fn unknown_and_misplaced_panels_are_left_out() {
        let text = r#"
            [[docks]]
            place = "main"
            tabs = [{ kind = "Search" }, { kind = "gone" }, { kind = "Messages" }]
        "#;
        let workspace = Workspace::new(&Session::parse(text).unwrap());
        assert_eq!(titles(&workspace, Place::Main), ["Messages"]);
    }

    #[test]
    fn the_active_tab_follows_its_panel() {
        let text = r#"
            [[docks]]
            place = "left-inner-top"
            active = 2
            tabs = [{ kind = "gone" }, { kind = "Outline" }, { kind = "Search" }]
        "#;
        let workspace = Workspace::new(&Session::parse(text).unwrap());
        let dock = &workspace.docks[Place::LeftInnerTop.index()];
        assert_eq!(
            titles(&workspace, Place::LeftInnerTop),
            ["Outline", "Search"]
        );
        assert_eq!(dock.active, 1);
    }

    #[test]
    fn bad_sizes_keep_their_defaults_or_minimum() {
        let text = r#"
            [[bands]]
            band = "left-inner"
            size = nan
            ratio = 2.0

            [[bands]]
            band = "right-inner"
            size = 1.0
            ratio = inf
        "#;
        let workspace = Workspace::new(&Session::parse(text).unwrap());
        let left = Band::LeftInner.index();
        let right = Band::RightInner.index();
        assert_eq!(
            workspace.sizes[left].to_bits(),
            Band::LeftInner.default_size().to_bits()
        );
        assert_eq!(workspace.ratios[left].to_bits(), 1_f32.to_bits());
        assert_eq!(workspace.sizes[right].to_bits(), MIN_DOCK[0].to_bits());
        assert_eq!(workspace.ratios[right].to_bits(), 0.5_f32.to_bits());
    }

    #[test]
    fn text_that_isnt_a_session_is_an_error() {
        assert!(Session::parse("docks = 3").is_err());
        assert!(Session::parse("[[docks]]\nplace = \"nowhere\"\ntabs = []").is_err());
    }
}
