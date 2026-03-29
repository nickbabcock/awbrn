#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoPortraitMetadata {
    key: &'static str,
    display_name: &'static str,
    awbw_id: u32,
}

impl CoPortraitMetadata {
    pub const fn new(key: &'static str, display_name: &'static str, awbw_id: u32) -> Self {
        Self {
            key,
            display_name,
            awbw_id,
        }
    }

    pub const fn key(&self) -> &'static str {
        self.key
    }

    pub const fn display_name(&self) -> &'static str {
        self.display_name
    }

    pub const fn awbw_id(&self) -> u32 {
        self.awbw_id
    }
}

include!("generated/co_portraits.rs");

pub fn co_portraits() -> &'static [CoPortraitMetadata] {
    &CO_PORTRAITS
}
