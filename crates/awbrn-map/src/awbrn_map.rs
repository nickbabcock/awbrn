use crate::{
    Position,
    awbw_map::AwbwMap,
    pathfinding::{MovementMap, PathFinder},
};
use awbrn_core::{GraphicalTerrain, MovementTerrain, Terrain};

/// Represents a game map with graphical terrain data
#[derive(Debug, Clone, PartialEq)]
pub struct AwbrnMap {
    /// Width of the map in tiles
    width: usize,

    /// Graphical terrain data stored as a flattened 2D array (row-major order)
    terrain: Vec<GraphicalTerrain>,
}

impl AwbrnMap {
    /// Convert an AwbwMap to an AwbrnMap, handling graphical differences
    pub fn from_map(map: &AwbwMap) -> Self {
        let width = map.width();

        let terrain = map
            .iter()
            .map(|(pos, t)| match t {
                Terrain::Mountain
                    if matches!(
                        map.terrain_at(pos.movement(0, -1)),
                        Some(Terrain::Property(_) | Terrain::MissileSilo(_))
                    ) =>
                {
                    GraphicalTerrain::StubbyMoutain
                }
                _ => GraphicalTerrain::Terrain(t),
            })
            .collect::<Vec<_>>();

        Self { width, terrain }
    }

    /// Create a new map with specified dimensions and default terrain
    pub fn new(width: usize, height: usize, default_terrain: GraphicalTerrain) -> Self {
        Self {
            width,
            terrain: vec![default_terrain; width * height],
        }
    }

    /// Get the width of the map
    pub fn width(&self) -> usize {
        self.width
    }

    /// Get the height of the map
    pub fn height(&self) -> usize {
        self.terrain.len() / self.width
    }

    /// Get the terrain at the specified position
    pub fn terrain_at(&self, pos: Position) -> Option<GraphicalTerrain> {
        self.terrain.get(pos.y * self.width + pos.x).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Position, GraphicalTerrain)> {
        self.terrain.iter().enumerate().map(move |(idx, terrain)| {
            let y = idx / self.width;
            let x = idx % self.width;
            (Position::new(x, y), *terrain)
        })
    }

    pub fn pathfinder(&self) -> PathFinder<&Self> {
        PathFinder::new(self)
    }
}

impl MovementMap for AwbrnMap {
    fn terrain_at(&self, pos: Position) -> Option<MovementTerrain> {
        self.terrain_at(pos)
            .map(|x| x.as_terrain())
            .map(MovementTerrain::from)
    }

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;
    use std::path::Path;

    #[test]
    fn test_specific_stubby_mountains() {
        // Construct path to the map file - using path relative to workspace root
        let workspace_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let map_path = workspace_dir.join("assets/maps/155801.txt");

        // Load the 155801 map data
        let map_data = fs::read_to_string(map_path).unwrap();
        let awbw_map = AwbwMap::parse_txt(&map_data).unwrap();
        let awbrn_map = AwbrnMap::from_map(&awbw_map);

        // Expected stubby mountain coordinates as a set of (x, y) tuples
        let expected_stubby_mountains: HashSet<Position> = [
            Position::new(21, 15),
            Position::new(16, 13),
            Position::new(13, 1),
            Position::new(3, 10),
            Position::new(14, 11),
            Position::new(16, 10),
        ]
        .into();

        // Find all stubby mountains in the map
        let actual_stubby_mountains = awbrn_map
            .iter()
            .filter_map(|(pos, terrain)| {
                if matches!(terrain, GraphicalTerrain::StubbyMoutain) {
                    Some(pos)
                } else {
                    None
                }
            })
            .collect();

        // Assert that the sets are equal
        assert_eq!(
            expected_stubby_mountains, actual_stubby_mountains,
            "Expected stubby mountains don't match actual stubby mountains"
        );
    }
}
