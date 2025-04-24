use crate::{
    BridgeType, Faction, GraphicalTerrain, MissileSiloStatus, PipeRubbleType, PipeSeamType,
    PipeType, PlayerFaction, Property, PropertyKind, RiverType, RoadType, ShoalType, Terrain,
    Weather,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Default)]
pub struct SpritesheetIndex {
    index: u16,
    animation_frames: u8,
}

impl SpritesheetIndex {
    #[inline]
    const fn single_frame(index: u16) -> Self {
        Self::new(index, 1)
    }

    #[inline]
    pub const fn new(index: u16, animation_frames: u8) -> Self {
        Self {
            index,
            animation_frames,
        }
    }

    #[inline]
    pub const fn index(&self) -> u16 {
        self.index
    }

    #[inline]
    pub const fn animation_frames(&self) -> u8 {
        self.animation_frames
    }

    #[inline]
    const fn following(&self, animation_frames: u8) -> SpritesheetIndex {
        Self {
            index: self.index() + self.animation_frames() as u16,
            animation_frames,
        }
    }
}

#[rustfmt::skip]
pub const fn spritesheet_index(weather: Weather, terrain: GraphicalTerrain) -> SpritesheetIndex {
    match terrain {
        GraphicalTerrain::StubbyMoutain => match weather {
            Weather::Clear | Weather::Rain => SpritesheetIndex::single_frame(6),
            Weather::Snow => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Wood)).following(1),
        },
        GraphicalTerrain::Terrain(terrain) => match terrain {
            Terrain::Plain => {
                spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Mountain)).following(1)
            }
            Terrain::Mountain => match weather {
                Weather::Clear => SpritesheetIndex::single_frame(1),
                Weather::Snow => spritesheet_index(Weather::Clear, GraphicalTerrain::Terrain(Terrain::Property(Property::Port(Faction::Player(PlayerFaction::YellowComet))))).following(1),
                Weather::Rain => spritesheet_index(Weather::Snow, GraphicalTerrain::Terrain(Terrain::Property(Property::Port(Faction::Player(PlayerFaction::YellowComet))))).following(1),
            }
            Terrain::Wood => match weather {
                Weather::Clear => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Teleporter)).following(1),
                Weather::Snow | Weather::Rain => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Plain)).following(1),
            },
            Terrain::River(river_type) => match river_type {
                RiverType::Cross => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Property(Property::Port(Faction::Player(PlayerFaction::RedFire))))).following(1),
                RiverType::ES => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::River(RiverType::Cross))).following(1),
                RiverType::ESW => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::River(RiverType::ES))).following(1),
                RiverType::Horizontal => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::River(RiverType::ESW))).following(1),
                RiverType::NE => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::River(RiverType::Horizontal))).following(1),
                RiverType::NES => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::River(RiverType::NE))).following(1),
                RiverType::SWN => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::River(RiverType::NES))).following(1),
                RiverType::SW => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::River(RiverType::SWN))).following(1),
                RiverType::Vertical => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::River(RiverType::SW))).following(1),
                RiverType::WNE => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::River(RiverType::Vertical))).following(1),
                RiverType::WN => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::River(RiverType::WNE))).following(1),
            },
            Terrain::Road(road_type) => match road_type {
                RoadType::Cross => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::River(RiverType::WN))).following(1),
                RoadType::ES => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Road(RoadType::Cross))).following(1),
                RoadType::ESW => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Road(RoadType::ES))).following(1),
                RoadType::Horizontal => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Bridge(BridgeType::Horizontal))).following(1),
                RoadType::NE => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Road(RoadType::Horizontal))).following(1),
                RoadType::NES => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Road(RoadType::NE))).following(1),
                RoadType::SWN => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Road(RoadType::NES))).following(1),
                RoadType::SW => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Road(RoadType::SWN))).following(1),
                RoadType::Vertical => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Bridge(BridgeType::Vertical))).following(1),
                RoadType::WNE => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Road(RoadType::Vertical))).following(1),
                RoadType::WN => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Road(RoadType::WNE))).following(1),
            },
            Terrain::Bridge(bridge_type) => match bridge_type {
                BridgeType::Horizontal => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Road(RoadType::ESW))).following(1),
                BridgeType::Vertical => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Road(RoadType::SW))).following(1),
            },
            Terrain::Sea => SpritesheetIndex::single_frame(450),
            Terrain::Shoal(shoal_type) => match shoal_type {
                ShoalType::Horizontal => SpritesheetIndex::new(558, 1),
                ShoalType::HorizontalNorth => SpritesheetIndex::new(563, 1),
                ShoalType::Vertical => SpritesheetIndex::new(511, 1),
                ShoalType::VerticalEast => SpritesheetIndex::new(505, 1),
            },
            Terrain::Reef => {
                SpritesheetIndex::single_frame(3)
            }
            Terrain::Property(property) => match property {
                Property::Airport(Faction::Neutral) => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Property(Property::Port(Faction::Player(PlayerFaction::JadeSun))))).following(1),
                Property::Base(Faction::Neutral) =>  spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Property(Property::Airport(Faction::Neutral)))).following(1),
                Property::City(Faction::Neutral) => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Property(Property::Base(Faction::Neutral)))).following(1),
                Property::ComTower(Faction::Neutral) => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Property(Property::City(Faction::Neutral)))).following(1),
                Property::Lab(Faction::Neutral) => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::PipeSeam(PipeSeamType::Horizontal))).following(1),
                Property::Port(Faction::Neutral) => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Property(Property::Lab(Faction::Neutral)))).following(1),

                Property::Airport(Faction::Player(PlayerFaction::AcidRain)) => match weather {
                    Weather::Clear | Weather::Snow => spritesheet_index(weather, GraphicalTerrain::StubbyMoutain).following(3),
                    Weather::Rain => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Wood)).following(3),
                },
                Property::Airport(Faction::Player(PlayerFaction::NoirEclipse)) => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::PipeSeam(PipeSeamType::Vertical))).following(3),
                Property::Airport(Faction::Player(PlayerFaction::PurpleLightning)) => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Pipe(PipeType::WN))).following(3),
                Property::Airport(Faction::Player(PlayerFaction::SilverClaw)) => match weather {
                    Weather::Clear => SpritesheetIndex::new(567, 3),
                    Weather::Rain => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Road(RoadType::WN))).following(3),
                    Weather::Snow => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Road(RoadType::WN))).following(3),
                },
                Property::Base(Faction::Player(faction @ PlayerFaction::PinkCosmos)) => spritesheet_index(weather, faction.owns(property.kind().prev())).following(4),
                Property::HQ(faction @ PlayerFaction::CobaltIce) => spritesheet_index(weather, faction.owns(property.kind().prev())).following(22),
                Property::HQ(faction @ PlayerFaction::RedFire) => spritesheet_index(weather, faction.owns(property.kind().prev())).following(12),
                Property::HQ(faction @ PlayerFaction::SilverClaw) => spritesheet_index(weather, faction.owns(property.kind().prev())).following(5),
                
                Property::Airport(Faction::Player(faction @ PlayerFaction::WhiteNova | faction @ PlayerFaction::YellowComet)) => spritesheet_index(weather, faction.prev().owns(property.kind().prev())).following(2),
                Property::Base(Faction::Player(faction @ PlayerFaction::WhiteNova | faction @ PlayerFaction::YellowComet | faction @ PlayerFaction::BrownDesert)) => spritesheet_index(weather, faction.owns(property.kind().prev())).following(4),
                Property::City(Faction::Player(faction @ PlayerFaction::WhiteNova | faction @ PlayerFaction::YellowComet)) => spritesheet_index(weather, faction.owns(property.kind().prev())).following(2),
                Property::ComTower(Faction::Player(faction @ PlayerFaction::WhiteNova | faction @ PlayerFaction::YellowComet)) => spritesheet_index(weather, faction.owns(property.kind().prev())).following(2),
                Property::HQ(faction @ PlayerFaction::WhiteNova | faction @ PlayerFaction::YellowComet) => spritesheet_index(weather, faction.owns(property.kind().prev())).following(2),
                Property::Lab(Faction::Player(faction @ PlayerFaction::WhiteNova | faction @ PlayerFaction::YellowComet)) => spritesheet_index(weather, faction.owns(property.kind().prev())).following(2),
                Property::Port(Faction::Player(faction @ PlayerFaction::WhiteNova | faction @ PlayerFaction::YellowComet)) => spritesheet_index(weather, faction.owns(property.kind().prev())).following(2),

                Property::Airport(Faction::Player(faction)) => spritesheet_index(weather, faction.prev().owns(property.kind().prev())).following(3),
                Property::Base(Faction::Player(faction)) => spritesheet_index(weather, faction.owns(property.kind().prev())).following(6),
                Property::City(Faction::Player(faction)) => spritesheet_index(weather, faction.owns(property.kind().prev())).following(3),
                Property::ComTower(Faction::Player(faction)) => spritesheet_index(weather, faction.owns(property.kind().prev())).following(3),
                Property::HQ(faction) => spritesheet_index(weather, faction.owns(property.kind().prev())).following(3),
                Property::Lab(Faction::Player(faction)) => spritesheet_index(weather, faction.owns(property.kind().prev())).following(3),
                Property::Port(Faction::Player(faction)) => spritesheet_index(weather, faction.owns(property.kind().prev())).following(3),
            },
            
            Terrain::Pipe(pipe_type) => match pipe_type {
                PipeType::EastEnd => spritesheet_index(weather, PlayerFaction::PinkCosmos.owns(PropertyKind::Port)).following(1),
                PipeType::ES => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Pipe(PipeType::EastEnd))).following(1),
                PipeType::Horizontal => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Pipe(PipeType::ES))).following(1),
                PipeType::NE => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Pipe(PipeType::NorthEnd))).following(1),
                PipeType::NorthEnd => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Pipe(PipeType::Horizontal))).following(1),
                PipeType::SouthEnd => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Pipe(PipeType::NE))).following(1),
                PipeType::SW => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Pipe(PipeType::SouthEnd))).following(1),
                PipeType::Vertical => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Pipe(PipeType::SW))).following(1),
                PipeType::WestEnd => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Pipe(PipeType::Vertical))).following(1),
                PipeType::WN => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Pipe(PipeType::WestEnd))).following(1),
            },
            Terrain::MissileSilo(missile_silo_status) => match weather {
                Weather::Clear =>  match missile_silo_status {
                    MissileSiloStatus::Loaded => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Property(Property::Port(Faction::Neutral)))).following(1),
                    MissileSiloStatus::Unloaded => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::MissileSilo(MissileSiloStatus::Loaded))).following(1),
                },
                Weather::Rain | Weather::Snow =>  match missile_silo_status {
                    MissileSiloStatus::Loaded => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::MissileSilo(MissileSiloStatus::Unloaded))).following(1),
                    MissileSiloStatus::Unloaded => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Property(Property::Port(Faction::Neutral)))).following(1),
                },
            },
            Terrain::PipeSeam(pipe_seam_type) => match pipe_seam_type {
                PipeSeamType::Horizontal => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::PipeRubble(PipeRubbleType::Horizontal))).following(1),
                PipeSeamType::Vertical => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::PipeRubble(PipeRubbleType::Vertical))).following(1),
            },
            Terrain::PipeRubble(pipe_rubble_type) => match weather {
                Weather::Clear => match pipe_rubble_type {
                    PipeRubbleType::Horizontal => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Property(Property::ComTower(Faction::Neutral)))).following(1),
                    PipeRubbleType::Vertical => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::MissileSilo(MissileSiloStatus::Unloaded))).following(1),
                },
                Weather::Rain | Weather::Snow => match pipe_rubble_type {
                    PipeRubbleType::Horizontal => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::Property(Property::ComTower(Faction::Neutral)))).following(1),
                    PipeRubbleType::Vertical => spritesheet_index(weather, GraphicalTerrain::Terrain(Terrain::MissileSilo(MissileSiloStatus::Loaded))).following(1),
                },
            } ,
            Terrain::Teleporter => {
                SpritesheetIndex::single_frame(4)
            }
        },
    }
}

impl PlayerFaction {
    #[inline]
    const fn owns(&self, property: PropertyKind) -> GraphicalTerrain {
        let prop = match property {
            PropertyKind::Airport => Property::Airport(Faction::Player(*self)),
            PropertyKind::Base => Property::Base(Faction::Player(*self)),
            PropertyKind::City => Property::City(Faction::Player(*self)),
            PropertyKind::ComTower => Property::ComTower(Faction::Player(*self)),
            PropertyKind::HQ => Property::HQ(*self),
            PropertyKind::Lab => Property::Lab(Faction::Player(*self)),
            PropertyKind::Port => Property::Port(Faction::Player(*self)),
        };
        GraphicalTerrain::Terrain(Terrain::Property(prop))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(
        GraphicalTerrain::Terrain(Terrain::Reef),
        SpritesheetIndex::single_frame(3)
    )]
    #[case(
        GraphicalTerrain::Terrain(Terrain::Teleporter),
        SpritesheetIndex::single_frame(4)
    )]
    fn weather_independent_terrain(
        #[case] terrain: GraphicalTerrain,
        #[case] expected: SpritesheetIndex,
    ) {
        assert_eq!(spritesheet_index(Weather::Clear, terrain), expected);
        assert_eq!(spritesheet_index(Weather::Snow, terrain), expected);
        assert_eq!(spritesheet_index(Weather::Rain, terrain), expected);
    }
}
