use awbrn_types::GraphicalTerrain;
use awvm::semantic::{Dimensions, Grid, Pos};
use std::ops::{Index, IndexMut};

/// The terrain a single viewpoint remembers, one entry per board position.
///
/// A projection reports a fogged tile's terrain but not its owner
/// (`spec/semantics/fog.md`), so the property sprite a viewer remembers is
/// presentation memory that the observation itself cannot supply. The grid is
/// sized once from the board it was built for; a position outside those
/// dimensions reads as unknown rather than panicking, so a caller that outlives
/// a board reshape falls back to the actual terrain instead of a stale one.
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainKnowledge {
    terrain: Grid<GraphicalTerrain>,
}

impl TerrainKnowledge {
    /// Seed a viewpoint's memory with what the board currently holds.
    pub fn from_fn(dimensions: Dimensions, terrain: impl FnMut(Pos) -> GraphicalTerrain) -> Self {
        Self {
            terrain: Grid::from_fn(dimensions, terrain),
        }
    }

    pub const fn dimensions(&self) -> Dimensions {
        self.terrain.dimensions()
    }

    pub fn get(&self, position: Pos) -> Option<&GraphicalTerrain> {
        self.terrain.get(position)
    }

    /// Record what the viewpoint sees now, returning what it remembered before.
    pub fn insert(&mut self, position: Pos, terrain: GraphicalTerrain) -> Option<GraphicalTerrain> {
        self.terrain
            .get_mut(position)
            .map(|slot| std::mem::replace(slot, terrain))
    }
}

impl Index<Pos> for TerrainKnowledge {
    type Output = GraphicalTerrain;

    fn index(&self, index: Pos) -> &Self::Output {
        &self.terrain[index]
    }
}

impl Index<&Pos> for TerrainKnowledge {
    type Output = GraphicalTerrain;

    fn index(&self, index: &Pos) -> &Self::Output {
        &self.terrain[*index]
    }
}

impl IndexMut<Pos> for TerrainKnowledge {
    fn index_mut(&mut self, index: Pos) -> &mut Self::Output {
        &mut self.terrain[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_types::{Faction, Property};

    #[test]
    fn insert_returns_what_the_viewpoint_remembered() {
        let mut knowledge =
            TerrainKnowledge::from_fn(Dimensions::new(2, 2), |_| GraphicalTerrain::Plain);
        let city = GraphicalTerrain::Property(Property::City(Faction::Neutral));

        assert_eq!(
            knowledge.insert(Pos::new(1, 1), city),
            Some(GraphicalTerrain::Plain)
        );
        assert_eq!(knowledge[Pos::new(1, 1)], city);
        assert_eq!(knowledge[Pos::new(0, 0)], GraphicalTerrain::Plain);
    }

    #[test]
    fn a_position_off_the_board_reads_as_unknown() {
        let mut knowledge =
            TerrainKnowledge::from_fn(Dimensions::new(2, 2), |_| GraphicalTerrain::Plain);

        assert_eq!(knowledge.get(Pos::new(2, 0)), None);
        assert_eq!(
            knowledge.insert(Pos::new(2, 0), GraphicalTerrain::Mountain),
            None
        );
    }
}
