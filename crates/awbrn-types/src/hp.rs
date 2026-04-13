use serde::{Deserialize, Serialize};

/// Exact unit HP on the 0-100 combat scale.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
#[serde(transparent)]
pub struct ExactHp(u8);

impl ExactHp {
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u8 {
        self.0
    }

    pub fn visual(self) -> VisualHp {
        VisualHp::new(self.0.div_ceil(10))
    }

    pub fn saturating_sub(self, damage: DamagePts) -> Self {
        Self(self.0.saturating_sub(damage.get()))
    }

    pub fn clamp_damage(self, damage: DamagePts) -> DamagePts {
        DamagePts::new(damage.get().min(self.0))
    }
}

/// Display HP on the 0-10 graphical HP scale.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
#[serde(transparent)]
pub struct VisualHp(u8);

impl VisualHp {
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Exact HP-point damage on the 0-100 combat scale.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
#[serde(transparent)]
pub struct DamagePts(u8);

impl DamagePts {
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_hp_converts_to_visual_hp() {
        assert_eq!(ExactHp::new(0).visual(), VisualHp::new(0));
        assert_eq!(ExactHp::new(1).visual(), VisualHp::new(1));
        assert_eq!(ExactHp::new(91).visual(), VisualHp::new(10));
    }

    #[test]
    fn exact_hp_clamps_damage() {
        assert_eq!(
            ExactHp::new(12).clamp_damage(DamagePts::new(80)),
            DamagePts::new(12)
        );
    }
}
