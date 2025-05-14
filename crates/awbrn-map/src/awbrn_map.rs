use crate::{
    Position,
    awbw_map::AwbwMap,
    pathfinding::{MovementMap, PathFinder},
};
use awbrn_core::{AwbwTerrain, GraphicalTerrain, MovementTerrain, SeaDirection, ShoalDirection};

/// Represents a game map with graphical terrain data
#[derive(Debug, Clone, PartialEq)]
pub struct AwbrnMap {
    /// Width of the map in tiles
    width: usize,

    /// Graphical terrain data stored as a flattened 2D array (row-major order)
    terrain: Vec<GraphicalTerrain>,
}

/// Represents the terrain types of neighboring tiles
#[derive(Debug, Clone, Copy)]
struct NearbyTiles {
    north_west: Option<AwbwTerrain>,
    north: Option<AwbwTerrain>,
    north_east: Option<AwbwTerrain>,
    east: Option<AwbwTerrain>,
    south_east: Option<AwbwTerrain>,
    south: Option<AwbwTerrain>,
    south_west: Option<AwbwTerrain>,
    west: Option<AwbwTerrain>,
}

impl AwbrnMap {
    /// Get neighboring tiles for a position
    fn get_nearby_tiles(map: &AwbwMap, pos: Position) -> NearbyTiles {
        let north_west = if pos.x > 0 && pos.y > 0 {
            map.terrain_at(pos.movement(-1, -1))
        } else {
            None
        };

        let north = if pos.y > 0 {
            map.terrain_at(pos.movement(0, -1))
        } else {
            None
        };

        let north_east = if pos.y > 0 {
            map.terrain_at(pos.movement(1, -1))
        } else {
            None
        };

        let east = map.terrain_at(pos.movement(1, 0));

        let south_east = map.terrain_at(pos.movement(1, 1));

        let south = map.terrain_at(pos.movement(0, 1));

        let south_west = if pos.x > 0 {
            map.terrain_at(pos.movement(-1, 1))
        } else {
            None
        };

        let west = if pos.x > 0 {
            map.terrain_at(pos.movement(-1, 0))
        } else {
            None
        };

        NearbyTiles {
            north_west,
            north,
            north_east,
            east,
            south_east,
            south,
            south_west,
            west,
        }
    }

    /// Determine the sea direction based on neighboring tiles
    fn determine_sea_direction(nearby: &NearbyTiles) -> SeaDirection {
        let is_land =
            |terrain: Option<AwbwTerrain>| -> bool { terrain.is_some_and(|t| t.is_land()) };

        // Check for land in each direction
        let n = is_land(nearby.north);
        let e = is_land(nearby.east);
        let s = is_land(nearby.south);
        let w = is_land(nearby.west);
        let nw = is_land(nearby.north_west);
        let ne = is_land(nearby.north_east);
        let se = is_land(nearby.south_east);
        let sw = is_land(nearby.south_west);

        // Full circle
        if n && e && s && w {
            return SeaDirection::N_E_S_W;
        }

        // Missing an edge
        if e && s && w && !n {
            return SeaDirection::E_S_W;
        }
        if n && s && w && !e {
            return SeaDirection::N_S_W;
        }
        if n && e && w && !s {
            return SeaDirection::N_E_W;
        }
        if n && e && s && !w {
            return SeaDirection::N_E_S;
        }

        // Tunnels
        if n && s && !e && !w {
            return SeaDirection::N_S;
        }
        if e && w && !n && !s {
            return SeaDirection::E_W;
        }

        // Corners
        if n && e && sw && !s && !w {
            return SeaDirection::N_E_SW;
        }
        if n && e && !s && !w {
            return SeaDirection::N_E;
        }

        if e && s && nw && !n && !w {
            return SeaDirection::E_S_NW;
        }
        if e && s && !n && !w {
            return SeaDirection::E_S;
        }

        if s && w && ne && !n && !e {
            return SeaDirection::S_W_NE;
        }
        if s && w && !n && !e {
            return SeaDirection::S_W;
        }

        if n && w && se && !e && !s {
            return SeaDirection::N_W_SE;
        }
        if n && w && !e && !s {
            return SeaDirection::N_W;
        }

        // Edge
        if n && se && sw && !e && !s && !w {
            return SeaDirection::N_SE_SW;
        }
        if n && se && !e && !s && !w {
            return SeaDirection::N_SE;
        }
        if n && sw && !e && !s && !w {
            return SeaDirection::N_SW;
        }
        if n && !e && !s && !w {
            return SeaDirection::N;
        }

        if e && nw && sw && !n && !s && !w {
            return SeaDirection::E_NW_SW;
        }
        if e && nw && !n && !s && !w {
            return SeaDirection::E_NW;
        }
        if e && sw && !n && !s && !w {
            return SeaDirection::E_SW;
        }
        if e && !n && !s && !w {
            return SeaDirection::E;
        }

        if s && nw && ne && !n && !e && !w {
            return SeaDirection::S_NW_NE;
        }
        if s && nw && !n && !e && !w {
            return SeaDirection::S_NW;
        }
        if s && ne && !n && !e && !w {
            return SeaDirection::S_NE;
        }
        if s && !n && !e && !w {
            return SeaDirection::S;
        }

        if w && ne && se && !n && !e && !s {
            return SeaDirection::W_NE_SE;
        }
        if w && ne && !n && !e && !s {
            return SeaDirection::W_NE;
        }
        if w && se && !n && !e && !s {
            return SeaDirection::W_SE;
        }
        if w && !n && !e && !s {
            return SeaDirection::W;
        }

        // Full Corners
        if nw && ne && se && sw && !n && !e && !s && !w {
            return SeaDirection::NW_NE_SE_SW;
        }

        // Missing 1 corner
        if ne && se && sw && !nw && !n && !e && !s && !w {
            return SeaDirection::NE_SE_SW;
        }
        if nw && se && sw && !ne && !n && !e && !s && !w {
            return SeaDirection::NW_SE_SW;
        }
        if nw && ne && sw && !se && !n && !e && !s && !w {
            return SeaDirection::NW_NE_SW;
        }
        if nw && ne && se && !sw && !n && !e && !s && !w {
            return SeaDirection::NW_NE_SE;
        }

        // Missing 2 corners
        if se && sw && !nw && !ne && !n && !e && !s && !w {
            return SeaDirection::SE_SW;
        }
        if nw && sw && !ne && !se && !n && !e && !s && !w {
            return SeaDirection::NW_SW;
        }
        if nw && ne && !se && !sw && !n && !e && !s && !w {
            return SeaDirection::NW_NE;
        }
        if ne && se && !nw && !sw && !n && !e && !s && !w {
            return SeaDirection::NE_SE;
        }
        if nw && se && !ne && !sw && !n && !e && !s && !w {
            return SeaDirection::NW_SE;
        }
        if ne && sw && !nw && !se && !n && !e && !s && !w {
            return SeaDirection::NE_SW;
        }

        // Missing 3 corners
        if nw && !ne && !se && !sw && !n && !e && !s && !w {
            return SeaDirection::NW;
        }
        if ne && !nw && !se && !sw && !n && !e && !s && !w {
            return SeaDirection::NE;
        }
        if se && !nw && !ne && !sw && !n && !e && !s && !w {
            return SeaDirection::SE;
        }
        if sw && !nw && !ne && !se && !n && !e && !s && !w {
            return SeaDirection::SW;
        }

        // Default
        SeaDirection::Sea
    }

    /// Determine the shoal direction based on neighboring tiles
    fn determine_shoal_direction(nearby: &NearbyTiles) -> ShoalDirection {
        let is_land =
            |terrain: Option<AwbwTerrain>| -> bool { terrain.is_some_and(|t| t.is_land()) };

        let is_shoal = |terrain: Option<AwbwTerrain>| -> bool {
            terrain.is_some_and(|t| matches!(t, AwbwTerrain::Shoal(_)))
        };

        // Check for land and shoal in each direction
        let n_land = is_land(nearby.north);
        let e_land = is_land(nearby.east);
        let s_land = is_land(nearby.south);
        let w_land = is_land(nearby.west);

        let n_shoal = is_shoal(nearby.north);
        let e_shoal = is_shoal(nearby.east);
        let s_shoal = is_shoal(nearby.south);
        let w_shoal = is_shoal(nearby.west);

        let mut parts = Vec::new();

        if n_land {
            parts.push(b'N');
        } else if !n_shoal {
            parts.push(b'A');
            parts.push(b'N');
        }

        if e_land {
            parts.push(b'E');
        } else if !e_shoal {
            parts.push(b'A');
            parts.push(b'E');
        }

        if s_land {
            parts.push(b'S');
        } else if !s_shoal {
            parts.push(b'A');
            parts.push(b'S');
        }

        if w_land {
            parts.push(b'W');
        } else if !w_shoal {
            parts.push(b'A');
            parts.push(b'W');
        }

        // Convert the parts to a ShoalDirection
        if parts.is_empty() {
            return ShoalDirection::C;
        }

        match parts.as_slice() {
            b"AE" => ShoalDirection::AE,
            b"AEAS" => ShoalDirection::AEAS,
            b"AEASAW" => ShoalDirection::AEASAW,
            b"AEASW" => ShoalDirection::AEASW,
            b"AEAW" => ShoalDirection::AEAW,
            b"AES" => ShoalDirection::AES,
            b"AESAW" => ShoalDirection::AESAW,
            b"AESW" => ShoalDirection::AESW,
            b"AEW" => ShoalDirection::AEW,
            b"AN" => ShoalDirection::AN,
            b"ANAE" => ShoalDirection::ANAE,
            b"ANAEAS" => ShoalDirection::ANAEAS,
            b"ANAEASAW" => ShoalDirection::ANAEASAW,
            b"ANAEASW" => ShoalDirection::ANAEASW,
            b"ANAEAW" => ShoalDirection::ANAEAW,
            b"ANAES" => ShoalDirection::ANAES,
            b"ANAESAW" => ShoalDirection::ANAESAW,
            b"ANAESW" => ShoalDirection::ANAESW,
            b"ANAEW" => ShoalDirection::ANAEW,
            b"ANAS" => ShoalDirection::ANAS,
            b"ANASAW" => ShoalDirection::ANASAW,
            b"ANASW" => ShoalDirection::ANASW,
            b"ANAW" => ShoalDirection::ANAW,
            b"ANE" => ShoalDirection::ANE,
            b"ANEAS" => ShoalDirection::ANEAS,
            b"ANEASAW" => ShoalDirection::ANEASAW,
            b"ANEASW" => ShoalDirection::ANEASW,
            b"ANEAW" => ShoalDirection::ANEAW,
            b"ANES" => ShoalDirection::ANES,
            b"ANESAW" => ShoalDirection::ANESAW,
            b"ANESW" => ShoalDirection::ANESW,
            b"ANEW" => ShoalDirection::ANEW,
            b"ANS" => ShoalDirection::ANS,
            b"ANSAW" => ShoalDirection::ANSAW,
            b"ANSW" => ShoalDirection::ANSW,
            b"ANW" => ShoalDirection::ANW,
            b"AS" => ShoalDirection::AS,
            b"ASAW" => ShoalDirection::ASAW,
            b"ASW" => ShoalDirection::ASW,
            b"AW" => ShoalDirection::AW,
            b"E" => ShoalDirection::E,
            b"EAS" => ShoalDirection::EAS,
            b"EASAW" => ShoalDirection::EASAW,
            b"EASW" => ShoalDirection::EASW,
            b"EAW" => ShoalDirection::EAW,
            b"ES" => ShoalDirection::ES,
            b"ESAW" => ShoalDirection::ESAW,
            b"ESW" => ShoalDirection::ESW,
            b"EW" => ShoalDirection::EW,
            b"N" => ShoalDirection::N,
            b"NAE" => ShoalDirection::NAE,
            b"NAEAS" => ShoalDirection::NAEAS,
            b"NAEASAW" => ShoalDirection::NAEASAW,
            b"NAEASW" => ShoalDirection::NAEASW,
            b"NAEAW" => ShoalDirection::NAEAW,
            b"NAES" => ShoalDirection::NAES,
            b"NAESAW" => ShoalDirection::NAESAW,
            b"NAESW" => ShoalDirection::NAESW,
            b"NAEW" => ShoalDirection::NAEW,
            b"NAS" => ShoalDirection::NAS,
            b"NASAW" => ShoalDirection::NASAW,
            b"NASW" => ShoalDirection::NASW,
            b"NAW" => ShoalDirection::NAW,
            b"NE" => ShoalDirection::NE,
            b"NEAS" => ShoalDirection::NEAS,
            b"NEASAW" => ShoalDirection::NEASAW,
            b"NEASW" => ShoalDirection::NEASW,
            b"NEAW" => ShoalDirection::NEAW,
            b"NES" => ShoalDirection::NES,
            b"NESAW" => ShoalDirection::NESAW,
            b"NESW" => ShoalDirection::NESW,
            b"NEW" => ShoalDirection::NEW,
            b"NS" => ShoalDirection::NS,
            b"NSAW" => ShoalDirection::NSAW,
            b"NSW" => ShoalDirection::NSW,
            b"NW" => ShoalDirection::NW,
            b"S" => ShoalDirection::S,
            b"SAW" => ShoalDirection::SAW,
            b"SW" => ShoalDirection::SW,
            b"W" => ShoalDirection::W,
            _ => ShoalDirection::C,
        }
    }

    /// Convert an AwbwMap to an AwbrnMap, handling graphical differences
    pub fn from_map(map: &AwbwMap) -> Self {
        let width = map.width();

        let terrain = map
            .iter()
            .map(|(pos, t)| match t {
                AwbwTerrain::Mountain
                    if matches!(
                        map.terrain_at(pos.movement(0, -1)),
                        Some(AwbwTerrain::Property(_) | AwbwTerrain::MissileSilo(_))
                    ) =>
                {
                    GraphicalTerrain::StubbyMoutain
                }
                AwbwTerrain::Mountain => GraphicalTerrain::Mountain,
                AwbwTerrain::Plain => GraphicalTerrain::Plain,
                AwbwTerrain::Wood => GraphicalTerrain::Wood,
                AwbwTerrain::River(river_type) => GraphicalTerrain::River(river_type),
                AwbwTerrain::Road(road_type) => GraphicalTerrain::Road(road_type),
                AwbwTerrain::Bridge(bridge_type) => GraphicalTerrain::Bridge(bridge_type),
                AwbwTerrain::Sea => {
                    let nearby = Self::get_nearby_tiles(map, pos);
                    let sea_direction = Self::determine_sea_direction(&nearby);
                    GraphicalTerrain::Sea(sea_direction)
                }
                AwbwTerrain::Shoal(_) => {
                    let nearby = Self::get_nearby_tiles(map, pos);
                    let shoal_direction = Self::determine_shoal_direction(&nearby);
                    GraphicalTerrain::Shoal(shoal_direction)
                }
                AwbwTerrain::Reef => GraphicalTerrain::Reef,
                AwbwTerrain::Property(property) => GraphicalTerrain::Property(property),
                AwbwTerrain::Pipe(pipe_type) => GraphicalTerrain::Pipe(pipe_type),
                AwbwTerrain::MissileSilo(missile_silo_status) => {
                    GraphicalTerrain::MissileSilo(missile_silo_status)
                }
                AwbwTerrain::PipeSeam(pipe_seam_type) => GraphicalTerrain::PipeSeam(pipe_seam_type),
                AwbwTerrain::PipeRubble(pipe_rubble_type) => {
                    GraphicalTerrain::PipeRubble(pipe_rubble_type)
                }
                AwbwTerrain::Teleporter => GraphicalTerrain::Teleporter,
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
