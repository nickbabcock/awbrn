use crate::{
    BridgeType, Faction, GraphicalMovement, GraphicalTerrain, MissileSiloStatus, PipeRubbleType,
    PipeSeamType, PipeType, PlayerFaction, Property, PropertyKind, RiverType, RoadType,
    SeaDirection, ShoalDirection, Unit, Weather,
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
        GraphicalTerrain::Plain => {
            spritesheet_index(weather, GraphicalTerrain::Mountain).following(1)
        }
        GraphicalTerrain::Mountain => match weather {
            Weather::Clear => SpritesheetIndex::single_frame(2),
            Weather::Snow => spritesheet_index(Weather::Snow, GraphicalTerrain::StubbyMoutain).following(1),
            Weather::Rain => spritesheet_index(Weather::Snow, GraphicalTerrain::Property(Property::Port(Faction::Player(PlayerFaction::YellowComet)))).following(1),
        }
        GraphicalTerrain::Wood => match weather {
            Weather::Clear => spritesheet_index(weather, GraphicalTerrain::Teleporter).following(1),
            Weather::Snow | Weather::Rain => spritesheet_index(weather, GraphicalTerrain::Plain).following(1),
        },
        GraphicalTerrain::River(river_type) => match river_type {
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
        GraphicalTerrain::Road(road_type) => match road_type {
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
        GraphicalTerrain::Bridge(bridge_type) => match bridge_type {
            BridgeType::Horizontal => spritesheet_index(weather, GraphicalTerrain::Road(RoadType::Horizontal)).following(1),
            BridgeType::Vertical => spritesheet_index(weather, GraphicalTerrain::Road(RoadType::Vertical)).following(1),
        },
        GraphicalTerrain::Sea(sea_type) => match sea_type {
            SeaDirection::E => spritesheet_index(Weather::Clear, GraphicalTerrain::Road(RoadType::WNE)).following(1),
            SeaDirection::E_NW => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::E)).following(1),
            SeaDirection::E_NW_SW => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::E_NW)).following(1),
            SeaDirection::E_S => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::E_NW_SW)).following(1),
            SeaDirection::E_S_NW => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::E_S)).following(1),
            SeaDirection::E_S_W => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::E_S_NW)).following(1),
            SeaDirection::E_SW => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::E_S_W)).following(1),
            SeaDirection::E_W => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::E_SW)).following(1),
            SeaDirection::N => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::E_W)).following(1),
            SeaDirection::N_E => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::N)).following(1),
            SeaDirection::N_E_S => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::N_E)).following(1),
            SeaDirection::N_E_S_W => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::N_E_S)).following(1),
            SeaDirection::N_E_SW => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::N_E_S_W)).following(1),
            SeaDirection::N_E_W => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::N_E_SW)).following(1),
            SeaDirection::N_S => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::N_E_W)).following(1),
            SeaDirection::N_S_W => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::N_S)).following(1),
            SeaDirection::N_SE => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::N_S_W)).following(1),
            SeaDirection::N_SE_SW => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::N_SE)).following(1),
            SeaDirection::N_SW => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::N_SE_SW)).following(1),
            SeaDirection::N_W => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::N_SW)).following(1),
            SeaDirection::N_W_SE => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::N_W)).following(1),
            SeaDirection::NE => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::N_W_SE)).following(1),
            SeaDirection::NE_SE => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::NE)).following(1),
            SeaDirection::NE_SE_SW => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::NE_SE)).following(1),
            SeaDirection::NE_SW => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::NE_SE_SW)).following(1),
            SeaDirection::NW => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::NE_SW)).following(1),
            SeaDirection::NW_NE => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::NW)).following(1),
            SeaDirection::NW_NE_SE => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::NW_NE)).following(1),
            SeaDirection::NW_NE_SE_SW => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::NW_NE_SE)).following(1),
            SeaDirection::NW_NE_SW => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::NW_NE_SE_SW)).following(1),
            SeaDirection::NW_SE => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::NW_NE_SW)).following(1),
            SeaDirection::NW_SE_SW => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::NW_SE)).following(1),
            SeaDirection::NW_SW => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::NW_SE_SW)).following(1),
            SeaDirection::S => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::NW_SW)).following(1),
            SeaDirection::S_E => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::S)).following(1),
            SeaDirection::S_NE => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::S_E)).following(1),
            SeaDirection::S_NW => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::S_NE)).following(1),
            SeaDirection::S_NW_NE => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::S_NW)).following(1),
            SeaDirection::S_W => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::S_NW_NE)).following(1),
            SeaDirection::S_W_NE => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::S_W)).following(1),
            SeaDirection::SE => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::S_W_NE)).following(1),
            SeaDirection::SE_SW => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::SE)).following(1),
            SeaDirection::SW => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::SE_SW)).following(1),
            SeaDirection::Sea => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::SW)).following(1),
            SeaDirection::W => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::Sea)).following(1),
            SeaDirection::W_E => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::W)).following(1),
            SeaDirection::W_NE => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::W_E)).following(1),
            SeaDirection::W_NE_SE => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::W_NE)).following(1),
            SeaDirection::W_SE => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::W_NE_SE)).following(1),
        }
        GraphicalTerrain::Shoal(shoal_type) => match shoal_type {
            ShoalDirection::AE => spritesheet_index(Weather::Clear, GraphicalTerrain::Sea(SeaDirection::W_SE)).following(1),
            ShoalDirection::AEAS => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::AE)).following(1),
            ShoalDirection::AEASAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::AEAS)).following(1),
            ShoalDirection::AEASW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::AEASAW)).following(1),
            ShoalDirection::AEAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::AEASW)).following(1),
            ShoalDirection::AES => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::AEAW)).following(1),
            ShoalDirection::AESAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::AES)).following(1),
            ShoalDirection::AESW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::AESAW)).following(1),
            ShoalDirection::AEW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::AESW)).following(1),
            ShoalDirection::AN => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::AEW)).following(1),
            ShoalDirection::ANAE => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::AN)).following(1),
            ShoalDirection::ANAEAS => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANAE)).following(1),
            ShoalDirection::ANAEASAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANAEAS)).following(1),
            ShoalDirection::ANAEASW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANAEASAW)).following(1),
            ShoalDirection::ANAEAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANAEASW)).following(1),
            ShoalDirection::ANAES => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANAEAW)).following(1),
            ShoalDirection::ANAESAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANAES)).following(1),
            ShoalDirection::ANAESW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANAESAW)).following(1),
            ShoalDirection::ANAEW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANAESW)).following(1),
            ShoalDirection::ANAS => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANAEW)).following(1),
            ShoalDirection::ANASAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANAS)).following(1),
            ShoalDirection::ANASW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANASAW)).following(1),
            ShoalDirection::ANAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANASW)).following(1),
            ShoalDirection::ANE => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANAW)).following(1),
            ShoalDirection::ANEAS => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANE)).following(1),
            ShoalDirection::ANEASAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANEAS)).following(1),
            ShoalDirection::ANEASW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANEASAW)).following(1),
            ShoalDirection::ANEAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANEASW)).following(1),
            ShoalDirection::ANES => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANEAW)).following(1),
            ShoalDirection::ANESAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANES)).following(1),
            ShoalDirection::ANESW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANESAW)).following(1),
            ShoalDirection::ANEW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANESW)).following(1),
            ShoalDirection::ANS => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANEW)).following(1),
            ShoalDirection::ANSAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANS)).following(1),
            ShoalDirection::ANSW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANSAW)).following(1),
            ShoalDirection::ANW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANSW)).following(1),
            ShoalDirection::AS => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ANW)).following(1),
            ShoalDirection::ASAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::AS)).following(1),
            ShoalDirection::ASW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ASAW)).following(1),
            ShoalDirection::AW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ASW)).following(1),
            ShoalDirection::C => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::AW)).following(1),
            ShoalDirection::E => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::C)).following(1),
            ShoalDirection::EAS => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::E)).following(1),
            ShoalDirection::EASAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::EAS)).following(1),
            ShoalDirection::EASW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::EASAW)).following(1),
            ShoalDirection::EAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::EASW)).following(1),
            ShoalDirection::ES => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::EAW)).following(1),
            ShoalDirection::ESAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ES)).following(1),
            ShoalDirection::ESW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ESAW)).following(1),
            ShoalDirection::EW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::ESW)).following(1),
            ShoalDirection::N => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::EW)).following(1),
            ShoalDirection::NAE => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::N)).following(1),
            ShoalDirection::NAEAS => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NAE)).following(1),
            ShoalDirection::NAEASAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NAEAS)).following(1),
            ShoalDirection::NAEASW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NAEASAW)).following(1),
            ShoalDirection::NAEAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NAEASW)).following(1),
            ShoalDirection::NAES => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NAEAW)).following(1),
            ShoalDirection::NAESAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NAES)).following(1),
            ShoalDirection::NAESW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NAESAW)).following(1),
            ShoalDirection::NAEW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NAESW)).following(1),
            ShoalDirection::NAS => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NAEW)).following(1),
            ShoalDirection::NASAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NAS)).following(1),
            ShoalDirection::NASW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NASAW)).following(1),
            ShoalDirection::NAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NASW)).following(1),
            ShoalDirection::NE => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NAW)).following(1),
            ShoalDirection::NEAS => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NE)).following(1),
            ShoalDirection::NEASAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NEAS)).following(1),
            ShoalDirection::NEASW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NEASAW)).following(1),
            ShoalDirection::NEAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NEASW)).following(1),
            ShoalDirection::NES => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NEAW)).following(1),
            ShoalDirection::NESAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NES)).following(1),
            ShoalDirection::NESW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NESAW)).following(1),
            ShoalDirection::NEW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NESW)).following(1),
            ShoalDirection::NS => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NEW)).following(1),
            ShoalDirection::NSAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NS)).following(1),
            ShoalDirection::NSW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NSAW)).following(1),
            ShoalDirection::NW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NSW)).following(1),
            ShoalDirection::S => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::NW)).following(1),
            ShoalDirection::SAW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::S)).following(1),
            ShoalDirection::SW => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::SAW)).following(1),
            ShoalDirection::W => spritesheet_index(Weather::Clear, GraphicalTerrain::Shoal(ShoalDirection::SW)).following(1),
        },
        GraphicalTerrain::Reef => {
            SpritesheetIndex::single_frame(4)
        }
        GraphicalTerrain::Property(property) => match property {
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
        GraphicalTerrain::Pipe(pipe_type) => match pipe_type {
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
        GraphicalTerrain::MissileSilo(missile_silo_status) => match missile_silo_status {
            MissileSiloStatus::Loaded => spritesheet_index(weather, GraphicalTerrain::Property(Property::Port(Faction::Neutral))).following(1),
            MissileSiloStatus::Unloaded => spritesheet_index(weather, GraphicalTerrain::MissileSilo(MissileSiloStatus::Loaded)).following(1),
        },
        GraphicalTerrain::PipeSeam(pipe_seam_type) => match pipe_seam_type {
            PipeSeamType::Horizontal => spritesheet_index(weather, GraphicalTerrain::PipeRubble(PipeRubbleType::Horizontal)).following(1),
            PipeSeamType::Vertical => spritesheet_index(weather, GraphicalTerrain::PipeRubble(PipeRubbleType::Vertical)).following(1),
        },
        GraphicalTerrain::PipeRubble(pipe_rubble_type) => match pipe_rubble_type {
            PipeRubbleType::Horizontal => spritesheet_index(weather, GraphicalTerrain::Property(Property::ComTower(Faction::Neutral))).following(1),
            PipeRubbleType::Vertical => spritesheet_index(weather, GraphicalTerrain::MissileSilo(MissileSiloStatus::Unloaded)).following(1),
        },
        GraphicalTerrain::Teleporter => {
            SpritesheetIndex::single_frame(5)
        }
    }
}

pub fn unit_spritesheet_index(
    movement: GraphicalMovement,
    unit: Unit,
    faction: PlayerFaction,
) -> SpritesheetIndex {
    let animation_frames = crate::get_unit_animation_frames(movement, unit, faction);
    SpritesheetIndex::new(
        animation_frames.start_index(),
        animation_frames.frame_count() as u8,
    )
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
