//! Serde for a [`Pos`] written as `{"x": …, "y": …}`.
//!
//! The VM spells a coordinate `[x, y]`, which `spec/model/violations.md` makes
//! canonical, and [`Pos`] serializes that way. The browser wire spells the same
//! coordinate as an object and has TypeScript declarations that say so. Both
//! spellings name one type; this module is where the second one is written, so
//! that keeping the wire stable costs a field attribute rather than a second
//! coordinate type.
//!
//! Use it as `#[serde(with = "awbrn_map::xy")]`, or `#[serde(with =
//! "awbrn_map::xy::vec")]` for a path.

use awvm::semantic::Pos;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The object spelling, which is what is actually read and written.
#[derive(Serialize, Deserialize)]
struct Xy {
    x: u8,
    y: u8,
}

impl From<Pos> for Xy {
    fn from(position: Pos) -> Self {
        Self {
            x: position.x,
            y: position.y,
        }
    }
}

impl From<Xy> for Pos {
    fn from(xy: Xy) -> Self {
        Pos::new(xy.x, xy.y)
    }
}

pub fn serialize<S: Serializer>(position: &Pos, serializer: S) -> Result<S::Ok, S::Error> {
    Xy::from(*position).serialize(serializer)
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Pos, D::Error> {
    Xy::deserialize(deserializer).map(Pos::from)
}

/// The same spelling for a sequence of coordinates, such as a move path.
pub mod vec {
    use super::{Pos, Xy};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(positions: &[Pos], serializer: S) -> Result<S::Ok, S::Error> {
        positions
            .iter()
            .map(|position| Xy::from(*position))
            .collect::<Vec<_>>()
            .serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Pos>, D::Error> {
        Vec::<Xy>::deserialize(deserializer)
            .map(|entries| entries.into_iter().map(Pos::from).collect())
    }
}
