use crate::{
    AwbwTerrain, BridgeType, Faction, GraphicalMovement, GraphicalTerrain, MissileSiloStatus,
    PipeRubbleType, PipeSeamType, PipeType, PlayerFaction, Property, PropertyKind, RiverType,
    RoadType, ShoalType, Unit, Weather,
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
            Weather::Clear | Weather::Rain => SpritesheetIndex::single_frame(0),
            Weather::Snow => spritesheet_index(Weather::Clear, GraphicalTerrain::Property(Property::Port(Faction::Player(PlayerFaction::YellowComet)))).following(1),
        },
        x => match x.as_terrain() {
            AwbwTerrain::Plain => {
                spritesheet_index(weather, GraphicalTerrain::Mountain).following(1)
            }
            AwbwTerrain::Mountain => match weather {
                Weather::Clear => SpritesheetIndex::single_frame(2),
                Weather::Snow => spritesheet_index(Weather::Snow, GraphicalTerrain::StubbyMoutain).following(1),
                Weather::Rain => spritesheet_index(Weather::Snow, GraphicalTerrain::Property(Property::Port(Faction::Player(PlayerFaction::YellowComet)))).following(1),
            }
            AwbwTerrain::Wood => match weather {
                Weather::Clear => spritesheet_index(weather, GraphicalTerrain::Teleporter).following(1),
                Weather::Snow | Weather::Rain => spritesheet_index(weather, GraphicalTerrain::Plain).following(1),
            },
            AwbwTerrain::River(river_type) => match river_type {
                RiverType::Cross => spritesheet_index(weather, GraphicalTerrain::Property(Property::Port(Faction::Player(PlayerFaction::RedFire)))).following(1),
                RiverType::ES => spritesheet_index(weather, GraphicalTerrain::River(RiverType::Cross)).following(1),
                RiverType::ESW => spritesheet_index(weather, GraphicalTerrain::River(RiverType::ES)).following(1),
                RiverType::Horizontal => spritesheet_index(weather, GraphicalTerrain::River(RiverType::ESW)).following(1),
                RiverType::NE => spritesheet_index(weather, GraphicalTerrain::River(RiverType::Horizontal)).following(1),
                RiverType::NES => spritesheet_index(weather, GraphicalTerrain::River(RiverType::NE)).following(1),
                RiverType::SW => spritesheet_index(weather, GraphicalTerrain::River(RiverType::NES)).following(1),
                RiverType::SWN => spritesheet_index(weather, GraphicalTerrain::River(RiverType::SW)).following(1),
                RiverType::Vertical => spritesheet_index(weather, GraphicalTerrain::River(RiverType::SWN)).following(1),
                RiverType::WN => spritesheet_index(weather, GraphicalTerrain::River(RiverType::Vertical)).following(1),
                RiverType::WNE => spritesheet_index(weather, GraphicalTerrain::River(RiverType::WN)).following(1),
            },
            AwbwTerrain::Road(road_type) => match road_type {
                RoadType::Cross => spritesheet_index(weather, GraphicalTerrain::River(RiverType::WNE)).following(1),
                RoadType::ES => spritesheet_index(weather, GraphicalTerrain::Road(RoadType::Cross)).following(1),
                RoadType::ESW => spritesheet_index(weather, GraphicalTerrain::Road(RoadType::ES)).following(1),
                RoadType::Horizontal => spritesheet_index(weather, GraphicalTerrain::Road(RoadType::ESW)).following(1),
                RoadType::NE => spritesheet_index(weather, GraphicalTerrain::Bridge(BridgeType::Horizontal)).following(1),
                RoadType::NES => spritesheet_index(weather, GraphicalTerrain::Road(RoadType::NE)).following(1),
                RoadType::SW => spritesheet_index(weather, GraphicalTerrain::Road(RoadType::NES)).following(1),
                RoadType::SWN => spritesheet_index(weather, GraphicalTerrain::Road(RoadType::SW)).following(1),
                RoadType::Vertical => spritesheet_index(weather, GraphicalTerrain::Road(RoadType::SWN)).following(1),
                RoadType::WN => spritesheet_index(weather, GraphicalTerrain::Bridge(BridgeType::Vertical)).following(1),
                RoadType::WNE => spritesheet_index(weather, GraphicalTerrain::Road(RoadType::WN)).following(1),
            },
            AwbwTerrain::Bridge(bridge_type) => match bridge_type {
                BridgeType::Horizontal => spritesheet_index(weather, GraphicalTerrain::Road(RoadType::Horizontal)).following(1),
                BridgeType::Vertical => spritesheet_index(weather, GraphicalTerrain::Road(RoadType::Vertical)).following(1),
            },
            AwbwTerrain::Sea => SpritesheetIndex::single_frame(450),
            AwbwTerrain::Shoal(shoal_type) => match shoal_type {
                ShoalType::Horizontal => SpritesheetIndex::new(558, 1),
                ShoalType::HorizontalNorth => SpritesheetIndex::new(563, 1),
                ShoalType::Vertical => SpritesheetIndex::new(511, 1),
                ShoalType::VerticalEast => SpritesheetIndex::new(505, 1),
            },
            AwbwTerrain::Reef => {
                SpritesheetIndex::single_frame(4)
            }
            AwbwTerrain::Property(property) => match property {
                Property::Airport(Faction::Neutral) => spritesheet_index(weather, GraphicalTerrain::Property(Property::Port(Faction::Player(PlayerFaction::JadeSun)))).following(1),
                Property::Base(Faction::Neutral) =>  spritesheet_index(weather, GraphicalTerrain::Property(Property::Airport(Faction::Neutral))).following(1),
                Property::City(Faction::Neutral) => spritesheet_index(weather, GraphicalTerrain::Property(Property::Base(Faction::Neutral))).following(1),
                Property::ComTower(Faction::Neutral) => spritesheet_index(weather, GraphicalTerrain::Property(Property::City(Faction::Neutral))).following(1),
                Property::Lab(Faction::Neutral) => spritesheet_index(weather, GraphicalTerrain::PipeSeam(PipeSeamType::Horizontal)).following(1),
                Property::Port(Faction::Neutral) => spritesheet_index(weather, GraphicalTerrain::Property(Property::Lab(Faction::Neutral))).following(1),

                Property::Airport(Faction::Player(PlayerFaction::AcidRain)) => spritesheet_index(weather, GraphicalTerrain::Wood).following(3),
                Property::Airport(Faction::Player(PlayerFaction::NoirEclipse)) => spritesheet_index(weather, GraphicalTerrain::PipeSeam(PipeSeamType::Vertical)).following(3),
                Property::Airport(Faction::Player(PlayerFaction::PurpleLightning)) => spritesheet_index(weather, GraphicalTerrain::Pipe(PipeType::WN)).following(3),
                Property::Airport(Faction::Player(PlayerFaction::SilverClaw)) => match weather {
                    Weather::Clear => SpritesheetIndex::new(567, 3),
                    Weather::Rain | Weather::Snow => spritesheet_index(weather, GraphicalTerrain::Road(RoadType::WNE)).following(3),
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
            
            AwbwTerrain::Pipe(pipe_type) => match pipe_type {
                PipeType::EastEnd => spritesheet_index(weather, PlayerFaction::PinkCosmos.owns(PropertyKind::Port)).following(1),
                PipeType::ES => spritesheet_index(weather, GraphicalTerrain::Pipe(PipeType::EastEnd)).following(1),
                PipeType::Horizontal => spritesheet_index(weather, GraphicalTerrain::Pipe(PipeType::ES)).following(1),
                PipeType::NE => spritesheet_index(weather, GraphicalTerrain::Pipe(PipeType::Horizontal)).following(1),
                PipeType::NorthEnd => spritesheet_index(weather, GraphicalTerrain::Pipe(PipeType::NE)).following(1),
                PipeType::SouthEnd => spritesheet_index(weather, GraphicalTerrain::Pipe(PipeType::NorthEnd)).following(1),
                PipeType::SW => spritesheet_index(weather, GraphicalTerrain::Pipe(PipeType::SouthEnd)).following(1),
                PipeType::Vertical => spritesheet_index(weather, GraphicalTerrain::Pipe(PipeType::SW)).following(1),
                PipeType::WestEnd => spritesheet_index(weather, GraphicalTerrain::Pipe(PipeType::Vertical)).following(1),
                PipeType::WN => spritesheet_index(weather, GraphicalTerrain::Pipe(PipeType::WestEnd)).following(1),
            },
            AwbwTerrain::MissileSilo(missile_silo_status) => match missile_silo_status {
                MissileSiloStatus::Loaded => spritesheet_index(weather, GraphicalTerrain::Property(Property::Port(Faction::Neutral))).following(1),
                MissileSiloStatus::Unloaded => spritesheet_index(weather, GraphicalTerrain::MissileSilo(MissileSiloStatus::Loaded)).following(1),
           },
            AwbwTerrain::PipeSeam(pipe_seam_type) => match pipe_seam_type {
                PipeSeamType::Horizontal => spritesheet_index(weather, GraphicalTerrain::PipeRubble(PipeRubbleType::Horizontal)).following(1),
                PipeSeamType::Vertical => spritesheet_index(weather, GraphicalTerrain::PipeRubble(PipeRubbleType::Vertical)).following(1),
            },
            AwbwTerrain::PipeRubble(pipe_rubble_type) => match pipe_rubble_type {
                PipeRubbleType::Horizontal => spritesheet_index(weather, GraphicalTerrain::Property(Property::ComTower(Faction::Neutral))).following(1),
                PipeRubbleType::Vertical => spritesheet_index(weather, GraphicalTerrain::MissileSilo(MissileSiloStatus::Unloaded)).following(1),
            },
            AwbwTerrain::Teleporter => {
                SpritesheetIndex::single_frame(5)
            }
        },
    }
}

pub const fn unit_spritesheet_index(
    movement: GraphicalMovement,
    unit: Unit,
    faction: PlayerFaction,
) -> SpritesheetIndex {
    const IND: [[u16; 4]; 25] = [
        [4, 3, 3, 3], // Anti-air
        [4, 3, 3, 3], // APC
        [4, 3, 3, 3], // Artillery
        [2, 2, 2, 2], // Battleship
        [4, 2, 2, 2], // BlackBoat
        [4, 3, 3, 3], // Bomb
        [2, 3, 3, 3], // Bomber
        [4, 2, 2, 2], // B-Copter
        [2, 2, 2, 2], // Carrier
        [2, 2, 2, 2], // Cruiser
        [2, 3, 3, 3], // Fighter
        [4, 4, 4, 4], // Infantry
        [2, 2, 2, 2], // Lander
        [4, 3, 3, 3], // MdTank
        [2, 4, 4, 4], // Mech
        [4, 3, 3, 3], // MegaTank
        [2, 3, 3, 3], // Missle
        [4, 3, 3, 3], // NeoTank
        [2, 3, 3, 3], // Piperunner
        [4, 3, 3, 3], // Recon
        [2, 3, 3, 3], // Rocket
        [2, 3, 3, 3], // Stealth
        [4, 2, 2, 2], // Sub
        [4, 3, 3, 3], // Tank
        [4, 2, 2, 2], // T-Copter
    ];

    let mut total: u16 = 0;
    let mut i = 0;
    while i < IND.len() {
        total += IND[i][0];
        total += IND[i][1];
        total += IND[i][2];
        total += IND[i][3];
        i += 1;
    }

    let faction_index = total * faction.index() as u16;

    i = 0;
    let mut unit_offset = 0;
    while i < unit.index() {
        unit_offset += IND[i][0];
        unit_offset += IND[i][1];
        unit_offset += IND[i][2];
        unit_offset += IND[i][3];
        i += 1;
    }

    let (offset, animation_frames) = match movement {
        GraphicalMovement::None => (0, IND[unit.index()][0]),
        GraphicalMovement::Up => (IND[unit.index()][0], IND[unit.index()][1]),
        GraphicalMovement::Down => (
            IND[unit.index()][1] + IND[unit.index()][0],
            IND[unit.index()][2],
        ),
        GraphicalMovement::Lateral => (
            IND[unit.index()][2] + IND[unit.index()][1] + IND[unit.index()][0],
            IND[unit.index()][3],
        ),
    };

    SpritesheetIndex::new(faction_index + unit_offset + offset, animation_frames as u8)
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
        GraphicalTerrain::Property(prop)
    }

    const fn index(&self) -> usize {
        match self {
            PlayerFaction::AcidRain => 0,
            PlayerFaction::AmberBlaze => 1,
            PlayerFaction::AzureAsteroid => 2,
            PlayerFaction::BlackHole => 3,
            PlayerFaction::BlueMoon => 4,
            PlayerFaction::BrownDesert => 5,
            PlayerFaction::CobaltIce => 6,
            PlayerFaction::GreenEarth => 7,
            PlayerFaction::GreySky => 8,
            PlayerFaction::JadeSun => 9,
            PlayerFaction::NoirEclipse => 10,
            PlayerFaction::OrangeStar => 11,
            PlayerFaction::PinkCosmos => 12,
            PlayerFaction::PurpleLightning => 13,
            PlayerFaction::RedFire => 14,
            PlayerFaction::SilverClaw => 15,
            PlayerFaction::TealGalaxy => 16,
            PlayerFaction::WhiteNova => 17,
            PlayerFaction::YellowComet => 18,
        }
    }
}

impl Unit {
    const fn index(&self) -> usize {
        match self {
            Unit::AntiAir => 0,
            Unit::APC => 1,
            Unit::Artillery => 2,
            Unit::BCopter => 3,
            Unit::Battleship => 4,
            Unit::BlackBoat => 5,
            Unit::BlackBomb => 6,
            Unit::Bomber => 7,
            Unit::Carrier => 8,
            Unit::Cruiser => 9,
            Unit::Fighter => 10,
            Unit::Infantry => 11,
            Unit::Lander => 12,
            Unit::MdTank => 13,
            Unit::Mech => 14,
            Unit::MegaTank => 15,
            Unit::Missile => 16,
            Unit::Neotank => 17,
            Unit::Piperunner => 18,
            Unit::Recon => 19,
            Unit::Rocket => 20,
            Unit::Stealth => 21,
            Unit::Sub => 22,
            Unit::TCopter => 23,
            Unit::Tank => 24,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(GraphicalTerrain::Reef, SpritesheetIndex::single_frame(4))]
    #[case(GraphicalTerrain::Teleporter, SpritesheetIndex::single_frame(5))]
    fn weather_independent_terrain(
        #[case] terrain: GraphicalTerrain,
        #[case] expected: SpritesheetIndex,
    ) {
        assert_eq!(spritesheet_index(Weather::Clear, terrain), expected);
        assert_eq!(spritesheet_index(Weather::Snow, terrain), expected);
        assert_eq!(spritesheet_index(Weather::Rain, terrain), expected);
    }
}
