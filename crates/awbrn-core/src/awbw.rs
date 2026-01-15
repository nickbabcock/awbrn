use serde::{Deserialize, Serialize};

/// Global ID used across all games on AWBW
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AwbwPlayerId(u32);

impl AwbwPlayerId {
    pub const fn new(x: u32) -> Self {
        Self(x)
    }

    pub const fn as_u32(&self) -> u32 {
        self.0
    }
}

/// The ID of the player in a specific game
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AwbwGamePlayerId(u32);

impl AwbwGamePlayerId {
    pub const fn new(x: u32) -> Self {
        Self(x)
    }

    pub const fn as_u32(&self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AwbwGameId(u32);

impl AwbwGameId {
    pub const fn new(x: u32) -> Self {
        Self(x)
    }

    pub const fn as_u32(&self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct AwbwUnitId(u32);

impl AwbwUnitId {
    pub const fn new(x: u32) -> Self {
        Self(x)
    }

    pub const fn as_u32(&self) -> u32 {
        self.0
    }
}

impl<'de> Deserialize<'de> for AwbwUnitId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct UnitIdVisitor;

        impl serde::de::Visitor<'_> for UnitIdVisitor {
            type Value = AwbwUnitId;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a number or numeric string")
            }

            fn visit_u32<E>(self, value: u32) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(AwbwUnitId::new(value))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let val = u32::try_from(value)
                    .map_err(|_| E::custom(format!("Unit ID out of range: {}", value)))?;
                Ok(AwbwUnitId::new(val))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let val = u32::try_from(value)
                    .map_err(|_| E::custom(format!("Unit ID out of range: {}", value)))?;
                Ok(AwbwUnitId::new(val))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let val = value
                    .parse::<u32>()
                    .map_err(|_| E::custom(format!("Invalid unit ID: {}", value)))?;
                Ok(AwbwUnitId::new(val))
            }
        }

        deserializer.deserialize_any(UnitIdVisitor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AwbwMapId(u32);

impl AwbwMapId {
    pub const fn new(x: u32) -> Self {
        Self(x)
    }

    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

pub use awbrn_types::AwbwFactionId;
