#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Default)]
pub enum Weather {
    #[default]
    Clear,
    Rain,
    Snow,
}
