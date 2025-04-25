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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum GraphicalMovement {
    None,
    Up,
    Down,
    Lateral,
}
