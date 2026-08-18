use awvm::ruleset::profile;

/// Canonical unit vocabulary generated from the AWBW ruleset.
///
/// Presentation and replay helpers live in [`UnitExt`]; the identity and wire
/// spelling come directly from AWVM so the workspace has only one unit enum.
pub use awvm::ruleset::UnitKind as Unit;

/// Canonical unit domain vocabulary generated from the AWBW ruleset.
pub use awvm::ruleset::Domain as UnitDomain;

/// AWBW presentation and compatibility operations on the canonical unit kind.
pub trait UnitExt: Sized + Copy {
    /// Get the display name used by AWBW replay payloads and UI.
    fn name(&self) -> &'static str;

    /// Convert an AWBW display name to a unit kind.
    fn from_awbw_name(name: &str) -> Option<Self>;

    /// Convert an AWBW numeric unit-type id to a unit kind.
    fn from_awbw_id(id: u32) -> Option<Self>;

    fn domain(self) -> UnitDomain;
    fn max_fuel(&self) -> u32;
    fn max_ammo(&self) -> u32;
    fn movement_range(&self) -> u8;
    fn base_cost(&self) -> u32;
    fn base_vision(&self) -> u32;
    fn attack_range_min(self) -> u32;
    fn attack_range_max(self) -> u32;

    fn is_indirect(&self) -> bool {
        (*self).attack_range_min() > 1
    }
}

impl UnitExt for Unit {
    fn name(&self) -> &'static str {
        match self {
            Unit::AntiAir => "Anti-Air",
            Unit::Apc => "APC",
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
            Unit::Piperunner => "Piperunner",
            Unit::Recon => "Recon",
            Unit::Rocket => "Rocket",
            Unit::Stealth => "Stealth",
            Unit::Sub => "Submarine",
            Unit::TCopter => "T-Copter",
            Unit::Tank => "Tank",
        }
    }

    fn from_awbw_name(name: &str) -> Option<Self> {
        Some(match name {
            "Anti-Air" => Unit::AntiAir,
            "APC" => Unit::Apc,
            "Artillery" => Unit::Artillery,
            "B-Copter" => Unit::BCopter,
            "Battleship" => Unit::Battleship,
            "Black Boat" => Unit::BlackBoat,
            "Black Bomb" => Unit::BlackBomb,
            "Bomber" => Unit::Bomber,
            "Carrier" => Unit::Carrier,
            "Cruiser" => Unit::Cruiser,
            "Fighter" => Unit::Fighter,
            "Infantry" => Unit::Infantry,
            "Lander" => Unit::Lander,
            "Md.Tank" => Unit::MdTank,
            "Mech" => Unit::Mech,
            "Mega Tank" => Unit::MegaTank,
            "Missile" => Unit::Missile,
            "Neotank" => Unit::NeoTank,
            "Piperunner" => Unit::Piperunner,
            "Recon" => Unit::Recon,
            "Rocket" => Unit::Rocket,
            "Stealth" => Unit::Stealth,
            "Sub" => Unit::Sub,
            "T-Copter" => Unit::TCopter,
            "Tank" => Unit::Tank,
            _ => return None,
        })
    }

    fn from_awbw_id(id: u32) -> Option<Self> {
        Unit::ALL
            .into_iter()
            .find(|kind| profile(*kind).awbw_id == id)
    }

    fn domain(self) -> UnitDomain {
        profile(self).domain
    }

    fn max_fuel(&self) -> u32 {
        narrow_u32(profile(*self).max_fuel)
    }

    fn max_ammo(&self) -> u32 {
        narrow_u32(profile(*self).max_ammo)
    }

    fn movement_range(&self) -> u8 {
        u8::try_from(profile(*self).movement).expect("ruleset movement fits the legacy view")
    }

    fn base_cost(&self) -> u32 {
        narrow_u32(profile(*self).cost)
    }

    fn base_vision(&self) -> u32 {
        u32::try_from(profile(*self).vision).expect("ruleset vision fits the legacy view")
    }

    fn attack_range_min(self) -> u32 {
        profile(self)
            .indirect_range
            .map_or(1, |range| narrow_u32(range.minimum))
    }

    fn attack_range_max(self) -> u32 {
        profile(self)
            .indirect_range
            .map_or(1, |range| narrow_u32(range.maximum))
    }
}

fn narrow_u32(value: u64) -> u32 {
    u32::try_from(value).expect("AWBW ruleset values fit the legacy view")
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum GraphicalMovement {
    Idle,
    Up,
    Down,
    Lateral,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_awbw_id_maps_known_ids() {
        assert_eq!(Unit::from_awbw_id(1), Some(Unit::Infantry));
        assert_eq!(Unit::from_awbw_id(18), Some(Unit::Sub));
        assert_eq!(Unit::from_awbw_id(46), Some(Unit::NeoTank));
        assert_eq!(Unit::from_awbw_id(960900), Some(Unit::Piperunner));
        assert_eq!(Unit::from_awbw_id(1141438), Some(Unit::MegaTank));
        assert_eq!(Unit::from_awbw_id(0), None);
        assert_eq!(Unit::from_awbw_id(19), None);
    }
}
