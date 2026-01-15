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
