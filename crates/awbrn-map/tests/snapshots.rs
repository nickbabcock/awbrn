use awbrn_core::GraphicalTerrain;
use awbrn_map::Position;
use insta::{assert_snapshot, glob};
use std::fmt::Write;

#[test]
fn test_map_snapshots() {
    glob!("../../../assets/maps", "*.txt", |path| {
        let data = std::fs::read_to_string(path).unwrap();
        let map = awbrn_map::AwbwMap::parse_txt(&data[..]);

        let map = match map {
            Ok(x) => x,
            Err(e) => panic!(
                "failed parsing: (INSTA_GLOB_FILTER={}) {}",
                path.file_name().unwrap().to_string_lossy(),
                e
            ),
        };

        assert_snapshot!(map.to_string())
    });
}

#[test]
fn test_map_refinement() {
    glob!("../../../assets/maps", "*.txt", |path| {
        let data = std::fs::read_to_string(path).unwrap();
        let Ok(map) = awbrn_map::AwbwMap::parse_txt(&data[..]) else {
            return;
        };

        let awbrn_map = awbrn_map::AwbrnMap::from_map(&map);

        // Create a formatted grid representation of the map
        let formatted_map = format_map_as_grid(&awbrn_map);
        assert_snapshot!(formatted_map);
    });
}

/// Format the map as a grid of 2-character terrain representations
fn format_map_as_grid(map: &awbrn_map::AwbrnMap) -> String {
    let width = map.width();
    let height = map.height();

    let mut result = String::new();

    for y in 0..height {
        for x in 0..width {
            let terrain = map.terrain_at(Position::new(x, y)).unwrap();
            let terrain = terrain_to_chars(terrain);
            let _ = write!(result, "{}", terrain);
            if x < width - 1 {
                let _ = write!(result, " ");
            }
        }
        let _ = writeln!(result);
    }

    result
}

/// Convert a GraphicalTerrain to a unique 2-character representation
fn terrain_to_chars(terrain: GraphicalTerrain) -> String {
    match terrain {
        // Basic terrains
        GraphicalTerrain::StubbyMoutain => "SM".to_string(),
        GraphicalTerrain::Plain => "PL".to_string(),
        GraphicalTerrain::Mountain => "MT".to_string(),
        GraphicalTerrain::Wood => "WD".to_string(),
        GraphicalTerrain::Reef => "RF".to_string(),

        // Rivers with different configurations
        GraphicalTerrain::River(river_type) => {
            let second_char = match river_type {
                awbrn_core::RiverType::Horizontal => 'H',
                awbrn_core::RiverType::Vertical => 'V',
                awbrn_core::RiverType::Cross => 'C',
                awbrn_core::RiverType::ES => 'E',
                awbrn_core::RiverType::SW => 'S',
                awbrn_core::RiverType::WN => 'W',
                awbrn_core::RiverType::NE => 'N',
                awbrn_core::RiverType::ESW => '1',
                awbrn_core::RiverType::SWN => '2',
                awbrn_core::RiverType::WNE => '3',
                awbrn_core::RiverType::NES => '4',
            };
            format!("R{}", second_char)
        }

        // Roads with different configurations
        GraphicalTerrain::Road(road_type) => {
            let second_char = match road_type {
                awbrn_core::RoadType::Horizontal => 'H',
                awbrn_core::RoadType::Vertical => 'V',
                awbrn_core::RoadType::Cross => 'C',
                awbrn_core::RoadType::ES => 'E',
                awbrn_core::RoadType::SW => 'S',
                awbrn_core::RoadType::WN => 'W',
                awbrn_core::RoadType::NE => 'N',
                awbrn_core::RoadType::ESW => '1',
                awbrn_core::RoadType::SWN => '2',
                awbrn_core::RoadType::WNE => '3',
                awbrn_core::RoadType::NES => '4',
            };
            format!("D{}", second_char)
        }

        // Bridges
        GraphicalTerrain::Bridge(bridge_type) => {
            let second_char = match bridge_type {
                awbrn_core::BridgeType::Horizontal => 'H',
                awbrn_core::BridgeType::Vertical => 'V',
            };
            format!("B{}", second_char)
        }

        // Properties
        GraphicalTerrain::Property(property) => match property {
            awbrn_core::Property::City(faction) => {
                let faction_char = match faction {
                    awbrn_core::Faction::Neutral => 'N',
                    awbrn_core::Faction::Player(player) => player_faction_to_char(player),
                };
                format!("C{}", faction_char)
            }
            awbrn_core::Property::Base(faction) => {
                let faction_char = match faction {
                    awbrn_core::Faction::Neutral => 'N',
                    awbrn_core::Faction::Player(player) => player_faction_to_char(player),
                };
                format!("B{}", faction_char)
            }
            awbrn_core::Property::Airport(faction) => {
                let faction_char = match faction {
                    awbrn_core::Faction::Neutral => 'N',
                    awbrn_core::Faction::Player(player) => player_faction_to_char(player),
                };
                format!("A{}", faction_char)
            }
            awbrn_core::Property::Port(faction) => {
                let faction_char = match faction {
                    awbrn_core::Faction::Neutral => 'N',
                    awbrn_core::Faction::Player(player) => player_faction_to_char(player),
                };
                format!("P{}", faction_char)
            }
            awbrn_core::Property::ComTower(faction) => {
                let faction_char = match faction {
                    awbrn_core::Faction::Neutral => 'N',
                    awbrn_core::Faction::Player(player) => player_faction_to_char(player),
                };
                format!("T{}", faction_char)
            }
            awbrn_core::Property::Lab(faction) => {
                let faction_char = match faction {
                    awbrn_core::Faction::Neutral => 'N',
                    awbrn_core::Faction::Player(player) => player_faction_to_char(player),
                };
                format!("L{}", faction_char)
            }
            awbrn_core::Property::HQ(player_faction) => {
                format!("H{}", player_faction_to_char(player_faction))
            }
        },

        // Pipes and related structures
        GraphicalTerrain::Pipe(pipe_type) => {
            let second_char = match pipe_type {
                awbrn_core::PipeType::Vertical => 'V',
                awbrn_core::PipeType::Horizontal => 'H',
                awbrn_core::PipeType::NE => 'N',
                awbrn_core::PipeType::ES => 'E',
                awbrn_core::PipeType::SW => 'S',
                awbrn_core::PipeType::WN => 'W',
                awbrn_core::PipeType::NorthEnd => '1',
                awbrn_core::PipeType::EastEnd => '2',
                awbrn_core::PipeType::SouthEnd => '3',
                awbrn_core::PipeType::WestEnd => '4',
            };
            format!("P{}", second_char)
        }
        GraphicalTerrain::PipeSeam(seam_type) => {
            let second_char = match seam_type {
                awbrn_core::PipeSeamType::Horizontal => 'H',
                awbrn_core::PipeSeamType::Vertical => 'V',
            };
            format!("S{}", second_char)
        }
        GraphicalTerrain::PipeRubble(rubble_type) => {
            let second_char = match rubble_type {
                awbrn_core::PipeRubbleType::Horizontal => 'H',
                awbrn_core::PipeRubbleType::Vertical => 'V',
            };
            format!("U{}", second_char)
        }

        // Special terrains
        GraphicalTerrain::MissileSilo(status) => {
            let second_char = match status {
                awbrn_core::MissileSiloStatus::Loaded => 'L',
                awbrn_core::MissileSiloStatus::Unloaded => 'U',
            };
            format!("M{}", second_char)
        }
        GraphicalTerrain::Teleporter => "TP".to_string(),

        // Sea and Shoal variants
        GraphicalTerrain::Sea(direction) => {
            let idx = sea_direction_to_index(direction);
            format!("S{}", idx)
        }
        GraphicalTerrain::Shoal(direction) => {
            let idx = shoal_direction_to_index(direction);
            format!("H{}", idx)
        }
    }
}

/// Convert a PlayerFaction to a single character representation
fn player_faction_to_char(player_faction: awbrn_core::PlayerFaction) -> char {
    match player_faction {
        awbrn_core::PlayerFaction::OrangeStar => 'O',
        awbrn_core::PlayerFaction::BlueMoon => 'B',
        awbrn_core::PlayerFaction::GreenEarth => 'G',
        awbrn_core::PlayerFaction::YellowComet => 'Y',
        awbrn_core::PlayerFaction::BlackHole => 'K',
        awbrn_core::PlayerFaction::RedFire => 'R',
        awbrn_core::PlayerFaction::GreySky => 'S',
        awbrn_core::PlayerFaction::BrownDesert => 'D',
        awbrn_core::PlayerFaction::AmberBlaze => 'A',
        awbrn_core::PlayerFaction::JadeSun => 'J',
        awbrn_core::PlayerFaction::CobaltIce => 'C',
        awbrn_core::PlayerFaction::PinkCosmos => 'P',
        awbrn_core::PlayerFaction::TealGalaxy => 'T',
        awbrn_core::PlayerFaction::PurpleLightning => 'L',
        awbrn_core::PlayerFaction::AcidRain => 'I',
        awbrn_core::PlayerFaction::WhiteNova => 'W',
        awbrn_core::PlayerFaction::AzureAsteroid => 'Z',
        awbrn_core::PlayerFaction::NoirEclipse => 'E',
        awbrn_core::PlayerFaction::SilverClaw => 'V',
    }
}

/// Convert a SeaDirection to a unique index
fn sea_direction_to_index(direction: awbrn_core::SeaDirection) -> char {
    match direction {
        awbrn_core::SeaDirection::E => '1',
        awbrn_core::SeaDirection::E_NW => '2',
        awbrn_core::SeaDirection::E_NW_SW => '3',
        awbrn_core::SeaDirection::E_S => '4',
        awbrn_core::SeaDirection::E_S_NW => '5',
        awbrn_core::SeaDirection::E_S_W => '6',
        awbrn_core::SeaDirection::E_SW => '7',
        awbrn_core::SeaDirection::E_W => '8',
        awbrn_core::SeaDirection::N => '9',
        awbrn_core::SeaDirection::N_E => 'a',
        awbrn_core::SeaDirection::N_E_S => 'b',
        awbrn_core::SeaDirection::N_E_S_W => 'c',
        awbrn_core::SeaDirection::N_E_SW => 'd',
        awbrn_core::SeaDirection::N_E_W => 'e',
        awbrn_core::SeaDirection::N_S => 'f',
        awbrn_core::SeaDirection::N_S_W => 'g',
        awbrn_core::SeaDirection::N_SE => 'h',
        awbrn_core::SeaDirection::N_SE_SW => 'i',
        awbrn_core::SeaDirection::N_SW => 'j',
        awbrn_core::SeaDirection::N_W => 'k',
        awbrn_core::SeaDirection::N_W_SE => 'l',
        awbrn_core::SeaDirection::NE => 'm',
        awbrn_core::SeaDirection::NE_SE => 'n',
        awbrn_core::SeaDirection::NE_SE_SW => 'o',
        awbrn_core::SeaDirection::NE_SW => 'p',
        awbrn_core::SeaDirection::NW => 'q',
        awbrn_core::SeaDirection::NW_NE => 'r',
        awbrn_core::SeaDirection::NW_NE_SE => 's',
        awbrn_core::SeaDirection::NW_NE_SE_SW => 't',
        awbrn_core::SeaDirection::NW_NE_SW => 'u',
        awbrn_core::SeaDirection::NW_SE => 'v',
        awbrn_core::SeaDirection::NW_SE_SW => 'w',
        awbrn_core::SeaDirection::NW_SW => 'x',
        awbrn_core::SeaDirection::S => 'y',
        awbrn_core::SeaDirection::S_E => 'z',
        awbrn_core::SeaDirection::S_NE => 'A',
        awbrn_core::SeaDirection::S_NW => 'B',
        awbrn_core::SeaDirection::S_NW_NE => 'C',
        awbrn_core::SeaDirection::S_W => 'D',
        awbrn_core::SeaDirection::S_W_NE => 'E',
        awbrn_core::SeaDirection::SE => 'F',
        awbrn_core::SeaDirection::SE_SW => 'G',
        awbrn_core::SeaDirection::SW => 'H',
        awbrn_core::SeaDirection::Sea => '0',
        awbrn_core::SeaDirection::W => 'I',
        awbrn_core::SeaDirection::W_E => 'J',
        awbrn_core::SeaDirection::W_NE => 'K',
        awbrn_core::SeaDirection::W_NE_SE => 'L',
        awbrn_core::SeaDirection::W_SE => 'M',
    }
}

/// Convert a ShoalDirection to a unique index
fn shoal_direction_to_index(direction: awbrn_core::ShoalDirection) -> char {
    match direction {
        awbrn_core::ShoalDirection::AE => '1',
        awbrn_core::ShoalDirection::AEAS => '2',
        awbrn_core::ShoalDirection::AEASAW => '3',
        awbrn_core::ShoalDirection::AEASW => '4',
        awbrn_core::ShoalDirection::AEAW => '5',
        awbrn_core::ShoalDirection::AES => '6',
        awbrn_core::ShoalDirection::AESAW => '7',
        awbrn_core::ShoalDirection::AESW => '8',
        awbrn_core::ShoalDirection::AEW => '9',
        awbrn_core::ShoalDirection::AN => 'a',
        awbrn_core::ShoalDirection::ANAE => 'b',
        awbrn_core::ShoalDirection::ANAEAS => 'c',
        awbrn_core::ShoalDirection::ANAEASAW => 'd',
        awbrn_core::ShoalDirection::ANAEASW => 'e',
        awbrn_core::ShoalDirection::ANAEAW => 'f',
        awbrn_core::ShoalDirection::ANAES => 'g',
        awbrn_core::ShoalDirection::ANAESAW => 'h',
        awbrn_core::ShoalDirection::ANAESW => 'i',
        awbrn_core::ShoalDirection::ANAEW => 'j',
        awbrn_core::ShoalDirection::ANAS => 'k',
        awbrn_core::ShoalDirection::ANASAW => 'l',
        awbrn_core::ShoalDirection::ANASW => 'm',
        awbrn_core::ShoalDirection::ANAW => 'n',
        awbrn_core::ShoalDirection::ANE => 'o',
        awbrn_core::ShoalDirection::ANEAS => 'p',
        awbrn_core::ShoalDirection::ANEASAW => 'q',
        awbrn_core::ShoalDirection::ANEASW => 'r',
        awbrn_core::ShoalDirection::ANEAW => 's',
        awbrn_core::ShoalDirection::ANES => 't',
        awbrn_core::ShoalDirection::ANESAW => 'u',
        awbrn_core::ShoalDirection::ANESW => 'v',
        awbrn_core::ShoalDirection::ANEW => 'w',
        awbrn_core::ShoalDirection::ANS => 'x',
        awbrn_core::ShoalDirection::ANSAW => 'y',
        awbrn_core::ShoalDirection::ANSW => 'z',
        awbrn_core::ShoalDirection::ANW => 'A',
        awbrn_core::ShoalDirection::AS => 'B',
        awbrn_core::ShoalDirection::ASAW => 'C',
        awbrn_core::ShoalDirection::ASW => 'D',
        awbrn_core::ShoalDirection::AW => 'E',
        awbrn_core::ShoalDirection::C => 'F',
        awbrn_core::ShoalDirection::E => 'G',
        awbrn_core::ShoalDirection::EAS => 'H',
        awbrn_core::ShoalDirection::EASAW => 'I',
        awbrn_core::ShoalDirection::EASW => 'J',
        awbrn_core::ShoalDirection::EAW => 'K',
        awbrn_core::ShoalDirection::ES => 'L',
        awbrn_core::ShoalDirection::ESAW => 'M',
        awbrn_core::ShoalDirection::ESW => 'N',
        awbrn_core::ShoalDirection::EW => 'O',
        awbrn_core::ShoalDirection::N => 'P',
        awbrn_core::ShoalDirection::NAE => 'Q',
        awbrn_core::ShoalDirection::NAEAS => 'R',
        awbrn_core::ShoalDirection::NAEASAW => 'S',
        awbrn_core::ShoalDirection::NAEASW => 'T',
        awbrn_core::ShoalDirection::NAEAW => 'U',
        awbrn_core::ShoalDirection::NAES => 'V',
        awbrn_core::ShoalDirection::NAESAW => 'W',
        awbrn_core::ShoalDirection::NAESW => 'X',
        awbrn_core::ShoalDirection::NAEW => 'Y',
        awbrn_core::ShoalDirection::NAS => 'Z',
        awbrn_core::ShoalDirection::NASAW => '0',
        awbrn_core::ShoalDirection::NASW => '!',
        awbrn_core::ShoalDirection::NAW => '@',
        awbrn_core::ShoalDirection::NE => '#',
        awbrn_core::ShoalDirection::NEAS => '$',
        awbrn_core::ShoalDirection::NEASAW => '%',
        awbrn_core::ShoalDirection::NEASW => '^',
        awbrn_core::ShoalDirection::NEAW => '&',
        awbrn_core::ShoalDirection::NES => '*',
        awbrn_core::ShoalDirection::NESAW => '(',
        awbrn_core::ShoalDirection::NESW => ')',
        awbrn_core::ShoalDirection::NEW => '-',
        awbrn_core::ShoalDirection::NS => '+',
        awbrn_core::ShoalDirection::NSAW => '=',
        awbrn_core::ShoalDirection::NSW => '[',
        awbrn_core::ShoalDirection::NW => ']',
        awbrn_core::ShoalDirection::S => '{',
        awbrn_core::ShoalDirection::SAW => '}',
        awbrn_core::ShoalDirection::SW => ':',
        awbrn_core::ShoalDirection::W => ';',
    }
}
