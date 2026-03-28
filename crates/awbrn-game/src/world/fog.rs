use awbrn_map::Position;
use awbrn_types::{GraphicalTerrain, PlayerFaction};
use bevy::prelude::*;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FogOfWarState {
    #[default]
    Hidden,
    AirUnitsVisible,
    AllVisible,
}

/// Per-tile fog properties derived from terrain type.
/// `limit` matches `TerrainTile.LimitFogOfWarSightDistance` from `Tiles.json`
/// (Wood/Reef limit sight to 1 tile).
/// `sight_increase` matches `TerrainTile.SightDistanceIncrease` and is applied
/// to non-air units *standing* on this tile (Mountain: +3). It is **not** used
/// inside `apply_unit_vision` — callers must add it to vision_range beforehand.
#[derive(Clone, Copy)]
pub struct TerrainFogProperties {
    /// Vision range bonus for non-air units standing on this tile (Mountain: 3).
    pub sight_increase: i32,
    /// Max distance at which units on this tile are visible. 0 means no limit.
    /// Wood and Reef have limit=1 (hidden unless adjacent).
    pub limit: u32,
}

impl TerrainFogProperties {
    pub fn from_graphical_terrain(terrain: GraphicalTerrain) -> Self {
        match terrain {
            GraphicalTerrain::Mountain | GraphicalTerrain::StubbyMoutain => TerrainFogProperties {
                sight_increase: 3,
                limit: 0,
            },
            GraphicalTerrain::Wood => TerrainFogProperties {
                sight_increase: 0,
                limit: 1,
            },
            GraphicalTerrain::Reef => TerrainFogProperties {
                sight_increase: 0,
                limit: 1,
            },
            _ => TerrainFogProperties {
                sight_increase: 0,
                limit: 0,
            },
        }
    }
}

/// Tile-level fog visibility grid. Row-major layout matching `BoardIndex`.
#[derive(Resource)]
pub struct FogOfWarMap {
    width: usize,
    height: usize,
    tiles: Vec<FogOfWarState>,
}

impl Default for FogOfWarMap {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl FogOfWarMap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            tiles: vec![FogOfWarState::Hidden; width * height],
        }
    }

    pub fn reset(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.tiles.resize(width * height, FogOfWarState::Hidden);
        self.clear();
    }

    pub fn is_fogged(&self, position: Position) -> bool {
        self.get(position) != Some(FogOfWarState::AllVisible)
    }

    pub fn is_air_units_visible(&self, position: Position) -> bool {
        matches!(
            self.get(position),
            Some(FogOfWarState::AirUnitsVisible | FogOfWarState::AllVisible)
        )
    }

    pub fn is_ground_units_visible(&self, position: Position) -> bool {
        self.get(position) == Some(FogOfWarState::AllVisible)
    }

    pub fn is_unit_visible(&self, position: Position, is_air_unit: bool) -> bool {
        if is_air_unit {
            self.is_air_units_visible(position)
        } else {
            self.is_ground_units_visible(position)
        }
    }

    pub fn get(&self, position: Position) -> Option<FogOfWarState> {
        self.tile_index(position).map(|i| self.tiles[i])
    }

    /// Reset all tiles to Hidden.
    pub fn clear(&mut self) {
        self.tiles.fill(FogOfWarState::Hidden);
    }

    /// Mark all tiles as AllVisible (no fog).
    pub fn reveal_all(&mut self) {
        self.tiles.fill(FogOfWarState::AllVisible);
    }

    /// Mark a single tile as AllVisible.
    pub fn reveal(&mut self, position: Position) {
        if let Some(i) = self.tile_index(position) {
            self.tiles[i] = FogOfWarState::AllVisible;
        }
    }

    /// Mark a tile as visible only to air units unless it is already fully visible.
    pub fn reveal_air_units(&mut self, position: Position) {
        if let Some(i) = self.tile_index(position)
            && self.tiles[i] == FogOfWarState::Hidden
        {
            self.tiles[i] = FogOfWarState::AirUnitsVisible;
        }
    }

    /// Compute vision for a single unit as a Manhattan-distance diamond.
    /// Target-tile `limit` (Wood/Reef) is checked here; the caller must add
    /// the source-tile `sight_increase` to `vision_range` before calling.
    pub fn apply_unit_vision(
        &mut self,
        unit_pos: Position,
        vision_range: i32,
        terrain_at: &impl Fn(Position) -> TerrainFogProperties,
    ) {
        for dx in -vision_range..=vision_range {
            for dy in -vision_range..=vision_range {
                let distance = dx.unsigned_abs() + dy.unsigned_abs();
                if distance > vision_range as u32 {
                    continue;
                }
                let Some(tile_pos) = self.offset(unit_pos, dx, dy) else {
                    continue;
                };
                let props = terrain_at(tile_pos);
                if props.limit > 0 && distance > props.limit {
                    // Wood/Reef: ground and sea units stay hidden beyond the
                    // limit, but air units remain visible via the
                    // `AirUnitsVisible` state.
                    self.reveal_air_units(tile_pos);
                    continue;
                }
                self.reveal(tile_pos);
            }
        }
    }

    fn tile_index(&self, position: Position) -> Option<usize> {
        if position.x < self.width && position.y < self.height {
            Some(position.y * self.width + position.x)
        } else {
            None
        }
    }

    fn offset(&self, pos: Position, dx: i32, dy: i32) -> Option<Position> {
        let x = pos.x as i32 + dx;
        let y = pos.y as i32 + dy;
        if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            Some(Position::new(x as usize, y as usize))
        } else {
            None
        }
    }
}

/// Whether fog rendering is currently active.
/// False for Spectator mode or non-fog games.
#[derive(Resource, Default)]
pub struct FogActive(pub bool);

/// The set of factions that are "friendly" to the current viewer.
/// Derived from the replay viewpoint and player registry.
#[derive(Resource, Default)]
pub struct FriendlyFactions(pub HashSet<PlayerFaction>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_map_is_all_fogged() {
        let map = FogOfWarMap::new(3, 3);
        assert!(map.is_fogged(Position::new(0, 0)));
        assert!(map.is_fogged(Position::new(1, 1)));
    }

    #[test]
    fn reveal_marks_tile_visible() {
        let mut map = FogOfWarMap::new(3, 3);
        map.reveal(Position::new(1, 1));
        assert!(!map.is_fogged(Position::new(1, 1)));
        assert!(map.is_fogged(Position::new(0, 0)));
    }

    #[test]
    fn out_of_bounds_is_fogged() {
        let map = FogOfWarMap::new(3, 3);
        assert!(map.is_fogged(Position::new(10, 10)));
    }

    #[test]
    fn clear_resets_to_hidden() {
        let mut map = FogOfWarMap::new(3, 3);
        map.reveal_all();
        assert!(!map.is_fogged(Position::new(1, 1)));
        map.clear();
        assert!(map.is_fogged(Position::new(1, 1)));
    }

    fn no_terrain_modifiers(_pos: Position) -> TerrainFogProperties {
        TerrainFogProperties {
            sight_increase: 0,
            limit: 0,
        }
    }

    #[test]
    fn apply_unit_vision_creates_manhattan_diamond() {
        let mut map = FogOfWarMap::new(7, 7);
        let center = Position::new(3, 3);
        map.apply_unit_vision(center, 2, &no_terrain_modifiers);

        assert!(!map.is_fogged(Position::new(3, 3)));
        assert!(!map.is_fogged(Position::new(3, 1)));
        assert!(!map.is_fogged(Position::new(3, 5)));
        assert!(!map.is_fogged(Position::new(1, 3)));
        assert!(!map.is_fogged(Position::new(5, 3)));
        assert!(!map.is_fogged(Position::new(4, 4)));

        assert!(map.is_fogged(Position::new(3, 0)));
        assert!(map.is_fogged(Position::new(0, 3)));
        assert!(map.is_fogged(Position::new(5, 4)));
    }

    #[test]
    fn apply_unit_vision_respects_limit_distance() {
        let mut map = FogOfWarMap::new(7, 7);
        let center = Position::new(3, 3);

        let terrain_at = |pos: Position| {
            if pos == Position::new(4, 3) {
                TerrainFogProperties {
                    sight_increase: 0,
                    limit: 1,
                }
            } else {
                no_terrain_modifiers(pos)
            }
        };

        map.apply_unit_vision(center, 3, &terrain_at);

        assert!(!map.is_fogged(Position::new(4, 3)));

        let mut map2 = FogOfWarMap::new(7, 7);
        let far_unit = Position::new(1, 3);
        map2.apply_unit_vision(far_unit, 3, &terrain_at);

        assert!(map2.is_fogged(Position::new(4, 3)));
        assert!(map2.is_air_units_visible(Position::new(4, 3)));
        assert!(!map2.is_ground_units_visible(Position::new(4, 3)));
    }

    #[test]
    fn apply_unit_vision_clamps_to_map_bounds() {
        let mut map = FogOfWarMap::new(3, 3);
        let corner = Position::new(0, 0);
        map.apply_unit_vision(corner, 2, &no_terrain_modifiers);

        assert!(!map.is_fogged(Position::new(0, 0)));
        assert!(!map.is_fogged(Position::new(1, 1)));
        assert!(!map.is_fogged(Position::new(2, 0)));
        assert!(!map.is_fogged(Position::new(0, 2)));
    }
}
