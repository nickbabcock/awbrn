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
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum GraphicalMovement {
    Idle,
    Up,
    Down,
    Lateral,
}
