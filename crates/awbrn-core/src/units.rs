#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
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
    Neotank,
    Piperunner,
    Recon,
    Rocket,
    Stealth,
    Sub,
    TCopter,
    Tank,
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
            Unit::Neotank => "Neo Tank",
            Unit::Piperunner => "Piperunner",
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
            "Neotank" => Some(Unit::Neotank),
            "Piperunner" => Some(Unit::Piperunner),
            "Recon" => Some(Unit::Recon),
            "Rocket" => Some(Unit::Rocket),
            "Stealth" => Some(Unit::Stealth),
            "Sub" => Some(Unit::Sub),
            "T-Copter" => Some(Unit::TCopter),
            "Tank" => Some(Unit::Tank),
            _ => None,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum GraphicalMovement {
    None,
    Up,
    Down,
    Lateral,
}
