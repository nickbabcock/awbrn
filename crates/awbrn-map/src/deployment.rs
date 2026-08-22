//! Units that a map places before the first turn.

use awbrn_types::{PlayerFaction, Unit, UnitExt, VisualHp};
use awvm::semantic::{CellIdx, Dimensions, Pos};

use crate::MapError;
use crate::awbw_map::PredeployedUnit;

/// The highest HP a map may give a unit, on the map's 1 to 10 scale.
pub(crate) const MAX_UNIT_HP: u32 = 10;

/// One unit that a map places before the first turn.
///
/// The tile is not a field here. A deployment is what stands on a tile, and
/// [`Deployments`] keys it by that tile, which is what turns "one unit for each
/// tile" into a property of the collection instead of a pass over a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deployment {
    /// The unit type.
    pub unit: Unit,
    /// The unit HP on the map's 1 to 10 scale.
    pub hp: VisualHp,
    /// The faction that owns the unit.
    pub faction: PlayerFaction,
}

/// The units a map places before the first turn, at most one for each tile.
///
/// Entries are held sorted by cell, so iteration is row-major without a sort
/// and a lookup is a binary search. Row-major order is what the map document's
/// digests hash, so keeping it here is what lets the hashing code state the
/// order once rather than restate it for each preimage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deployments {
    dimensions: Dimensions,
    entries: Vec<(CellIdx, Deployment)>,
}

impl Deployments {
    /// An empty set of deployments over a board of this shape.
    pub fn new(dimensions: Dimensions) -> Self {
        Self {
            dimensions,
            entries: Vec::new(),
        }
    }

    /// The board shape these deployments are keyed against.
    pub fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    /// Places `deployment` on `position`.
    ///
    /// Fails when the tile is off the board or already holds a unit.
    pub fn insert(&mut self, position: Pos, deployment: Deployment) -> Result<(), MapError> {
        let cell =
            self.dimensions
                .cell_index(position)
                .ok_or_else(|| MapError::UnitOutOfBounds {
                    x: u32::from(position.x),
                    y: u32::from(position.y),
                })?;

        match self.entries.binary_search_by_key(&cell, |(cell, _)| *cell) {
            Ok(_) => Err(MapError::UnitPositionOccupied {
                x: u32::from(position.x),
                y: u32::from(position.y),
            }),
            Err(slot) => {
                self.entries.insert(slot, (cell, deployment));
                Ok(())
            }
        }
    }

    /// The unit standing on `position`, if any.
    pub fn get(&self, position: Pos) -> Option<&Deployment> {
        let cell = self.dimensions.cell_index(position)?;
        self.entries
            .binary_search_by_key(&cell, |(cell, _)| *cell)
            .ok()
            .map(|slot| &self.entries[slot].1)
    }

    /// How many units the map places.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the map places no units at all.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every unit with the tile it stands on, row by row.
    pub fn iter(&self) -> impl Iterator<Item = (Pos, &Deployment)> {
        self.entries.iter().map(|(cell, deployment)| {
            let position = self
                .dimensions
                .position_of(*cell)
                .expect("a cell was minted by these dimensions");
            (position, deployment)
        })
    }

    /// The same deployments with each faction replaced by `map_faction`.
    ///
    /// Tiles are untouched, so the one-unit-for-each-tile property is kept
    /// without rebuilding the index.
    pub(crate) fn map_factions(
        &self,
        mut map_faction: impl FnMut(PlayerFaction) -> PlayerFaction,
    ) -> Self {
        Self {
            dimensions: self.dimensions,
            entries: self
                .entries
                .iter()
                .map(|(cell, deployment)| {
                    (
                        *cell,
                        Deployment {
                            faction: map_faction(deployment.faction),
                            ..*deployment
                        },
                    )
                })
                .collect(),
        }
    }
}

impl Deployment {
    /// Converts one AWBW predeployed unit, with the tile it names.
    pub(crate) fn from_predeployed(entry: &PredeployedUnit) -> Result<(Pos, Self), MapError> {
        let unit = Unit::from_awbw_id(entry.unit_id)
            .ok_or(MapError::UnknownUnitId { id: entry.unit_id })?;
        let faction = PlayerFaction::from_country_code(&entry.country_code).ok_or_else(|| {
            MapError::UnknownCountryCode {
                code: entry.country_code.clone(),
            }
        })?;

        // A coordinate past `u8` cannot name a tile of any board the VM runs,
        // so it is out of bounds before a board shape is consulted.
        let position = u8::try_from(entry.unit_x)
            .ok()
            .zip(u8::try_from(entry.unit_y).ok())
            .map(|(x, y)| Pos::new(x, y))
            .ok_or(MapError::UnitOutOfBounds {
                x: entry.unit_x,
                y: entry.unit_y,
            })?;

        if entry.unit_hp == 0 || entry.unit_hp > MAX_UNIT_HP {
            return Err(MapError::UnitHpOutOfRange {
                x: entry.unit_x,
                y: entry.unit_y,
                hp: entry.unit_hp,
            });
        }

        Ok((
            position,
            Self {
                unit,
                hp: VisualHp::new(entry.unit_hp as u8),
                faction,
            },
        ))
    }
}
