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
