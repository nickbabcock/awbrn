use crate::Position;
use awbrn_core::MovementTerrain;
use std::{cmp::Reverse, collections::BinaryHeap};

/// A trait for maps that provide terrain information for pathfinding
pub trait MovementMap {
    /// Get the terrain at the specified coordinates
    fn terrain_at(&self, pos: Position) -> Option<MovementTerrain>;

    fn width(&self) -> usize;

    fn height(&self) -> usize;
}

impl<T: MovementMap> MovementMap for &'_ T {
    fn terrain_at(&self, pos: Position) -> Option<MovementTerrain> {
        (**self).terrain_at(pos)
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
}

impl<M: MovementMap> Reachable<'_, M> {
    pub fn into_positions(self) -> impl Iterator<Item = (Position, u8)> {
        self.map
            .cost_map
            .drain(..)
            .enumerate()
            .filter_map(|(idx, cost)| cost.map(|c| (idx, c)))
            .map(|(idx, cost)| {
                let y = idx / self.map.map.width();
                let x = idx % self.map.map.width();
                (Position::new(x, y), cost)
            })
    }
}

pub struct PathFinder<M> {
    map: M,

    queue: BinaryHeap<(Reverse<u8>, Position)>,

    /// Map of position to cost
    cost_map: Vec<Option<u8>>,
}

impl<M: MovementMap> PathFinder<M> {
    /// Create a new PathFinder
    pub fn new(map: M) -> Self {
        Self {
            map,
            queue: BinaryHeap::new(),
            cost_map: Vec::new(),
        }
    }

    /// Find reachable positions from the starting position with the given movement points
    pub fn reachable(
        &mut self,
        start: Position,
        movement_points: u8,
        costs: impl TerrainCosts,
    ) -> Reachable<M> {
        self.cost_map.clear();
        self.cost_map
            .resize(self.map.height() * self.map.width(), None);
        self.queue.clear();

        if let Some(index) = self.cost_map.get_mut(start.y * self.map.width() + start.x) {
            *index = Some(0);
        } else {
            return Reachable { map: self };
        }

        self.queue.push((Reverse(0), start));

        // Directions to explore: down, right, up, left
        let directions = [(0, 1), (1, 0), (0, -1), (-1, 0)];
        while let Some((Reverse(current_cost), current)) = self.queue.pop() {
            // Optimization: If we pop an element whose cost is already higher
            // than a known shorter path to it, skip it. This happens if we
            // added multiple entries for the same node to the queue before finding
            // the absolute shortest path.
            let index = current.y * self.map.width() + current.x;
            if matches!(self.cost_map[index], Some(known_cost) if current_cost > known_cost) {
                continue;
            }

            for (dx, dy) in &directions {
                let new_pos = current.movement(*dx, *dy);

                let Some(terrain) = self.map.terrain_at(new_pos) else {
                    continue;
                };

                let Some(terrain_cost) = costs.cost(terrain) else {
                    continue;
                };
                let movement_cost = current_cost + terrain_cost;

                if movement_cost > movement_points {
                    continue;
                }

                let index = new_pos.y * self.map.width() + new_pos.x;
                match &mut self.cost_map[index] {
                    Some(c) if movement_cost < *c => *c = movement_cost,
                    Some(_) => continue,
                    cost @ None => *cost = Some(movement_cost),
                }

                self.queue.push((Reverse(movement_cost), new_pos));
            }
        }

        Reachable { map: self }
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
}
