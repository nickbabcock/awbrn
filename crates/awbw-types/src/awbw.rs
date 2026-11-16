use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AwbwCoId(u32);

impl AwbwCoId {
    pub const fn new(x: u32) -> Self {
        Self(x)
    }

    pub const fn as_u32(self) -> u32 {
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
