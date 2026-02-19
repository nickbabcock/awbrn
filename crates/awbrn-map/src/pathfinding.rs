use crate::Position;
use awbrn_core::MovementTerrain;

/// A trait for maps that provide terrain information for pathfinding
pub trait MovementMap {
    /// Get the terrain at the specified coordinates
    fn terrain_at(&self, pos: Position) -> Option<MovementTerrain>;

    /// Get terrain at a pre-validated flat index (caller ensures idx < width * height)
    fn terrain_at_flat(&self, flat_idx: usize) -> MovementTerrain;

    fn width(&self) -> usize;

    fn height(&self) -> usize;
}

impl<T: MovementMap> MovementMap for &'_ T {
    fn terrain_at(&self, pos: Position) -> Option<MovementTerrain> {
        (**self).terrain_at(pos)
    }

    fn terrain_at_flat(&self, flat_idx: usize) -> MovementTerrain {
        (**self).terrain_at_flat(flat_idx)
    }

    fn width(&self) -> usize {
        (**self).width()
    }

    fn height(&self) -> usize {
        (**self).height()
    }
}

pub trait TerrainCosts {
    /// Return the cost for moving onto the specified terrain
    fn cost(&self, terrain: MovementTerrain) -> Option<u8>;
}

impl<T: TerrainCosts> TerrainCosts for &'_ T {
    fn cost(&self, terrain: MovementTerrain) -> Option<u8> {
        (**self).cost(terrain)
    }
}

pub struct Reachable<'a, M> {
    map: &'a mut PathFinder<M>,
    map_width: usize,
}

struct PositionIter<'a, M> {
    pathfinder: &'a mut PathFinder<M>,
    cursor: usize,
    map_width: usize,
}

impl<M: MovementMap> Iterator for PositionIter<'_, M> {
    type Item = (Position, u8);

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.pathfinder.visited.len().saturating_sub(self.cursor);
        (remaining, Some(remaining))
    }

    fn next(&mut self) -> Option<Self::Item> {
        let flat_idx = self.pathfinder.visited.get(self.cursor).copied()? as usize;
        self.cursor += 1;
        let cost = self.pathfinder.cost_map[flat_idx];
        let y = flat_idx / self.map_width;
        let x = flat_idx % self.map_width;
        Some((Position::new(x, y), cost))
    }
}

impl<'a, M: MovementMap> Reachable<'a, M> {
    pub fn into_positions(self) -> impl Iterator<Item = (Position, u8)> + 'a {
        PositionIter {
            pathfinder: self.map,
            cursor: 0,
            map_width: self.map_width,
        }
    }
}

pub struct PathFinder<M> {
    map: M,

    /// Bucket queue indexed by movement cost (Dial's algorithm)
    buckets: Vec<Vec<u32>>,

    /// Movement cost per flat index; u8::MAX means unvisited
    cost_map: Vec<u8>,

    /// Flat indices of all cells reached during the current search
    visited: Vec<u32>,
}

impl<M: MovementMap> PathFinder<M> {
    /// Create a new PathFinder
    pub fn new(map: M) -> Self {
        let map_size = map.width() * map.height();
        Self {
            map,
            buckets: Vec::with_capacity(256),
            cost_map: Vec::with_capacity(map_size),
            visited: Vec::with_capacity(map_size),
        }
    }

    /// Find reachable positions from the starting position with the given movement points
    pub fn reachable(
        &mut self,
        start: Position,
        movement_points: u8,
        costs: impl TerrainCosts,
    ) -> Reachable<'_, M> {
        let map_width = self.map.width();
        let map_size = self.map.height() * map_width;

        self.visited.clear();

        // First call: allocate and initialize cost_map
        if self.cost_map.len() != map_size {
            self.cost_map.clear();
            self.cost_map.resize(map_size, u8::MAX);
        } else {
            self.cost_map.fill(u8::MAX);
        }

        let num_buckets = movement_points as usize + 1;
        if self.buckets.len() < num_buckets {
            // New bucket Vecs get initial capacity to reduce reallocation churn
            let bucket_capacity = (map_size / num_buckets).max(16);
            self.buckets
                .resize_with(num_buckets, || Vec::with_capacity(bucket_capacity));
        }
        for bucket in self.buckets[..num_buckets].iter_mut() {
            bucket.clear();
        }

        let start_idx = start.y * map_width + start.x;
        if start_idx >= map_size {
            return Reachable {
                map: self,
                map_width,
            };
        }

        self.cost_map[start_idx] = 0;
        self.visited.push(start_idx as u32);
        self.buckets[0].push(start_idx as u32);

        let map_height = self.map.height();
        let mut current_cost = 0usize;

        while current_cost < num_buckets {
            if self.buckets[current_cost].is_empty() {
                current_cost += 1;
                continue;
            }

            // Take the batch to avoid borrow conflicts when 0-cost edges push back to this bucket
            let mut batch = std::mem::take(&mut self.buckets[current_cost]);

            for &flat_idx in &batch {
                let flat_idx = flat_idx as usize;

                // Skip stale entries: a better path was found after this entry was enqueued
                if self.cost_map[flat_idx] != current_cost as u8 {
                    continue;
                }

                // x for left/right boundary checks; y for up/down
                let x = flat_idx % map_width;
                let y = flat_idx / map_width;

                if x + 1 < map_width {
                    self.relax_neighbor(&costs, current_cost, num_buckets, flat_idx + 1);
                }
                if x > 0 {
                    self.relax_neighbor(&costs, current_cost, num_buckets, flat_idx - 1);
                }
                if y + 1 < map_height {
                    self.relax_neighbor(&costs, current_cost, num_buckets, flat_idx + map_width);
                }
                if y > 0 {
                    self.relax_neighbor(&costs, current_cost, num_buckets, flat_idx - map_width);
                }
            }

            batch.clear();
            // Check for 0-cost edge additions that landed back in the current bucket
            let remaining = std::mem::take(&mut self.buckets[current_cost]);
            if remaining.is_empty() {
                self.buckets[current_cost] = batch; // return capacity for reuse
                current_cost += 1;
            } else {
                // 0-cost edges added new entries; process them before advancing
                self.buckets[current_cost] = remaining;
            }
        }

        Reachable {
            map: self,
            map_width,
        }
    }

    /// edge relaxation
    #[inline(always)]
    fn relax_neighbor(
        &mut self,
        costs: &impl TerrainCosts,
        current_cost: usize,
        num_buckets: usize,
        new_flat: usize,
    ) {
        let terrain = self.map.terrain_at_flat(new_flat);
        if let Some(terrain_cost) = costs.cost(terrain) {
            let movement_cost = current_cost + terrain_cost as usize;
            if movement_cost < num_buckets {
                let c = self.cost_map[new_flat];
                if c == u8::MAX {
                    // First visit to this cell
                    self.cost_map[new_flat] = movement_cost as u8;
                    self.visited.push(new_flat as u32);
                    self.buckets[movement_cost].push(new_flat as u32);
                } else if movement_cost < c as usize {
                    // Better path found
                    self.cost_map[new_flat] = movement_cost as u8;
                    self.buckets[movement_cost].push(new_flat as u32);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::AwbwMap;
    use awbrn_core::{AwbwTerrain, MovementCost, MovementTerrain, RiverType, UnitMovement};
    use rstest::rstest;

    // Struct implementing TerrainCosts for testing
    struct UnitMovementCosts {
        movement_type: UnitMovement,
    }

    impl TerrainCosts for UnitMovementCosts {
        fn cost(&self, terrain: MovementTerrain) -> Option<u8> {
            MovementCost::from_terrain(&terrain).cost(self.movement_type)
        }
    }

    /// Test helper to create a map of the specified size and base terrain
    fn create_test_map(width: usize, height: usize, base_terrain: AwbwTerrain) -> AwbwMap {
        AwbwMap::new(width, height, base_terrain)
    }

    /// Helper function that tests movement for a unit and returns the reachable positions
    fn test_movement_pathfinder(
        map: &AwbwMap,
        start: Position,
        movement_type: UnitMovement,
        movement_points: u8,
    ) -> HashMap<Position, u8> {
        let costs = UnitMovementCosts { movement_type };
        let mut pathfinder = PathFinder::new(map);
        let reachable = pathfinder.reachable(start, movement_points, costs);
        reachable.into_positions().collect()
    }

    /// Helper to verify that positions are reachable with expected costs
    fn assert_positions_with_costs(positions: &HashMap<Position, u8>, expected: &[(Position, u8)]) {
        for (pos, expected_cost) in expected {
            let actual_cost = positions.get(pos);
            assert_eq!(
                actual_cost,
                Some(expected_cost),
                "Position {:?} should have cost {} but got {:?}",
                pos,
                expected_cost,
                actual_cost
            );
        }
    }

    #[rstest]
    #[case(UnitMovement::Foot, 3, vec![
        (Position::new(0, 0), 0), // start
        (Position::new(1, 0), 1), // right
        (Position::new(0, 1), 1), // down
        (Position::new(1, 1), 2), // diagonal
        (Position::new(2, 0), 2), // right 2
        (Position::new(0, 2), 2), // down 2
        (Position::new(3, 0), 3), // right 3
        (Position::new(0, 3), 3), // down 3
    ])]
    #[case(UnitMovement::Tires, 3, vec![
        (Position::new(0, 0), 0), // start
        (Position::new(1, 0), 2), // right (plains=2 for Tires)
        (Position::new(0, 1), 2), // down
    ])]
    #[case(UnitMovement::Air, 4, vec![
        (Position::new(0, 0), 0), // start
        (Position::new(1, 0), 1), // right
        (Position::new(0, 1), 1), // down
        (Position::new(2, 0), 2), // right 2
        (Position::new(0, 2), 2), // down 2
        (Position::new(3, 0), 3), // right 3
        (Position::new(0, 3), 3), // down 3
        (Position::new(4, 0), 4), // right 4
        (Position::new(0, 4), 4), // down 4
    ])]
    fn test_unit_movement_on_plains(
        #[case] movement_type: UnitMovement,
        #[case] movement_points: u8,
        #[case] expected_positions: Vec<(Position, u8)>,
    ) {
        let map = create_test_map(5, 5, AwbwTerrain::Plain);
        let positions =
            test_movement_pathfinder(&map, Position::new(0, 0), movement_type, movement_points);

        assert_positions_with_costs(&positions, &expected_positions);
    }

    #[rstest]
    #[case(UnitMovement::Foot, Position::new(0, 0), 3, vec![
        (Position::new(0, 0), 0), // start top left
        (Position::new(1, 0), 1), // right
        (Position::new(0, 1), 1), // down
    ])]
    #[case(UnitMovement::Foot, Position::new(4, 0), 3, vec![
        (Position::new(4, 0), 0), // start top right
        (Position::new(3, 0), 1), // left
        (Position::new(4, 1), 1), // down
    ])]
    #[case(UnitMovement::Foot, Position::new(0, 4), 3, vec![
        (Position::new(0, 4), 0), // start bottom left
        (Position::new(1, 4), 1), // right
        (Position::new(0, 3), 1), // up
    ])]
    #[case(UnitMovement::Foot, Position::new(4, 4), 3, vec![
        (Position::new(4, 4), 0), // start bottom right
        (Position::new(3, 4), 1), // left
        (Position::new(4, 3), 1), // up
    ])]
    fn test_movement_from_map_corners(
        #[case] movement_type: UnitMovement,
        #[case] start_pos: Position,
        #[case] movement_points: u8,
        #[case] expected_positions: Vec<(Position, u8)>,
    ) {
        let map = create_test_map(5, 5, AwbwTerrain::Plain);
        let positions = test_movement_pathfinder(&map, start_pos, movement_type, movement_points);

        assert_positions_with_costs(&positions, &expected_positions);
    }

    #[rstest]
    #[case(UnitMovement::Foot, 3, vec![
        (Position::new(2, 2), 0), // start (in mountain)
        (Position::new(1, 2), 1), // left
        (Position::new(2, 1), 1), // up
        (Position::new(3, 2), 1), // right
    ])]
    #[case(UnitMovement::Boot, 3, vec![
        (Position::new(2, 2), 0), // start (in mountain)
        (Position::new(1, 2), 1), // left
        (Position::new(2, 1), 1), // up
        (Position::new(3, 2), 1), // right
        (Position::new(2, 3), 1), // down (river - boot only)
    ])]
    #[case(UnitMovement::Sea, 2, vec![
        (Position::new(1, 4), 0), // start (in sea)
        (Position::new(2, 4), 1), // right
    ])]
    fn test_movement_on_mixed_terrain(
        #[case] movement_type: UnitMovement,
        #[case] movement_points: u8,
        #[case] expected_positions: Vec<(Position, u8)>,
    ) {
        // Create mixed terrain map with mountains, rivers, and sea
        let mut map = AwbwMap::new(5, 5, AwbwTerrain::Plain);

        // Set center as mountain
        *map.terrain_at_mut(Position::new(2, 2)).unwrap() = AwbwTerrain::Mountain;

        // Add rivers (row 3)
        *map.terrain_at_mut(Position::new(2, 3)).unwrap() =
            AwbwTerrain::River(RiverType::Horizontal);
        *map.terrain_at_mut(Position::new(3, 3)).unwrap() =
            AwbwTerrain::River(RiverType::Horizontal);

        // Add sea (row 4)
        *map.terrain_at_mut(Position::new(1, 4)).unwrap() = AwbwTerrain::Sea;
        *map.terrain_at_mut(Position::new(2, 4)).unwrap() = AwbwTerrain::Sea;

        // Different starting positions based on unit type
        let start_pos = match movement_type {
            UnitMovement::Sea => Position::new(1, 4), // Start in sea for sea units
            _ => Position::new(2, 2),                 // Start in mountain for other units
        };

        let positions = test_movement_pathfinder(&map, start_pos, movement_type, movement_points);
        assert_positions_with_costs(&positions, &expected_positions);
    }

    #[test]
    fn test_unreachable_terrain() {
        let mut map = AwbwMap::new(3, 3, AwbwTerrain::Sea);

        // Create a plain tile in the center
        *map.terrain_at_mut(Position::new(1, 1)).unwrap() = AwbwTerrain::Plain;

        // Test foot unit in the center - should be trapped by sea
        let costs = UnitMovementCosts {
            movement_type: UnitMovement::Foot,
        };
        let mut pathfinder = PathFinder::new(&map);
        let reachable = pathfinder.reachable(Position::new(1, 1), 5, costs);
        let positions: HashMap<Position, u8> = reachable.into_positions().collect();

        // Should only be able to reach the starting position
        assert_eq!(positions.len(), 1);
        assert_eq!(positions.get(&Position::new(1, 1)), Some(&0));
    }

    #[test]
    fn test_pathfinder_reuse() {
        let map = AwbwMap::new(5, 5, AwbwTerrain::Plain);

        // Create a PathFinder that we'll reuse
        let mut pathfinder = PathFinder::new(&map);

        // Test foot unit movement
        let foot_costs = UnitMovementCosts {
            movement_type: UnitMovement::Foot,
        };
        let reachable1 = pathfinder.reachable(Position::new(0, 0), 3, &foot_costs);
        let positions1: HashMap<Position, u8> = reachable1.into_positions().collect();

        // Test a different starting position with the same pathfinder
        let reachable2 = pathfinder.reachable(Position::new(4, 4), 2, &foot_costs);
        let positions2: HashMap<Position, u8> = reachable2.into_positions().collect();

        // Results should be different
        assert_ne!(positions1.len(), positions2.len());

        // Positions should match expected values
        assert!(positions1.contains_key(&Position::new(0, 0)));
        assert!(!positions1.contains_key(&Position::new(4, 4)));

        assert!(!positions2.contains_key(&Position::new(0, 0)));
        assert!(positions2.contains_key(&Position::new(4, 4)));
    }

    #[test]
    fn test_into_positions() {
        let map = AwbwMap::new(5, 5, AwbwTerrain::Plain);
        let mut pathfinder = PathFinder::new(&map);

        // Create a context
        let foot_costs = UnitMovementCosts {
            movement_type: UnitMovement::Foot,
        };
        let reachable = pathfinder.reachable(Position::new(0, 0), 2, &foot_costs);

        // Convert to positions and drain the cost_map
        let positions: Vec<(Position, u8)> = reachable.into_positions().collect();

        // Verify we got positions
        assert!(!positions.is_empty());

        // Verify we have the start position with cost 0
        assert!(positions.contains(&(Position::new(0, 0), 0)));

        // Verify adjacent positions have cost 1
        assert!(positions.contains(&(Position::new(1, 0), 1)));
        assert!(positions.contains(&(Position::new(0, 1), 1)));

        // Verify diagonal positions have cost 2
        assert!(positions.contains(&(Position::new(1, 1), 2)));
    }

    #[test]
    fn test_into_positions_size_hint() {
        let map = AwbwMap::new(5, 5, AwbwTerrain::Plain);
        let mut pathfinder = PathFinder::new(&map);

        let foot_costs = UnitMovementCosts {
            movement_type: UnitMovement::Foot,
        };
        let reachable = pathfinder.reachable(Position::new(0, 0), 2, &foot_costs);
        let mut iter = reachable.into_positions();

        assert_eq!(iter.size_hint(), (6, Some(6)));

        assert!(iter.next().is_some());
        assert_eq!(iter.size_hint(), (5, Some(5)));

        for _ in 0..5 {
            assert!(iter.next().is_some());
        }

        assert_eq!(iter.size_hint(), (0, Some(0)));
        assert_eq!(iter.next(), None);
    }
}
