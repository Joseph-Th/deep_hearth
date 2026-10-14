//! Durable records for work that exclusively occupies the local player's attention.

mod block_excavation;
mod consumption;
mod gathering;
mod maintenance;
mod manual_power;
mod movement;
mod player;
mod prospecting;
mod storage_dismantling;
mod structural_construction;

pub use block_excavation::BlockExcavationWork;
pub use consumption::{DrinkingWork, EatingWork};
pub use gathering::SurfaceGatheringWork;
pub use maintenance::EquipmentMaintenanceWork;
pub use manual_power::ManualPowerWork;
pub use movement::PlayerStepWork;
pub use player::PlayerWork;
pub use prospecting::ProspectingWork;
pub use storage_dismantling::StorageEnclosureDismantlingWork;
pub use structural_construction::StructuralConstructionWork;
