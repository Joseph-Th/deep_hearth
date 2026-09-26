//! Durable records for work that exclusively occupies the local player's attention.

mod consumption;
mod maintenance;
mod manual_power;
mod player;
mod prospecting;
mod storage_dismantling;

pub use consumption::{DrinkingWork, EatingWork};
pub use maintenance::EquipmentMaintenanceWork;
pub use manual_power::ManualPowerWork;
pub use player::PlayerWork;
pub use prospecting::ProspectingWork;
pub use storage_dismantling::StorageEnclosureDismantlingWork;
