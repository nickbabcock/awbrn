/// A unit type as AWBW names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AwbwUnitKind {
    AntiAir,
    Apc,
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
    Piperunner,
    Recon,
    Rocket,
    Stealth,
    Sub,
    TCopter,
    Tank,
}

impl AwbwUnitKind {
    /// Parse the unit name AWBW writes in replays, such as `Md.Tank`.
    pub fn from_awbw_name(name: &str) -> Option<Self> {
        Some(match name {
            "Anti-Air" => Self::AntiAir,
            "APC" => Self::Apc,
            "Artillery" => Self::Artillery,
            "B-Copter" => Self::BCopter,
            "Battleship" => Self::Battleship,
            "Black Boat" => Self::BlackBoat,
            "Black Bomb" => Self::BlackBomb,
            "Bomber" => Self::Bomber,
            "Carrier" => Self::Carrier,
            "Cruiser" => Self::Cruiser,
            "Fighter" => Self::Fighter,
            "Infantry" => Self::Infantry,
            "Lander" => Self::Lander,
            "Md.Tank" => Self::MdTank,
            "Mech" => Self::Mech,
            "Mega Tank" => Self::MegaTank,
            "Missile" => Self::Missile,
            "Neotank" => Self::NeoTank,
            "Piperunner" => Self::Piperunner,
            "Recon" => Self::Recon,
            "Rocket" => Self::Rocket,
            "Stealth" => Self::Stealth,
            "Sub" => Self::Sub,
            "T-Copter" => Self::TCopter,
            "Tank" => Self::Tank,
            _ => return None,
        })
    }

    /// The display name, such as `MD Tank`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::AntiAir => "Anti-Air",
            Self::Apc => "APC",
            Self::Artillery => "Artillery",
            Self::BCopter => "B-Copter",
            Self::Battleship => "Battleship",
            Self::BlackBoat => "Black Boat",
            Self::BlackBomb => "Black Bomb",
            Self::Bomber => "Bomber",
            Self::Carrier => "Carrier",
            Self::Cruiser => "Cruiser",
            Self::Fighter => "Fighter",
            Self::Infantry => "Infantry",
            Self::Lander => "Lander",
            Self::MdTank => "MD Tank",
            Self::Mech => "Mech",
            Self::MegaTank => "Mega Tank",
            Self::Missile => "Missile",
            Self::NeoTank => "Neo Tank",
            Self::Piperunner => "Piperunner",
            Self::Recon => "Recon",
            Self::Rocket => "Rocket",
            Self::Stealth => "Stealth",
            Self::Sub => "Submarine",
            Self::TCopter => "T-Copter",
            Self::Tank => "Tank",
        }
    }
}
