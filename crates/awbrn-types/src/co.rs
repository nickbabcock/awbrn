use crate::AwbwCoId;

/// Canonical commander vocabulary generated from the AWBW ruleset.
pub use awvm::ruleset::CommanderKind as Co;

/// AWBW compatibility operations on the canonical commander kind.
pub trait CoExt: Sized {
    fn from_awbw_id(id: AwbwCoId) -> Option<Self>;
}

impl CoExt for Co {
    fn from_awbw_id(id: AwbwCoId) -> Option<Self> {
        Some(match id.as_u32() {
            1 => Co::Andy,
            2 => Co::Grit,
            3 => Co::Kanbei,
            5 => Co::Drake,
            7 => Co::Max,
            8 => Co::Sami,
            9 => Co::Olaf,
            10 => Co::Eagle,
            11 => Co::Adder,
            12 => Co::Hawke,
            13 => Co::Sensei,
            14 => Co::Jess,
            15 => Co::Colin,
            16 => Co::Lash,
            17 => Co::Hachi,
            18 => Co::Sonja,
            19 => Co::Sasha,
            20 => Co::Grimm,
            21 => Co::Koal,
            22 => Co::Jake,
            23 => Co::Kindle,
            24 => Co::Nell,
            25 => Co::Flak,
            26 => Co::Jugger,
            27 => Co::Javier,
            28 => Co::Rachel,
            29 => Co::Sturm,
            30 => Co::VonBolt,
            31 => Co::Neutral,
            _ => return None,
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_existing_awbw_co_ids() {
        assert_eq!(Co::from_awbw_id(AwbwCoId::new(1)), Some(Co::Andy));
        assert_eq!(Co::from_awbw_id(AwbwCoId::new(11)), Some(Co::Adder));
        assert_eq!(Co::from_awbw_id(AwbwCoId::new(30)), Some(Co::VonBolt));
        assert_eq!(Co::from_awbw_id(AwbwCoId::new(31)), Some(Co::Neutral));
        assert_eq!(Co::from_awbw_id(AwbwCoId::new(4)), None);
    }
}
