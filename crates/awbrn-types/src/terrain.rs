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

impl ShoalDirection {
    /// The sides of the tile that land lies against, as north, east, south and
    /// west.
    ///
    /// The name of a variant lists the sides in that order: a bare side letter
    /// is land, a side after an `A` is open water, and a side that is absent
    /// holds more shoal.
    pub const fn land_sides(self) -> [bool; 4] {
        match self {
            ShoalDirection::AE => [false, false, false, false],
            ShoalDirection::AEAS => [false, false, false, false],
            ShoalDirection::AEASAW => [false, false, false, false],
            ShoalDirection::AEASW => [false, false, false, true],
            ShoalDirection::AEAW => [false, false, false, false],
            ShoalDirection::AES => [false, false, true, false],
            ShoalDirection::AESAW => [false, false, true, false],
            ShoalDirection::AESW => [false, false, true, true],
            ShoalDirection::AEW => [false, false, false, true],
            ShoalDirection::AN => [false, false, false, false],
            ShoalDirection::ANAE => [false, false, false, false],
            ShoalDirection::ANAEAS => [false, false, false, false],
            ShoalDirection::ANAEASAW => [false, false, false, false],
            ShoalDirection::ANAEASW => [false, false, false, true],
            ShoalDirection::ANAEAW => [false, false, false, false],
            ShoalDirection::ANAES => [false, false, true, false],
            ShoalDirection::ANAESAW => [false, false, true, false],
            ShoalDirection::ANAESW => [false, false, true, true],
            ShoalDirection::ANAEW => [false, false, false, true],
            ShoalDirection::ANAS => [false, false, false, false],
            ShoalDirection::ANASAW => [false, false, false, false],
            ShoalDirection::ANASW => [false, false, false, true],
            ShoalDirection::ANAW => [false, false, false, false],
            ShoalDirection::ANE => [false, true, false, false],
            ShoalDirection::ANEAS => [false, true, false, false],
            ShoalDirection::ANEASAW => [false, true, false, false],
            ShoalDirection::ANEASW => [false, true, false, true],
            ShoalDirection::ANEAW => [false, true, false, false],
            ShoalDirection::ANES => [false, true, true, false],
            ShoalDirection::ANESAW => [false, true, true, false],
            ShoalDirection::ANESW => [false, true, true, true],
            ShoalDirection::ANEW => [false, true, false, true],
            ShoalDirection::ANS => [false, false, true, false],
            ShoalDirection::ANSAW => [false, false, true, false],
            ShoalDirection::ANSW => [false, false, true, true],
            ShoalDirection::ANW => [false, false, false, true],
            ShoalDirection::AS => [false, false, false, false],
            ShoalDirection::ASAW => [false, false, false, false],
            ShoalDirection::ASW => [false, false, false, true],
            ShoalDirection::AW => [false, false, false, false],
            ShoalDirection::C => [false, false, false, false],
            ShoalDirection::E => [false, true, false, false],
            ShoalDirection::EAS => [false, true, false, false],
            ShoalDirection::EASAW => [false, true, false, false],
            ShoalDirection::EASW => [false, true, false, true],
            ShoalDirection::EAW => [false, true, false, false],
            ShoalDirection::ES => [false, true, true, false],
            ShoalDirection::ESAW => [false, true, true, false],
            ShoalDirection::ESW => [false, true, true, true],
            ShoalDirection::EW => [false, true, false, true],
            ShoalDirection::N => [true, false, false, false],
            ShoalDirection::NAE => [true, false, false, false],
            ShoalDirection::NAEAS => [true, false, false, false],
            ShoalDirection::NAEASAW => [true, false, false, false],
            ShoalDirection::NAEASW => [true, false, false, true],
            ShoalDirection::NAEAW => [true, false, false, false],
            ShoalDirection::NAES => [true, false, true, false],
            ShoalDirection::NAESAW => [true, false, true, false],
            ShoalDirection::NAESW => [true, false, true, true],
            ShoalDirection::NAEW => [true, false, false, true],
            ShoalDirection::NAS => [true, false, false, false],
            ShoalDirection::NASAW => [true, false, false, false],
            ShoalDirection::NASW => [true, false, false, true],
            ShoalDirection::NAW => [true, false, false, false],
            ShoalDirection::NE => [true, true, false, false],
            ShoalDirection::NEAS => [true, true, false, false],
            ShoalDirection::NEASAW => [true, true, false, false],
            ShoalDirection::NEASW => [true, true, false, true],
            ShoalDirection::NEAW => [true, true, false, false],
            ShoalDirection::NES => [true, true, true, false],
            ShoalDirection::NESAW => [true, true, true, false],
            ShoalDirection::NESW => [true, true, true, true],
            ShoalDirection::NEW => [true, true, false, true],
            ShoalDirection::NS => [true, false, true, false],
            ShoalDirection::NSAW => [true, false, true, false],
            ShoalDirection::NSW => [true, false, true, true],
            ShoalDirection::NW => [true, false, false, true],
            ShoalDirection::S => [false, false, true, false],
            ShoalDirection::SAW => [false, false, true, false],
            ShoalDirection::SW => [false, false, true, true],
            ShoalDirection::W => [false, false, false, true],
        }
    }

    /// The variant that records the shoal the client draws.
    ///
    /// The picture and the document then name the same land, because they are
    /// the same answer read twice.
    pub const fn shoal_type(self) -> ShoalType {
        let sides = self.land_sides();
        ShoalType::from_land(sides[0], sides[1], sides[2], sides[3])
    }
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
