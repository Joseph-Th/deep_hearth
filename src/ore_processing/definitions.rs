//! Immutable ore/material-preparation process definitions shared by the runtime resolvers.

mod comminution;
mod operating;
mod precipitation;
mod screening;
mod separation;

pub use comminution::{ComminutionProcessDefinition, ManualComminutionProcessDefinition};
pub use operating::{ManualOreEquipmentProfile, ManualOreProcessProfile, PoweredOreProcessProfile};
pub use precipitation::PrecipitationProcessDefinition;
pub use screening::ScreeningProcessDefinition;
pub(in crate::ore_processing) use separation::ConstituentSeparationPhysics;
pub use separation::{
    ConstituentRecoveryProfile, ConstituentSeparationProcessDefinition,
    ManualConstituentSeparationProcessDefinition,
};
