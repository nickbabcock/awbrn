use crate::UnitMovement;

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, strum::EnumCount, strum::VariantArray,
)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
pub enum Unit {
    AntiAir,
    APC,
    Artillery,
    BCopter,
    Battleship,
    BlackBoat,
    BlackBomb,
    Bomber,
    Carrier,
    Cruiser,
    Fighter,
    Infantry,
    Lander,
    MdTank,
    Mech,
    MegaTank,
    Missile,
    NeoTank,
    PipeRunner,
    Recon,
    Rocket,
    Stealth,
    Sub,
    TCopter,
    Tank,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
pub enum UnitDomain {
    Ground,
    Air,
    Sea,
}

impl Unit {
    pub const COUNT: usize = <Self as strum::EnumCount>::COUNT;

    /// Stable dense index for tables keyed by [`Unit`].
    pub const fn table_index(self) -> usize {
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
            Unit::NeoTank => 17,
            Unit::PipeRunner => 18,
            Unit::Recon => 19,
            Unit::Rocket => 20,
            Unit::Stealth => 21,
            Unit::Sub => 22,
            Unit::TCopter => 23,
            Unit::Tank => 24,
        }
    }

    /// Get the display name of this unit
    pub const fn name(&self) -> &'static str {
        match self {
            Unit::AntiAir => "Anti-Air",
            Unit::APC => "APC",
            Unit::Artillery => "Artillery",
            Unit::BCopter => "B-Copter",
            Unit::Battleship => "Battleship",
            Unit::BlackBoat => "Black Boat",
            Unit::BlackBomb => "Black Bomb",
            Unit::Bomber => "Bomber",
            Unit::Carrier => "Carrier",
            Unit::Cruiser => "Cruiser",
            Unit::Fighter => "Fighter",
            Unit::Infantry => "Infantry",
            Unit::Lander => "Lander",
            Unit::MdTank => "MD Tank",
            Unit::Mech => "Mech",
            Unit::MegaTank => "Mega Tank",
            Unit::Missile => "Missile",
            Unit::NeoTank => "Neo Tank",
            Unit::PipeRunner => "Piperunner",
            Unit::Recon => "Recon",
            Unit::Rocket => "Rocket",
            Unit::Stealth => "Stealth",
            Unit::Sub => "Submarine",
            Unit::TCopter => "T-Copter",
            Unit::Tank => "Tank",
        }
    }

    /// Convert a display name to a unit, inverting the `name` method
    pub fn from_awbw_name(name: &str) -> Option<Self> {
        match name {
            "Anti-Air" => Some(Unit::AntiAir),
            "APC" => Some(Unit::APC),
            "Artillery" => Some(Unit::Artillery),
            "B-Copter" => Some(Unit::BCopter),
            "Battleship" => Some(Unit::Battleship),
            "Black Boat" => Some(Unit::BlackBoat),
            "Black Bomb" => Some(Unit::BlackBomb),
            "Bomber" => Some(Unit::Bomber),
            "Carrier" => Some(Unit::Carrier),
            "Cruiser" => Some(Unit::Cruiser),
            "Fighter" => Some(Unit::Fighter),
            "Infantry" => Some(Unit::Infantry),
            "Lander" => Some(Unit::Lander),
            "Md.Tank" => Some(Unit::MdTank),
            "Mech" => Some(Unit::Mech),
            "Mega Tank" => Some(Unit::MegaTank),
            "Missile" => Some(Unit::Missile),
            "Neotank" => Some(Unit::NeoTank),
            "Piperunner" => Some(Unit::PipeRunner),
            "Recon" => Some(Unit::Recon),
            "Rocket" => Some(Unit::Rocket),
            "Stealth" => Some(Unit::Stealth),
            "Sub" => Some(Unit::Sub),
            "T-Copter" => Some(Unit::TCopter),
            "Tank" => Some(Unit::Tank),
            _ => None,
        }
    }

    pub const fn domain(self) -> UnitDomain {
        match self {
            Unit::BCopter
            | Unit::BlackBomb
            | Unit::Bomber
            | Unit::Fighter
            | Unit::Stealth
            | Unit::TCopter => UnitDomain::Air,
            Unit::Battleship
            | Unit::BlackBoat
            | Unit::Carrier
            | Unit::Cruiser
            | Unit::Lander
            | Unit::Sub => UnitDomain::Sea,
            _ => UnitDomain::Ground,
        }
    }

    pub const fn max_fuel(&self) -> u32 {
        match self {
            Unit::AntiAir => 60,
            Unit::APC => 70,
            Unit::Artillery => 50,
            Unit::BCopter => 99,
            Unit::Battleship => 99,
            Unit::BlackBoat => 60,
            Unit::BlackBomb => 45,
            Unit::Bomber => 99,
            Unit::Carrier => 99,
            Unit::Cruiser => 99,
            Unit::Fighter => 99,
            Unit::Infantry => 99,
            Unit::Lander => 99,
            Unit::MdTank => 50,
            Unit::Mech => 70,
            Unit::MegaTank => 50,
            Unit::Missile => 50,
            Unit::NeoTank => 99,
            Unit::PipeRunner => 99,
            Unit::Recon => 80,
            Unit::Rocket => 50,
            Unit::Stealth => 60,
            Unit::Sub => 60,
            Unit::TCopter => 99,
            Unit::Tank => 70,
        }
    }

    pub const fn max_ammo(&self) -> u32 {
        match self {
            Unit::AntiAir => 9,
            Unit::APC => 0,
            Unit::Artillery => 9,
            Unit::BCopter => 6,
            Unit::Battleship => 9,
            Unit::BlackBoat => 0,
            Unit::BlackBomb => 0,
            Unit::Bomber => 9,
            Unit::Carrier => 9,
            Unit::Cruiser => 9,
            Unit::Fighter => 9,
            Unit::Infantry => 0,
            Unit::Lander => 0,
            Unit::MdTank => 8,
            Unit::Mech => 3,
            Unit::MegaTank => 3,
            Unit::Missile => 6,
            Unit::NeoTank => 9,
            Unit::PipeRunner => 9,
            Unit::Recon => 0,
            Unit::Rocket => 6,
            Unit::Stealth => 6,
            Unit::Sub => 6,
            Unit::TCopter => 0,
            Unit::Tank => 9,
        }
    }

    pub const fn movement_range(&self) -> u8 {
        match self {
            Unit::Infantry => 3,
            Unit::Mech => 2,
            Unit::MdTank => 5,
            Unit::Tank => 6,
            Unit::Recon => 8,
            Unit::APC => 6,
            Unit::Artillery => 5,
            Unit::Rocket => 5,
            Unit::AntiAir => 6,
            Unit::Missile => 4,
            Unit::Fighter => 9,
            Unit::Bomber => 7,
            Unit::BCopter => 6,
            Unit::TCopter => 6,
            Unit::Battleship => 5,
            Unit::Cruiser => 6,
            Unit::Lander => 6,
            Unit::Sub => 5,
            Unit::BlackBoat => 7,
            Unit::Carrier => 5,
            Unit::Stealth => 6,
            Unit::NeoTank => 6,
            Unit::PipeRunner => 9,
            Unit::BlackBomb => 9,
            Unit::MegaTank => 4,
        }
    }

    pub const fn movement_type(&self) -> UnitMovement {
        match self {
            Unit::Infantry => UnitMovement::Foot,
            Unit::Mech => UnitMovement::Boot,
            Unit::Recon | Unit::Rocket | Unit::Missile => UnitMovement::Tires,
            Unit::MdTank
            | Unit::Tank
            | Unit::APC
            | Unit::Artillery
            | Unit::AntiAir
            | Unit::NeoTank
            | Unit::MegaTank => UnitMovement::Treads,
            Unit::BCopter
            | Unit::BlackBomb
            | Unit::Bomber
            | Unit::Fighter
            | Unit::Stealth
            | Unit::TCopter => UnitMovement::Air,
            Unit::Battleship | Unit::Carrier | Unit::Cruiser | Unit::Sub => UnitMovement::Sea,
            Unit::BlackBoat | Unit::Lander => UnitMovement::Lander,
            Unit::PipeRunner => UnitMovement::Pipe,
        }
    }

    pub const fn base_cost(&self) -> u32 {
        match self {
            Unit::AntiAir => 8000,
            Unit::APC => 5000,
            Unit::Artillery => 6000,
            Unit::BCopter => 9000,
            Unit::Battleship => 28000,
            Unit::BlackBoat => 7500,
            Unit::BlackBomb => 25000,
            Unit::Bomber => 22000,
            Unit::Carrier => 30000,
            Unit::Cruiser => 18000,
            Unit::Fighter => 20000,
            Unit::Infantry => 1000,
            Unit::Lander => 12000,
            Unit::MdTank => 16000,
            Unit::Mech => 3000,
            Unit::MegaTank => 28000,
            Unit::Missile => 12000,
            Unit::NeoTank => 22000,
            Unit::PipeRunner => 20000,
            Unit::Recon => 4000,
            Unit::Rocket => 15000,
            Unit::Stealth => 24000,
            Unit::Sub => 20000,
            Unit::TCopter => 5000,
            Unit::Tank => 7000,
        }
    }

    pub const fn base_vision(&self) -> u32 {
        match self {
            Unit::Infantry => 2,
            Unit::Mech => 2,
            Unit::MdTank => 1,
            Unit::Tank => 3,
            Unit::Recon => 5,
            Unit::APC => 1,
            Unit::Artillery => 1,
            Unit::Rocket => 1,
            Unit::AntiAir => 2,
            Unit::Missile => 5,
            Unit::Fighter => 2,
            Unit::Bomber => 2,
            Unit::BCopter => 3,
            Unit::TCopter => 2,
            Unit::Battleship => 2,
            Unit::Cruiser => 3,
            Unit::Lander => 1,
            Unit::Sub => 5,
            Unit::BlackBoat => 1,
            Unit::Carrier => 4,
            Unit::Stealth => 4,
            Unit::NeoTank => 1,
            Unit::PipeRunner => 4,
            Unit::BlackBomb => 1,
            Unit::MegaTank => 1,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum GraphicalMovement {
    Idle,
    Up,
    Down,
    Lateral,
}
