use crate::{
    AwbwTerrain, BridgeType, MissileSiloStatus, PipeRubbleType, PipeSeamType, PipeType, Property,
    RiverType, RoadType, ShoalType,
};

#[cfg(feature = "bevy")]
use bevy::reflect::ReflectSerialize;

/// Sea configurations based on the variants file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[expect(non_camel_case_types)]
pub enum SeaDirection {
    E,
    E_NW,
    E_NW_SW,
    E_S,
    E_S_NW,
    E_S_W,
    E_SW,
    E_W,
    N,
    N_E,
    N_E_S,
    N_E_S_W,
    N_E_SW,
    N_E_W,
    N_S,
    N_S_W,
    N_SE,
    N_SE_SW,
    N_SW,
    N_W,
    N_W_SE,
    NE,
    NE_SE,
    NE_SE_SW,
    NE_SW,
    NW,
    NW_NE,
    NW_NE_SE,
    NW_NE_SE_SW,
    NW_NE_SW,
    NW_SE,
    NW_SE_SW,
    NW_SW,
    S,
    S_E,
    S_NE,
    S_NW,
    S_NW_NE,
    S_W,
    S_W_NE,
    SE,
    SE_SW,
    SW,
    Sea,
    W,
    W_E,
    W_NE,
    W_NE_SE,
    W_SE,
}

/// Shoal configurations based on the variants file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum ShoalDirection {
    AE,
    AEAS,
    AEASAW,
    AEASW,
    AEAW,
    AES,
    AESAW,
    AESW,
    AEW,
    AN,
    ANAE,
    ANAEAS,
    ANAEASAW,
    ANAEASW,
    ANAEAW,
    ANAES,
    ANAESAW,
    ANAESW,
    ANAEW,
    ANAS,
    ANASAW,
    ANASW,
    ANAW,
    ANE,
    ANEAS,
    ANEASAW,
    ANEASW,
    ANEAW,
    ANES,
    ANESAW,
    ANESW,
    ANEW,
    ANS,
    ANSAW,
    ANSW,
    ANW,
    AS,
    ASAW,
    ASW,
    AW,
    C,
    E,
    EAS,
    EASAW,
    EASW,
    EAW,
    ES,
    ESAW,
    ESW,
    EW,
    N,
    NAE,
    NAEAS,
    NAEASAW,
    NAEASW,
    NAEAW,
    NAES,
    NAESAW,
    NAESW,
    NAEW,
    NAS,
    NASAW,
    NASW,
    NAW,
    NE,
    NEAS,
    NEASAW,
    NEASW,
    NEAW,
    NES,
    NESAW,
    NESW,
    NEW,
    NS,
    NSAW,
    NSW,
    NW,
    S,
    SAW,
    SW,
    W,
}

/// Terrain that represents the graphical representation. One can have tall
/// mountains and stubby mountains, but functionally they act the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
// Opaque because the AWBW terrain parts it holds come from `awbw-types`, which
// has no Bevy reflection.
#[cfg_attr(
    feature = "bevy",
    derive(bevy::reflect::Reflect),
    reflect(opaque, Serialize)
)]
pub enum GraphicalTerrain {
    // Basic terrains
    StubbyMoutain,
    Plain,
    Mountain,
    Wood,
    Reef,

    // Rivers with different configurations
    River(RiverType),

    // Roads with different configurations
    Road(RoadType),

    // Bridges
    Bridge(BridgeType),

    // Properties
    Property(Property),

    // Pipes and related structures
    Pipe(PipeType),
    PipeSeam(PipeSeamType),
    PipeRubble(PipeRubbleType),

    // Special terrains
    MissileSilo(MissileSiloStatus),
    Teleporter,

    // Sea and Shoal variants from the variants file
    Sea(SeaDirection),
    Shoal(ShoalDirection),
}

impl GraphicalTerrain {
    pub const fn as_terrain(self) -> AwbwTerrain {
        match self {
            // Basic terrains
            GraphicalTerrain::StubbyMoutain => AwbwTerrain::Mountain,
            GraphicalTerrain::Plain => AwbwTerrain::Plain,
            GraphicalTerrain::Mountain => AwbwTerrain::Mountain,
            GraphicalTerrain::Wood => AwbwTerrain::Wood,
            GraphicalTerrain::Reef => AwbwTerrain::Reef,

            // Rivers
            GraphicalTerrain::River(river_type) => AwbwTerrain::River(river_type),

            // Roads
            GraphicalTerrain::Road(road_type) => AwbwTerrain::Road(road_type),

            // Bridges
            GraphicalTerrain::Bridge(bridge_type) => AwbwTerrain::Bridge(bridge_type),

            // Properties
            GraphicalTerrain::Property(property) => AwbwTerrain::Property(property),

            // Pipes and related
            GraphicalTerrain::Pipe(pipe_type) => AwbwTerrain::Pipe(pipe_type),
            GraphicalTerrain::PipeSeam(pipe_seam_type) => AwbwTerrain::PipeSeam(pipe_seam_type),
            GraphicalTerrain::PipeRubble(pipe_rubble_type) => {
                AwbwTerrain::PipeRubble(pipe_rubble_type)
            }

            // Special terrains
            GraphicalTerrain::MissileSilo(status) => AwbwTerrain::MissileSilo(status),
            GraphicalTerrain::Teleporter => AwbwTerrain::Teleporter,

            // Sea variants
            GraphicalTerrain::Sea(_) => AwbwTerrain::Sea,
            // Shoal variants - for simplicity, mapping all to Horizontal for now
            // This would need refinement based on actual requirements
            GraphicalTerrain::Shoal(_) => AwbwTerrain::Shoal(ShoalType::Horizontal),
        }
    }

    /// Defense star bonus applied when a unit is on this terrain type.
    pub const fn defense_stars(self) -> u8 {
        match self {
            GraphicalTerrain::Plain | GraphicalTerrain::Reef => 1,
            GraphicalTerrain::Mountain | GraphicalTerrain::StubbyMoutain => 4,
            GraphicalTerrain::Wood => 2,
            GraphicalTerrain::Property(p) => p.defense_stars(),
            GraphicalTerrain::MissileSilo(_) => 3,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Faction, PlayerFaction};

    #[test]
    fn graphical_terrain_defense_stars() {
        assert_eq!(GraphicalTerrain::Plain.defense_stars(), 1);
        assert_eq!(GraphicalTerrain::Reef.defense_stars(), 1);
        assert_eq!(GraphicalTerrain::Mountain.defense_stars(), 4);
        assert_eq!(GraphicalTerrain::StubbyMoutain.defense_stars(), 4);
        assert_eq!(GraphicalTerrain::Wood.defense_stars(), 2);
        assert_eq!(
            GraphicalTerrain::Property(Property::HQ(PlayerFaction::OrangeStar)).defense_stars(),
            4
        );
        assert_eq!(
            GraphicalTerrain::Property(Property::City(Faction::Neutral)).defense_stars(),
            3
        );
        assert_eq!(
            GraphicalTerrain::MissileSilo(MissileSiloStatus::Loaded).defense_stars(),
            3
        );
        assert_eq!(
            GraphicalTerrain::Sea(SeaDirection::N_E_S_W).defense_stars(),
            0
        );
        assert_eq!(
            GraphicalTerrain::River(RiverType::Horizontal).defense_stars(),
            0
        );
        assert_eq!(
            GraphicalTerrain::Road(RoadType::Horizontal).defense_stars(),
            0
        );
    }
}
