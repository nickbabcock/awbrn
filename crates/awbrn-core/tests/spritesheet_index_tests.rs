use awbrn_core::{
    BridgeType, Faction, GraphicalMovement, GraphicalTerrain, MissileSiloStatus, PipeRubbleType,
    PipeSeamType, PipeType, PlayerFaction, Property, PropertyKind, RiverType, RoadType, ShoalType,
    Terrain, Unit, Weather, spritesheet_index, unit_spritesheet_index,
};
use insta::assert_json_snapshot;
use std::collections::{BTreeMap, HashMap};

// Helper function to generate all PlayerFaction variants
fn all_player_factions() -> Vec<PlayerFaction> {
    vec![
        PlayerFaction::AcidRain,
        PlayerFaction::AmberBlaze,
        PlayerFaction::AzureAsteroid,
        PlayerFaction::BlackHole,
        PlayerFaction::BlueMoon,
        PlayerFaction::BrownDesert,
        PlayerFaction::CobaltIce,
        PlayerFaction::GreenEarth,
        PlayerFaction::GreySky,
        PlayerFaction::JadeSun,
        PlayerFaction::NoirEclipse,
        PlayerFaction::OrangeStar,
        PlayerFaction::PinkCosmos,
        PlayerFaction::PurpleLightning,
        PlayerFaction::RedFire,
        PlayerFaction::SilverClaw,
        PlayerFaction::TealGalaxy,
        PlayerFaction::WhiteNova,
        PlayerFaction::YellowComet,
    ]
}

// Helper function to generate all PropertyKind variants
fn all_property_kinds() -> Vec<PropertyKind> {
    vec![
        PropertyKind::Airport,
        PropertyKind::Base,
        PropertyKind::City,
        PropertyKind::ComTower,
        PropertyKind::HQ,
        PropertyKind::Lab,
        PropertyKind::Port,
    ]
}

// Helper function to generate all RiverType variants
fn all_river_types() -> Vec<RiverType> {
    vec![
        RiverType::Cross,
        RiverType::ES,
        RiverType::ESW,
        RiverType::Horizontal,
        RiverType::NE,
        RiverType::NES,
        RiverType::SW,
        RiverType::SWN,
        RiverType::Vertical,
        RiverType::WN,
        RiverType::WNE,
    ]
}

// Helper function to generate all RoadType variants
fn all_road_types() -> Vec<RoadType> {
    vec![
        RoadType::Cross,
        RoadType::ES,
        RoadType::ESW,
        RoadType::Horizontal,
        RoadType::NE,
        RoadType::NES,
        RoadType::SW,
        RoadType::SWN,
        RoadType::Vertical,
        RoadType::WN,
        RoadType::WNE,
    ]
}

// Helper function to generate all BridgeType variants
fn all_bridge_types() -> Vec<BridgeType> {
    vec![BridgeType::Horizontal, BridgeType::Vertical]
}

// Helper function to generate all ShoalType variants
fn all_shoal_types() -> Vec<ShoalType> {
    vec![
        ShoalType::Horizontal,
        ShoalType::HorizontalNorth,
        ShoalType::Vertical,
        ShoalType::VerticalEast,
    ]
}

// Helper function to generate all PipeType variants
fn all_pipe_types() -> Vec<PipeType> {
    vec![
        PipeType::Vertical,
        PipeType::Horizontal,
        PipeType::NE,
        PipeType::ES,
        PipeType::SW,
        PipeType::WN,
        PipeType::NorthEnd,
        PipeType::EastEnd,
        PipeType::SouthEnd,
        PipeType::WestEnd,
    ]
}

// Helper function to generate all MissileSiloStatus variants
fn all_missile_silo_statuses() -> Vec<MissileSiloStatus> {
    vec![MissileSiloStatus::Loaded, MissileSiloStatus::Unloaded]
}

// Helper function to generate all PipeSeamType variants
fn all_pipe_seam_types() -> Vec<PipeSeamType> {
    vec![PipeSeamType::Horizontal, PipeSeamType::Vertical]
}

// Helper function to generate all PipeRubbleType variants
fn all_pipe_rubble_types() -> Vec<PipeRubbleType> {
    vec![PipeRubbleType::Horizontal, PipeRubbleType::Vertical]
}

// Helper to create Property instances from PropertyKind
fn create_property(kind: &PropertyKind, faction: Faction) -> Property {
    match kind {
        PropertyKind::Airport => Property::Airport(faction),
        PropertyKind::Base => Property::Base(faction),
        PropertyKind::City => Property::City(faction),
        PropertyKind::ComTower => Property::ComTower(faction),
        PropertyKind::HQ => match faction {
            Faction::Player(pf) => Property::HQ(pf),
            Faction::Neutral => panic!("Neutral HQ is not a valid property"),
        },
        PropertyKind::Lab => Property::Lab(faction),
        PropertyKind::Port => Property::Port(faction),
    }
}

// Generates all variants of GraphicalTerrain
fn get_all_graphical_terrains() -> Vec<GraphicalTerrain> {
    let mut terrains = Vec::new();

    terrains.push(GraphicalTerrain::StubbyMoutain);

    // Basic terrains
    terrains.push(GraphicalTerrain::Terrain(Terrain::Plain));
    terrains.push(GraphicalTerrain::Terrain(Terrain::Mountain));
    terrains.push(GraphicalTerrain::Terrain(Terrain::Wood));
    terrains.push(GraphicalTerrain::Terrain(Terrain::Sea));
    terrains.push(GraphicalTerrain::Terrain(Terrain::Reef));
    terrains.push(GraphicalTerrain::Terrain(Terrain::Teleporter));

    // Rivers
    for river_type in all_river_types() {
        terrains.push(GraphicalTerrain::Terrain(Terrain::River(river_type)));
    }

    // Roads
    for road_type in all_road_types() {
        terrains.push(GraphicalTerrain::Terrain(Terrain::Road(road_type)));
    }

    // Bridges
    for bridge_type in all_bridge_types() {
        terrains.push(GraphicalTerrain::Terrain(Terrain::Bridge(bridge_type)));
    }

    // Shoals
    for shoal_type in all_shoal_types() {
        terrains.push(GraphicalTerrain::Terrain(Terrain::Shoal(shoal_type)));
    }

    // Pipes
    for pipe_type in all_pipe_types() {
        terrains.push(GraphicalTerrain::Terrain(Terrain::Pipe(pipe_type)));
    }

    // Missile Silos
    for status in all_missile_silo_statuses() {
        terrains.push(GraphicalTerrain::Terrain(Terrain::MissileSilo(status)));
    }

    // Pipe Seams
    for seam_type in all_pipe_seam_types() {
        terrains.push(GraphicalTerrain::Terrain(Terrain::PipeSeam(seam_type)));
    }

    // Pipe Rubble
    for rubble_type in all_pipe_rubble_types() {
        terrains.push(GraphicalTerrain::Terrain(Terrain::PipeRubble(rubble_type)));
    }

    // Properties
    let player_factions = all_player_factions();
    let property_kinds = all_property_kinds();

    for kind in &property_kinds {
        // Neutral Properties (except HQ)
        if *kind != PropertyKind::HQ {
            terrains.push(GraphicalTerrain::Terrain(Terrain::Property(
                create_property(kind, Faction::Neutral),
            )));
        }

        // Player Properties
        for faction in &player_factions {
            terrains.push(GraphicalTerrain::Terrain(Terrain::Property(
                create_property(kind, Faction::Player(*faction)),
            )));
        }
    }

    terrains
}

// Helper function to generate all Unit variants
fn all_units() -> Vec<Unit> {
    vec![
        Unit::AntiAir,
        Unit::APC,
        Unit::Artillery,
        Unit::BCopter,
        Unit::Battleship,
        Unit::BlackBoat,
        Unit::BlackBomb,
        Unit::Bomber,
        Unit::Carrier,
        Unit::Cruiser,
        Unit::Fighter,
        Unit::Infantry,
        Unit::Lander,
        Unit::MdTank,
        Unit::Mech,
        Unit::MegaTank,
        Unit::Missile,
        Unit::Neotank,
        Unit::Piperunner,
        Unit::Recon,
        Unit::Rocket,
        Unit::Stealth,
        Unit::Sub,
        Unit::TCopter,
        Unit::Tank,
    ]
}

// Helper function to generate all GraphicalMovement variants
fn all_movements() -> Vec<GraphicalMovement> {
    vec![
        GraphicalMovement::None,
        GraphicalMovement::Up,
        GraphicalMovement::Down,
        GraphicalMovement::Lateral,
    ]
}

#[test]
fn snapshot_all_sprite_indices() {
    // Test for all weather types
    let weather_types = vec![Weather::Clear, Weather::Snow, Weather::Rain];

    // Get all terrain types
    let all_terrains = get_all_graphical_terrains();

    // Create a structured representation for snapshot testing
    let mut snapshot_data = BTreeMap::new();

    for weather in weather_types {
        let weather_name = format!("{:?}", weather);
        let mut terrain_indices = BTreeMap::new();

        for terrain in &all_terrains {
            let sprite = spritesheet_index(weather, *terrain);
            let terrain_name = format!("{:?}", terrain);

            terrain_indices.insert(terrain_name, (sprite.index(), sprite.animation_frames()));
        }

        snapshot_data.insert(weather_name, terrain_indices);
    }

    // Create a snapshot of all sprite indices
    assert_json_snapshot!("sprite_indices", snapshot_data);
}

#[test]
fn no_overlapping_indices_clear_weather() {
    let all_terrains = get_all_graphical_terrains();
    let mut terrain_map = HashMap::new();

    for terrain in all_terrains {
        let sprite = spritesheet_index(Weather::Clear, terrain);
        let start_index = sprite.index();
        let end_index = start_index + sprite.animation_frames() as u16;

        for i in start_index..end_index {
            if let Some(existing_terrain) = terrain_map.insert(i, terrain) {
                panic!(
                    "Overlap detected! Index {} is used by {:?} (Sprite: {:?}) and {:?} (Sprite: {:?})",
                    i,
                    terrain,
                    sprite,
                    existing_terrain,
                    spritesheet_index(Weather::Clear, existing_terrain)
                );
            }
        }
    }
}

#[test]
fn snapshot_all_unit_sprite_indices() {
    // Get all possible combinations of parameters
    let all_movements = all_movements();
    let all_unit_types = all_units();
    let all_factions = all_player_factions();

    // Create a structured representation for snapshot testing
    let mut snapshot_data = BTreeMap::new();

    for movement in &all_movements {
        let movement_name = format!("{:?}", movement);
        let mut faction_indices = BTreeMap::new();

        for faction in &all_factions {
            let faction_name = format!("{:?}", faction);
            let mut unit_indices = BTreeMap::new();

            for unit in &all_unit_types {
                let sprite = unit_spritesheet_index(*movement, *unit, *faction);
                let unit_name = format!("{:?}", unit);

                unit_indices.insert(unit_name, (sprite.index(), sprite.animation_frames()));
            }

            faction_indices.insert(faction_name, unit_indices);
        }

        snapshot_data.insert(movement_name, faction_indices);
    }

    // Create a snapshot of all unit sprite indices
    assert_json_snapshot!("unit_sprite_indices", snapshot_data);
}

#[test]
fn no_overlapping_unit_indices() {
    let all_movements = all_movements();
    let all_unit_types = all_units();
    let all_factions = all_player_factions();
    let mut index_map = HashMap::new();

    for movement in &all_movements {
        for faction in &all_factions {
            for unit in &all_unit_types {
                let sprite = unit_spritesheet_index(*movement, *unit, *faction);
                let start_index = sprite.index();
                let end_index = start_index + sprite.animation_frames() as u16;
                let current_key = format!("{:?}_{:?}_{:?}", movement, unit, faction);

                for i in start_index..end_index {
                    if let Some((existing_key, existing_sprite)) =
                        index_map.insert(i, (current_key.clone(), sprite))
                    {
                        panic!(
                            "Overlap detected! Index {} is used by {} (Sprite: {:?}) and {} (Sprite: {:?})",
                            i, current_key, sprite, existing_key, existing_sprite
                        );
                    }
                }
            }
        }
    }
}
