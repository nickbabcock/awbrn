use awbrn_map::Position;
use awbrn_types::GraphicalTerrain;
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
                awbrn_types::RiverType::Horizontal => 'H',
                awbrn_types::RiverType::Vertical => 'V',
                awbrn_types::RiverType::Cross => 'C',
                awbrn_types::RiverType::ES => 'E',
                awbrn_types::RiverType::SW => 'S',
                awbrn_types::RiverType::WN => 'W',
                awbrn_types::RiverType::NE => 'N',
                awbrn_types::RiverType::ESW => '1',
                awbrn_types::RiverType::SWN => '2',
                awbrn_types::RiverType::WNE => '3',
                awbrn_types::RiverType::NES => '4',
            };
            format!("R{}", second_char)
        }

        // Roads with different configurations
        GraphicalTerrain::Road(road_type) => {
            let second_char = match road_type {
                awbrn_types::RoadType::Horizontal => 'H',
                awbrn_types::RoadType::Vertical => 'V',
                awbrn_types::RoadType::Cross => 'C',
                awbrn_types::RoadType::ES => 'E',
                awbrn_types::RoadType::SW => 'S',
                awbrn_types::RoadType::WN => 'W',
                awbrn_types::RoadType::NE => 'N',
                awbrn_types::RoadType::ESW => '1',
                awbrn_types::RoadType::SWN => '2',
                awbrn_types::RoadType::WNE => '3',
                awbrn_types::RoadType::NES => '4',
            };
            format!("D{}", second_char)
        }

        // Bridges
        GraphicalTerrain::Bridge(bridge_type) => {
            let second_char = match bridge_type {
                awbrn_types::BridgeType::Horizontal => 'H',
                awbrn_types::BridgeType::Vertical => 'V',
            };
            format!("B{}", second_char)
        }

        // Properties
        GraphicalTerrain::Property(property) => match property {
            awbrn_types::Property::City(faction) => {
                let faction_char = match faction {
                    awbrn_types::Faction::Neutral => 'N',
                    awbrn_types::Faction::Player(player) => player_faction_to_char(player),
                };
                format!("C{}", faction_char)
            }
            awbrn_types::Property::Base(faction) => {
                let faction_char = match faction {
                    awbrn_types::Faction::Neutral => 'N',
                    awbrn_types::Faction::Player(player) => player_faction_to_char(player),
                };
                format!("B{}", faction_char)
            }
            awbrn_types::Property::Airport(faction) => {
                let faction_char = match faction {
                    awbrn_types::Faction::Neutral => 'N',
                    awbrn_types::Faction::Player(player) => player_faction_to_char(player),
                };
                format!("A{}", faction_char)
            }
            awbrn_types::Property::Port(faction) => {
                let faction_char = match faction {
                    awbrn_types::Faction::Neutral => 'N',
                    awbrn_types::Faction::Player(player) => player_faction_to_char(player),
                };
                format!("P{}", faction_char)
            }
            awbrn_types::Property::ComTower(faction) => {
                let faction_char = match faction {
                    awbrn_types::Faction::Neutral => 'N',
                    awbrn_types::Faction::Player(player) => player_faction_to_char(player),
                };
                format!("T{}", faction_char)
            }
            awbrn_types::Property::Lab(faction) => {
                let faction_char = match faction {
                    awbrn_types::Faction::Neutral => 'N',
                    awbrn_types::Faction::Player(player) => player_faction_to_char(player),
                };
                format!("L{}", faction_char)
            }
            awbrn_types::Property::HQ(player_faction) => {
                format!("H{}", player_faction_to_char(player_faction))
            }
        },

        // Pipes and related structures
        GraphicalTerrain::Pipe(pipe_type) => {
            let second_char = match pipe_type {
                awbrn_types::PipeType::Vertical => 'V',
                awbrn_types::PipeType::Horizontal => 'H',
                awbrn_types::PipeType::NE => 'N',
                awbrn_types::PipeType::ES => 'E',
                awbrn_types::PipeType::SW => 'S',
                awbrn_types::PipeType::WN => 'W',
                awbrn_types::PipeType::NorthEnd => '1',
                awbrn_types::PipeType::EastEnd => '2',
                awbrn_types::PipeType::SouthEnd => '3',
                awbrn_types::PipeType::WestEnd => '4',
            };
            format!("P{}", second_char)
        }
        GraphicalTerrain::PipeSeam(seam_type) => {
            let second_char = match seam_type {
                awbrn_types::PipeSeamType::Horizontal => 'H',
                awbrn_types::PipeSeamType::Vertical => 'V',
            };
            format!("S{}", second_char)
        }
        GraphicalTerrain::PipeRubble(rubble_type) => {
            let second_char = match rubble_type {
                awbrn_types::PipeRubbleType::Horizontal => 'H',
                awbrn_types::PipeRubbleType::Vertical => 'V',
            };
            format!("U{}", second_char)
        }

        // Special terrains
        GraphicalTerrain::MissileSilo(status) => {
            let second_char = match status {
                awbrn_types::MissileSiloStatus::Loaded => 'L',
                awbrn_types::MissileSiloStatus::Unloaded => 'U',
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
fn player_faction_to_char(player_faction: awbrn_types::PlayerFaction) -> char {
    match player_faction {
        awbrn_types::PlayerFaction::OrangeStar => 'O',
        awbrn_types::PlayerFaction::BlueMoon => 'B',
        awbrn_types::PlayerFaction::GreenEarth => 'G',
        awbrn_types::PlayerFaction::YellowComet => 'Y',
        awbrn_types::PlayerFaction::BlackHole => 'K',
        awbrn_types::PlayerFaction::RedFire => 'R',
        awbrn_types::PlayerFaction::GreySky => 'S',
        awbrn_types::PlayerFaction::BrownDesert => 'D',
        awbrn_types::PlayerFaction::AmberBlaze => 'A',
        awbrn_types::PlayerFaction::JadeSun => 'J',
        awbrn_types::PlayerFaction::CobaltIce => 'C',
        awbrn_types::PlayerFaction::PinkCosmos => 'P',
        awbrn_types::PlayerFaction::TealGalaxy => 'T',
        awbrn_types::PlayerFaction::PurpleLightning => 'L',
        awbrn_types::PlayerFaction::AcidRain => 'I',
        awbrn_types::PlayerFaction::UmberWilds => 'U',
        awbrn_types::PlayerFaction::WhiteNova => 'W',
        awbrn_types::PlayerFaction::AzureAsteroid => 'Z',
        awbrn_types::PlayerFaction::NoirEclipse => 'E',
        awbrn_types::PlayerFaction::SilverClaw => 'V',
    }
}

/// Convert a SeaDirection to a unique index
fn sea_direction_to_index(direction: awbrn_types::SeaDirection) -> char {
    match direction {
        awbrn_types::SeaDirection::E => '1',
        awbrn_types::SeaDirection::E_NW => '2',
        awbrn_types::SeaDirection::E_NW_SW => '3',
        awbrn_types::SeaDirection::E_S => '4',
        awbrn_types::SeaDirection::E_S_NW => '5',
        awbrn_types::SeaDirection::E_S_W => '6',
        awbrn_types::SeaDirection::E_SW => '7',
        awbrn_types::SeaDirection::E_W => '8',
        awbrn_types::SeaDirection::N => '9',
        awbrn_types::SeaDirection::N_E => 'a',
        awbrn_types::SeaDirection::N_E_S => 'b',
        awbrn_types::SeaDirection::N_E_S_W => 'c',
        awbrn_types::SeaDirection::N_E_SW => 'd',
        awbrn_types::SeaDirection::N_E_W => 'e',
        awbrn_types::SeaDirection::N_S => 'f',
        awbrn_types::SeaDirection::N_S_W => 'g',
        awbrn_types::SeaDirection::N_SE => 'h',
        awbrn_types::SeaDirection::N_SE_SW => 'i',
        awbrn_types::SeaDirection::N_SW => 'j',
        awbrn_types::SeaDirection::N_W => 'k',
        awbrn_types::SeaDirection::N_W_SE => 'l',
        awbrn_types::SeaDirection::NE => 'm',
        awbrn_types::SeaDirection::NE_SE => 'n',
        awbrn_types::SeaDirection::NE_SE_SW => 'o',
        awbrn_types::SeaDirection::NE_SW => 'p',
        awbrn_types::SeaDirection::NW => 'q',
        awbrn_types::SeaDirection::NW_NE => 'r',
        awbrn_types::SeaDirection::NW_NE_SE => 's',
        awbrn_types::SeaDirection::NW_NE_SE_SW => 't',
        awbrn_types::SeaDirection::NW_NE_SW => 'u',
        awbrn_types::SeaDirection::NW_SE => 'v',
        awbrn_types::SeaDirection::NW_SE_SW => 'w',
        awbrn_types::SeaDirection::NW_SW => 'x',
        awbrn_types::SeaDirection::S => 'y',
        awbrn_types::SeaDirection::S_E => 'z',
        awbrn_types::SeaDirection::S_NE => 'A',
        awbrn_types::SeaDirection::S_NW => 'B',
        awbrn_types::SeaDirection::S_NW_NE => 'C',
        awbrn_types::SeaDirection::S_W => 'D',
        awbrn_types::SeaDirection::S_W_NE => 'E',
        awbrn_types::SeaDirection::SE => 'F',
        awbrn_types::SeaDirection::SE_SW => 'G',
        awbrn_types::SeaDirection::SW => 'H',
        awbrn_types::SeaDirection::Sea => '0',
        awbrn_types::SeaDirection::W => 'I',
        awbrn_types::SeaDirection::W_E => 'J',
        awbrn_types::SeaDirection::W_NE => 'K',
        awbrn_types::SeaDirection::W_NE_SE => 'L',
        awbrn_types::SeaDirection::W_SE => 'M',
    }
}

/// Convert a ShoalDirection to a unique index
fn shoal_direction_to_index(direction: awbrn_types::ShoalDirection) -> char {
    match direction {
        awbrn_types::ShoalDirection::AE => '1',
        awbrn_types::ShoalDirection::AEAS => '2',
        awbrn_types::ShoalDirection::AEASAW => '3',
        awbrn_types::ShoalDirection::AEASW => '4',
        awbrn_types::ShoalDirection::AEAW => '5',
        awbrn_types::ShoalDirection::AES => '6',
        awbrn_types::ShoalDirection::AESAW => '7',
        awbrn_types::ShoalDirection::AESW => '8',
        awbrn_types::ShoalDirection::AEW => '9',
        awbrn_types::ShoalDirection::AN => 'a',
        awbrn_types::ShoalDirection::ANAE => 'b',
        awbrn_types::ShoalDirection::ANAEAS => 'c',
        awbrn_types::ShoalDirection::ANAEASAW => 'd',
        awbrn_types::ShoalDirection::ANAEASW => 'e',
        awbrn_types::ShoalDirection::ANAEAW => 'f',
        awbrn_types::ShoalDirection::ANAES => 'g',
        awbrn_types::ShoalDirection::ANAESAW => 'h',
        awbrn_types::ShoalDirection::ANAESW => 'i',
        awbrn_types::ShoalDirection::ANAEW => 'j',
        awbrn_types::ShoalDirection::ANAS => 'k',
        awbrn_types::ShoalDirection::ANASAW => 'l',
        awbrn_types::ShoalDirection::ANASW => 'm',
        awbrn_types::ShoalDirection::ANAW => 'n',
        awbrn_types::ShoalDirection::ANE => 'o',
        awbrn_types::ShoalDirection::ANEAS => 'p',
        awbrn_types::ShoalDirection::ANEASAW => 'q',
        awbrn_types::ShoalDirection::ANEASW => 'r',
        awbrn_types::ShoalDirection::ANEAW => 's',
        awbrn_types::ShoalDirection::ANES => 't',
        awbrn_types::ShoalDirection::ANESAW => 'u',
        awbrn_types::ShoalDirection::ANESW => 'v',
        awbrn_types::ShoalDirection::ANEW => 'w',
        awbrn_types::ShoalDirection::ANS => 'x',
        awbrn_types::ShoalDirection::ANSAW => 'y',
        awbrn_types::ShoalDirection::ANSW => 'z',
        awbrn_types::ShoalDirection::ANW => 'A',
        awbrn_types::ShoalDirection::AS => 'B',
        awbrn_types::ShoalDirection::ASAW => 'C',
        awbrn_types::ShoalDirection::ASW => 'D',
        awbrn_types::ShoalDirection::AW => 'E',
        awbrn_types::ShoalDirection::C => 'F',
        awbrn_types::ShoalDirection::E => 'G',
        awbrn_types::ShoalDirection::EAS => 'H',
        awbrn_types::ShoalDirection::EASAW => 'I',
        awbrn_types::ShoalDirection::EASW => 'J',
        awbrn_types::ShoalDirection::EAW => 'K',
        awbrn_types::ShoalDirection::ES => 'L',
        awbrn_types::ShoalDirection::ESAW => 'M',
        awbrn_types::ShoalDirection::ESW => 'N',
        awbrn_types::ShoalDirection::EW => 'O',
        awbrn_types::ShoalDirection::N => 'P',
        awbrn_types::ShoalDirection::NAE => 'Q',
        awbrn_types::ShoalDirection::NAEAS => 'R',
        awbrn_types::ShoalDirection::NAEASAW => 'S',
        awbrn_types::ShoalDirection::NAEASW => 'T',
        awbrn_types::ShoalDirection::NAEAW => 'U',
        awbrn_types::ShoalDirection::NAES => 'V',
        awbrn_types::ShoalDirection::NAESAW => 'W',
        awbrn_types::ShoalDirection::NAESW => 'X',
        awbrn_types::ShoalDirection::NAEW => 'Y',
        awbrn_types::ShoalDirection::NAS => 'Z',
        awbrn_types::ShoalDirection::NASAW => '0',
        awbrn_types::ShoalDirection::NASW => '!',
        awbrn_types::ShoalDirection::NAW => '@',
        awbrn_types::ShoalDirection::NE => '#',
        awbrn_types::ShoalDirection::NEAS => '$',
        awbrn_types::ShoalDirection::NEASAW => '%',
        awbrn_types::ShoalDirection::NEASW => '^',
        awbrn_types::ShoalDirection::NEAW => '&',
        awbrn_types::ShoalDirection::NES => '*',
        awbrn_types::ShoalDirection::NESAW => '(',
        awbrn_types::ShoalDirection::NESW => ')',
        awbrn_types::ShoalDirection::NEW => '-',
        awbrn_types::ShoalDirection::NS => '+',
        awbrn_types::ShoalDirection::NSAW => '=',
        awbrn_types::ShoalDirection::NSW => '[',
        awbrn_types::ShoalDirection::NW => ']',
        awbrn_types::ShoalDirection::S => '{',
        awbrn_types::ShoalDirection::SAW => '}',
        awbrn_types::ShoalDirection::SW => ':',
        awbrn_types::ShoalDirection::W => ';',
    }
}
