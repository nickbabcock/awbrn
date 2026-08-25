//! Presentation visibility, read from a typed AWVM observation.
//!
//! No vision is computed here. `awvm::semantic::AwbwVisibility` is the only
//! implementation of `spec/semantics/fog.md` in the workspace; this module
//! caches what one recipient's projection already said, so rendering does not
//! have to carry an `Observation` around.
//!
//! [`ViewerVisibility`] is derived by
//! [`crate::replay::refresh_viewer_visibility`], which is the only writer.

use std::collections::HashSet;

use awbrn_map::{Dimensions, Grid, Pos};
use awbrn_types::{AwbwGamePlayerId, AwbwUnitId, PlayerFaction};
use bevy::prelude::*;

/// The set of factions the viewer commands.
///
/// This is authority, not sight: play mode asks it which units the local
/// player may select. What the viewer can *see* is [`ViewerVisibility`].
#[derive(Resource, Default, Debug)]
pub struct FriendlyFactions(pub HashSet<PlayerFaction>);

/// What the selected viewpoint is entitled to see.
///
/// Every field is a restatement of the selected recipient's `Observation`:
/// a tile is visible because the projection called it visible, and a unit is
/// visible because the projection listed it. An empty default is omniscient
/// and unfogged, which is what a headless fixture without an observation gets.
#[derive(Resource, Debug, Default)]
pub struct ViewerVisibility {
    fog: bool,
    /// One flag for each tile, over the same board shape as `BoardIndex`. A
    /// zero-tile board is what no selected observation looks like, and it
    /// reads as fully visible because `fog` is false alongside it.
    tiles: Grid<bool>,
    units: HashSet<AwbwUnitId>,
    players: HashSet<AwbwGamePlayerId>,
}

impl ViewerVisibility {
    /// Whether the viewer is looking through fog at all.
    ///
    /// A spectator and an unfogged match both answer `false`, and every
    /// consumer treats that as "show everything".
    pub fn fog_active(&self) -> bool {
        self.fog
    }

    pub fn tile_visible(&self, position: Pos) -> bool {
        if !self.fog {
            return true;
        }
        self.tiles.get(position).copied().unwrap_or(false)
    }

    pub fn is_fogged(&self, position: Pos) -> bool {
        !self.tile_visible(position)
    }

    /// Whether the viewer may see `unit` where it currently is.
    ///
    /// Cargo is excluded by the caller, which does not render carried units.
    pub fn unit_visible(&self, unit: AwbwUnitId) -> bool {
        !self.fog || self.units.contains(&unit)
    }

    /// Whether `player`'s funds and unit roster are disclosed to the viewer.
    ///
    /// A projection reports a teammate's private state and an opponent's
    /// public state, so this is the observation's own disclosure rule rather
    /// than a second one.
    pub fn player_disclosed(&self, player: AwbwGamePlayerId) -> bool {
        !self.fog || self.players.contains(&player)
    }

    /// Forget the selected observation and show everything.
    ///
    /// The mutators below are the seam
    /// [`crate::replay::refresh_viewer_visibility`] writes through. They are
    /// public so a test can state a view without building an `Observation`;
    /// nothing in production writes them anywhere else.
    pub fn clear(&mut self) {
        self.reset(false, Dimensions::new(0, 0));
    }

    pub fn reset(&mut self, fog: bool, dimensions: Dimensions) {
        self.fog = fog;
        self.tiles.refill(dimensions, false);
        self.units.clear();
        self.players.clear();
    }

    pub fn set_tile_visible(&mut self, position: Pos) {
        if let Some(tile) = self.tiles.get_mut(position) {
            *tile = true;
        }
    }

    pub fn set_unit_visible(&mut self, unit: AwbwUnitId) {
        self.units.insert(unit);
    }

    pub fn set_player_disclosed(&mut self, player: AwbwGamePlayerId) {
        self.players.insert(player);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unobserved_viewer_sees_everything() {
        let visibility = ViewerVisibility::default();
        assert!(!visibility.fog_active());
        assert!(visibility.tile_visible(Pos::new(4, 9)));
        assert!(visibility.unit_visible(AwbwUnitId::new(7)));
        assert!(visibility.player_disclosed(AwbwGamePlayerId::new(3)));
    }

    #[test]
    fn a_fogged_viewer_sees_only_what_the_observation_listed() {
        let mut visibility = ViewerVisibility::default();
        visibility.reset(true, Dimensions::new(3, 3));
        visibility.set_tile_visible(Pos::new(1, 1));
        visibility.set_unit_visible(AwbwUnitId::new(7));
        visibility.set_player_disclosed(AwbwGamePlayerId::new(3));

        assert!(visibility.tile_visible(Pos::new(1, 1)));
        assert!(visibility.is_fogged(Pos::new(0, 0)));
        // Off the board is never visible, and never panics.
        assert!(visibility.is_fogged(Pos::new(9, 9)));
        assert!(visibility.unit_visible(AwbwUnitId::new(7)));
        assert!(!visibility.unit_visible(AwbwUnitId::new(8)));
        assert!(visibility.player_disclosed(AwbwGamePlayerId::new(3)));
        assert!(!visibility.player_disclosed(AwbwGamePlayerId::new(4)));
    }

    #[test]
    fn an_unfogged_match_discloses_everything_it_was_reset_with() {
        let mut visibility = ViewerVisibility::default();
        visibility.reset(false, Dimensions::new(3, 3));

        assert!(visibility.tile_visible(Pos::new(2, 2)));
        assert!(visibility.unit_visible(AwbwUnitId::new(1)));
        assert!(visibility.player_disclosed(AwbwGamePlayerId::new(1)));
    }
}
