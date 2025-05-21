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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AwbwFactionId(u8);

impl AwbwFactionId {
    pub const fn new(x: u8) -> Self {
        Self(x)
    }

    pub const fn as_u8(&self) -> u8 {
        self.0
    }
}
