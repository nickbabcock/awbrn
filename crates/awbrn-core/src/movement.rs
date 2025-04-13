use crate::MovementTerrain;

/// Represents different movement capabilities of units
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnitMovement {
    Foot,   // Infantry
    Boot,   // Mech
    Treads, // Tank-type units
    Tires,  // Wheeled vehicles
    Sea,    // Ships
    Lander, // Transport ships
    Air,    // Flying units
    Pipe,   // Pipe units
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MovementCost {
    costs: [Option<u8>; 11],
}

impl MovementCost {
    pub const PLAINS: MovementCost = PLAINS_MOVEMENT;
    pub const MOUNTAINS: MovementCost = MOUNTAINS_MOVEMENT;
    pub const WOODS: MovementCost = WOODS_MOVEMENT;
    pub const RIVERS: MovementCost = RIVERS_MOVEMENT;
    pub const INFRASTRUCTURE: MovementCost = INFRASTRUCTURE_MOVEMENT;
    pub const SEA: MovementCost = SEA_MOVEMENT;
    pub const SHOALS: MovementCost = SHOALS_MOVEMENT;
    pub const REEFS: MovementCost = REEFS_MOVEMENT;
    pub const PIPES: MovementCost = PIPES_MOVEMENT;
    pub const TELEPORT: MovementCost = TELEPORT_MOVEMENT;

    pub const fn new(data: &[(UnitMovement, Option<u8>)]) -> Self {
        let mut costs = [None; 11];
        let mut i = 0;
        while i < data.len() {
            let (movement_type, cost) = data[i];
            costs[movement_type as usize] = cost;
            i += 1;
        }
        MovementCost { costs }
    }

    pub const fn from_terrain(terrain: &MovementTerrain) -> Self {
        match terrain {
            MovementTerrain::Plains => Self::PLAINS,
            MovementTerrain::Mountains => Self::MOUNTAINS,
            MovementTerrain::Woods => Self::WOODS,
            MovementTerrain::Rivers => Self::RIVERS,
            MovementTerrain::Infrastructure => Self::INFRASTRUCTURE,
            MovementTerrain::Sea => Self::SEA,
            MovementTerrain::Shoals => Self::SHOALS,
            MovementTerrain::Reefs => Self::REEFS,
            MovementTerrain::Pipes => Self::PIPES,
            MovementTerrain::Teleport => Self::TELEPORT,
        }
    }

    pub fn cost(&self, movement_type: UnitMovement) -> Option<u8> {
        self.costs[movement_type as usize]
    }
}

const PLAINS_MOVEMENT: MovementCost = MovementCost::new(&[
    (UnitMovement::Foot, Some(1)),
    (UnitMovement::Boot, Some(1)),
    (UnitMovement::Treads, Some(1)),
    (UnitMovement::Tires, Some(2)),
    (UnitMovement::Air, Some(1)),
]);

const MOUNTAINS_MOVEMENT: MovementCost = MovementCost::new(&[
    (UnitMovement::Foot, Some(2)),
    (UnitMovement::Boot, Some(1)),
    (UnitMovement::Air, Some(1)),
]);

const WOODS_MOVEMENT: MovementCost = MovementCost::new(&[
    (UnitMovement::Foot, Some(1)),
    (UnitMovement::Boot, Some(1)),
    (UnitMovement::Treads, Some(2)),
    (UnitMovement::Tires, Some(3)),
    (UnitMovement::Air, Some(1)),
]);

const RIVERS_MOVEMENT: MovementCost = MovementCost::new(&[
    (UnitMovement::Foot, Some(2)),
    (UnitMovement::Boot, Some(1)),
    (UnitMovement::Air, Some(1)),
]);

const INFRASTRUCTURE_MOVEMENT: MovementCost = MovementCost::new(&[
    (UnitMovement::Foot, Some(1)),
    (UnitMovement::Boot, Some(1)),
    (UnitMovement::Treads, Some(1)),
    (UnitMovement::Tires, Some(1)),
    (UnitMovement::Air, Some(1)),
]);

const SEA_MOVEMENT: MovementCost = MovementCost::new(&[
    (UnitMovement::Air, Some(1)),
    (UnitMovement::Sea, Some(1)),
    (UnitMovement::Lander, Some(1)),
]);

const SHOALS_MOVEMENT: MovementCost = MovementCost::new(&[
    (UnitMovement::Foot, Some(1)),
    (UnitMovement::Boot, Some(1)),
    (UnitMovement::Treads, Some(1)),
    (UnitMovement::Tires, Some(1)),
    (UnitMovement::Air, Some(1)),
    (UnitMovement::Lander, Some(1)),
]);

const REEFS_MOVEMENT: MovementCost = MovementCost::new(&[
    (UnitMovement::Air, Some(1)),
    (UnitMovement::Sea, Some(2)),
    (UnitMovement::Lander, Some(2)),
]);

const PIPES_MOVEMENT: MovementCost = MovementCost::new(&[(UnitMovement::Pipe, Some(1))]);

const TELEPORT_MOVEMENT: MovementCost = MovementCost::new(&[
    (UnitMovement::Foot, Some(0)),
    (UnitMovement::Boot, Some(0)),
    (UnitMovement::Treads, Some(0)),
    (UnitMovement::Tires, Some(0)),
    (UnitMovement::Air, Some(0)),
    (UnitMovement::Sea, Some(0)),
    (UnitMovement::Lander, Some(0)),
    (UnitMovement::Pipe, Some(0)),
]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MovementTerrain;

    #[test]
    fn test_movement_cost_new() {
        // Test creating a new MovementCost with specified costs
        let data = vec![
            (UnitMovement::Foot, Some(1)),
            (UnitMovement::Boot, Some(2)),
            (UnitMovement::Air, Some(3)),
        ];

        let movement_cost = MovementCost::new(&data);

        // Check that specified costs are set correctly
        assert_eq!(movement_cost.cost(UnitMovement::Foot), Some(1));
        assert_eq!(movement_cost.cost(UnitMovement::Boot), Some(2));
        assert_eq!(movement_cost.cost(UnitMovement::Air), Some(3));

        // Check that unspecified costs are None
        assert_eq!(movement_cost.cost(UnitMovement::Treads), None);
        assert_eq!(movement_cost.cost(UnitMovement::Tires), None);
        assert_eq!(movement_cost.cost(UnitMovement::Sea), None);
        assert_eq!(movement_cost.cost(UnitMovement::Lander), None);
        assert_eq!(movement_cost.cost(UnitMovement::Pipe), None);
    }

    #[test]
    fn test_movement_cost_empty() {
        // Test creating an empty MovementCost
        let empty_data: Vec<(UnitMovement, Option<u8>)> = vec![];
        let empty_cost = MovementCost::new(&empty_data);

        // All costs should be None
        assert_eq!(empty_cost.cost(UnitMovement::Foot), None);
        assert_eq!(empty_cost.cost(UnitMovement::Boot), None);
        assert_eq!(empty_cost.cost(UnitMovement::Treads), None);
        assert_eq!(empty_cost.cost(UnitMovement::Tires), None);
        assert_eq!(empty_cost.cost(UnitMovement::Sea), None);
        assert_eq!(empty_cost.cost(UnitMovement::Lander), None);
        assert_eq!(empty_cost.cost(UnitMovement::Air), None);
        assert_eq!(empty_cost.cost(UnitMovement::Pipe), None);
    }

    #[test]
    fn test_movement_cost_none_values() {
        // Test creating a MovementCost with explicit None values
        let data = vec![
            (UnitMovement::Foot, Some(1)),
            (UnitMovement::Boot, None),
            (UnitMovement::Treads, Some(2)),
        ];

        let movement_cost = MovementCost::new(&data);

        // Verify that explicit None is preserved
        assert_eq!(movement_cost.cost(UnitMovement::Foot), Some(1));
        assert_eq!(movement_cost.cost(UnitMovement::Boot), None);
        assert_eq!(movement_cost.cost(UnitMovement::Treads), Some(2));
    }

    #[test]
    fn test_plains_movement_costs() {
        // Test predefined PLAINS movement costs
        assert_eq!(MovementCost::PLAINS.cost(UnitMovement::Foot), Some(1));
        assert_eq!(MovementCost::PLAINS.cost(UnitMovement::Boot), Some(1));
        assert_eq!(MovementCost::PLAINS.cost(UnitMovement::Treads), Some(1));
        assert_eq!(MovementCost::PLAINS.cost(UnitMovement::Tires), Some(2));
        assert_eq!(MovementCost::PLAINS.cost(UnitMovement::Air), Some(1));

        // Sea units shouldn't be able to move on plains
        assert_eq!(MovementCost::PLAINS.cost(UnitMovement::Sea), None);
        assert_eq!(MovementCost::PLAINS.cost(UnitMovement::Lander), None);

        // Pipe units shouldn't be able to move on plains
        assert_eq!(MovementCost::PLAINS.cost(UnitMovement::Pipe), None);
    }

    #[test]
    fn test_mountains_movement_costs() {
        // Test predefined MOUNTAINS movement costs
        assert_eq!(MovementCost::MOUNTAINS.cost(UnitMovement::Foot), Some(2));
        assert_eq!(MovementCost::MOUNTAINS.cost(UnitMovement::Boot), Some(1));
        assert_eq!(MovementCost::MOUNTAINS.cost(UnitMovement::Air), Some(1));

        // Vehicles shouldn't be able to move on mountains
        assert_eq!(MovementCost::MOUNTAINS.cost(UnitMovement::Treads), None);
        assert_eq!(MovementCost::MOUNTAINS.cost(UnitMovement::Tires), None);

        // Sea units shouldn't be able to move on mountains
        assert_eq!(MovementCost::MOUNTAINS.cost(UnitMovement::Sea), None);
        assert_eq!(MovementCost::MOUNTAINS.cost(UnitMovement::Lander), None);
    }

    #[test]
    fn test_sea_movement_costs() {
        // Test predefined SEA movement costs
        assert_eq!(MovementCost::SEA.cost(UnitMovement::Air), Some(1));
        assert_eq!(MovementCost::SEA.cost(UnitMovement::Sea), Some(1));
        assert_eq!(MovementCost::SEA.cost(UnitMovement::Lander), Some(1));

        // Land units shouldn't be able to move on sea
        assert_eq!(MovementCost::SEA.cost(UnitMovement::Foot), None);
        assert_eq!(MovementCost::SEA.cost(UnitMovement::Boot), None);
        assert_eq!(MovementCost::SEA.cost(UnitMovement::Treads), None);
        assert_eq!(MovementCost::SEA.cost(UnitMovement::Tires), None);
    }

    #[test]
    fn test_pipes_movement_costs() {
        // Test predefined PIPES movement costs
        assert_eq!(MovementCost::PIPES.cost(UnitMovement::Pipe), Some(1));

        // Other units shouldn't be able to move in pipes
        assert_eq!(MovementCost::PIPES.cost(UnitMovement::Foot), None);
        assert_eq!(MovementCost::PIPES.cost(UnitMovement::Boot), None);
        assert_eq!(MovementCost::PIPES.cost(UnitMovement::Treads), None);
        assert_eq!(MovementCost::PIPES.cost(UnitMovement::Tires), None);
        assert_eq!(MovementCost::PIPES.cost(UnitMovement::Air), None);
        assert_eq!(MovementCost::PIPES.cost(UnitMovement::Sea), None);
        assert_eq!(MovementCost::PIPES.cost(UnitMovement::Lander), None);
    }

    #[test]
    fn test_teleport_movement_costs() {
        // Test predefined TELEPORT movement costs (all should be 0)
        assert_eq!(MovementCost::TELEPORT.cost(UnitMovement::Foot), Some(0));
        assert_eq!(MovementCost::TELEPORT.cost(UnitMovement::Boot), Some(0));
        assert_eq!(MovementCost::TELEPORT.cost(UnitMovement::Treads), Some(0));
        assert_eq!(MovementCost::TELEPORT.cost(UnitMovement::Tires), Some(0));
        assert_eq!(MovementCost::TELEPORT.cost(UnitMovement::Air), Some(0));
        assert_eq!(MovementCost::TELEPORT.cost(UnitMovement::Sea), Some(0));
        assert_eq!(MovementCost::TELEPORT.cost(UnitMovement::Lander), Some(0));
        assert_eq!(MovementCost::TELEPORT.cost(UnitMovement::Pipe), Some(0));
    }

    #[test]
    fn test_from_terrain() {
        // Test that from_terrain method returns the correct MovementCost
        assert_eq!(
            MovementCost::from_terrain(&MovementTerrain::Plains),
            MovementCost::PLAINS
        );
        assert_eq!(
            MovementCost::from_terrain(&MovementTerrain::Mountains),
            MovementCost::MOUNTAINS
        );
        assert_eq!(
            MovementCost::from_terrain(&MovementTerrain::Woods),
            MovementCost::WOODS
        );
        assert_eq!(
            MovementCost::from_terrain(&MovementTerrain::Rivers),
            MovementCost::RIVERS
        );
        assert_eq!(
            MovementCost::from_terrain(&MovementTerrain::Infrastructure),
            MovementCost::INFRASTRUCTURE
        );
        assert_eq!(
            MovementCost::from_terrain(&MovementTerrain::Sea),
            MovementCost::SEA
        );
        assert_eq!(
            MovementCost::from_terrain(&MovementTerrain::Shoals),
            MovementCost::SHOALS
        );
        assert_eq!(
            MovementCost::from_terrain(&MovementTerrain::Reefs),
            MovementCost::REEFS
        );
        assert_eq!(
            MovementCost::from_terrain(&MovementTerrain::Pipes),
            MovementCost::PIPES
        );
        assert_eq!(
            MovementCost::from_terrain(&MovementTerrain::Teleport),
            MovementCost::TELEPORT
        );
    }

    #[test]
    fn test_shoals_movement_costs() {
        // Test predefined SHOALS movement costs
        assert_eq!(MovementCost::SHOALS.cost(UnitMovement::Foot), Some(1));
        assert_eq!(MovementCost::SHOALS.cost(UnitMovement::Boot), Some(1));
        assert_eq!(MovementCost::SHOALS.cost(UnitMovement::Treads), Some(1));
        assert_eq!(MovementCost::SHOALS.cost(UnitMovement::Tires), Some(1));
        assert_eq!(MovementCost::SHOALS.cost(UnitMovement::Air), Some(1));
        assert_eq!(MovementCost::SHOALS.cost(UnitMovement::Lander), Some(1));

        // Regular ships can't navigate in shoals
        assert_eq!(MovementCost::SHOALS.cost(UnitMovement::Sea), None);
    }

    #[test]
    fn test_reefs_movement_costs() {
        // Test predefined REEFS movement costs
        assert_eq!(MovementCost::REEFS.cost(UnitMovement::Air), Some(1));
        assert_eq!(MovementCost::REEFS.cost(UnitMovement::Sea), Some(2));
        assert_eq!(MovementCost::REEFS.cost(UnitMovement::Lander), Some(2));

        // Land units can't navigate reefs
        assert_eq!(MovementCost::REEFS.cost(UnitMovement::Foot), None);
        assert_eq!(MovementCost::REEFS.cost(UnitMovement::Boot), None);
        assert_eq!(MovementCost::REEFS.cost(UnitMovement::Treads), None);
        assert_eq!(MovementCost::REEFS.cost(UnitMovement::Tires), None);
    }
}
