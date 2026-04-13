use serde::{Deserialize, Serialize};

use crate::AwbwCoId;

/// Typed CO identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Co {
    Andy,
    Nell,
    Hachi,
    Jake,
    Rachel,
    Colin,
    Sasha,
    Grimm,
    Grit,
    Olaf,
    Eagle,
    Drake,
    Jess,
    Javier,
    Max,
    Adder,
    Flak,
    Lash,
    Hawke,
    Jugger,
    Kindle,
    Koal,
    Sami,
    Sonja,
    Kanbei,
    Sensei,
    Sturm,
    VonBolt,
    NoCo,
}

impl Co {
    /// Converts an AWBW numeric CO ID to the typed enum.
    pub fn from_awbw_id(id: AwbwCoId) -> Option<Self> {
        match id.as_u32() {
            1 => Some(Co::Andy),
            2 => Some(Co::Grit),
            3 => Some(Co::Kanbei),
            5 => Some(Co::Drake),
            7 => Some(Co::Max),
            8 => Some(Co::Sami),
            9 => Some(Co::Olaf),
            10 => Some(Co::Eagle),
            11 => Some(Co::Adder),
            12 => Some(Co::Hawke),
            13 => Some(Co::Sensei),
            14 => Some(Co::Jess),
            15 => Some(Co::Colin),
            16 => Some(Co::Lash),
            17 => Some(Co::Hachi),
            18 => Some(Co::Sonja),
            19 => Some(Co::Sasha),
            20 => Some(Co::Grimm),
            21 => Some(Co::Koal),
            22 => Some(Co::Jake),
            23 => Some(Co::Kindle),
            24 => Some(Co::Nell),
            25 => Some(Co::Flak),
            26 => Some(Co::Jugger),
            27 => Some(Co::Javier),
            28 => Some(Co::Rachel),
            29 => Some(Co::Sturm),
            30 => Some(Co::VonBolt),
            31 => Some(Co::NoCo),
            _ => None,
        }
    }

    /// Returns the D2D stats for this CO.
    ///
    /// COs with special-case mechanics return only their flat bonuses here.
    /// Special branches such as Nell luck and Grit indirects belong in the
    /// combat wiring that can inspect the full engagement.
    pub fn stats(self) -> CoStats {
        match self {
            Co::Colin => CoStats {
                attack_bonus: -10,
                ..Default::default()
            },
            Co::Grimm => CoStats {
                attack_bonus: 30,
                defense_bonus: -20,
                ..Default::default()
            },
            Co::Hawke => CoStats {
                attack_bonus: 10,
                ..Default::default()
            },
            Co::Kanbei => CoStats {
                attack_bonus: 30,
                defense_bonus: 30,
                ..Default::default()
            },
            Co::Nell => CoStats {
                max_good_luck: 19,
                ..Default::default()
            },
            Co::Flak => CoStats {
                max_good_luck: 24,
                max_bad_luck: 9,
                ..Default::default()
            },
            Co::Jugger => CoStats {
                max_good_luck: 29,
                max_bad_luck: 14,
                ..Default::default()
            },
            Co::VonBolt => CoStats {
                attack_bonus: 10,
                defense_bonus: 10,
                ..Default::default()
            },
            _ => CoStats::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownAwbwCoId(pub AwbwCoId);

impl std::fmt::Display for UnknownAwbwCoId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown AWBW CO ID {}", self.0.as_u32())
    }
}

impl std::error::Error for UnknownAwbwCoId {}

impl TryFrom<AwbwCoId> for Co {
    type Error = UnknownAwbwCoId;

    fn try_from(value: AwbwCoId) -> Result<Self, Self::Error> {
        Co::from_awbw_id(value).ok_or(UnknownAwbwCoId(value))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CoStats {
    pub attack_bonus: i32,
    pub defense_bonus: i32,
    pub max_good_luck: u8,
    pub max_bad_luck: u8,
    pub vision_bonus: u8,
}

impl Default for CoStats {
    fn default() -> Self {
        Self {
            attack_bonus: 0,
            defense_bonus: 0,
            max_good_luck: 9,
            max_bad_luck: 0,
            vision_bonus: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_existing_awbw_co_ids() {
        assert_eq!(Co::from_awbw_id(AwbwCoId::new(1)), Some(Co::Andy));
        assert_eq!(Co::from_awbw_id(AwbwCoId::new(11)), Some(Co::Adder));
        assert_eq!(Co::from_awbw_id(AwbwCoId::new(30)), Some(Co::VonBolt));
        assert_eq!(Co::from_awbw_id(AwbwCoId::new(31)), Some(Co::NoCo));
        assert_eq!(Co::from_awbw_id(AwbwCoId::new(4)), None);
    }

    #[test]
    fn luck_cos_have_explicit_luck_bounds() {
        let nell = Co::Nell.stats();
        assert_eq!(nell.max_good_luck, 19);
        assert_eq!(nell.max_bad_luck, 0);

        let flak = Co::Flak.stats();
        assert_eq!(flak.max_good_luck, 24);
        assert_eq!(flak.max_bad_luck, 9);

        let jugger = Co::Jugger.stats();
        assert_eq!(jugger.max_good_luck, 29);
        assert_eq!(jugger.max_bad_luck, 14);
    }
}
