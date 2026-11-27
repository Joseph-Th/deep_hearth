//! Player-visible completion feedback shared by manual and unattended production.

use std::fmt::Write as _;

use deep_hearth::production::ProcessCompletion;
use deep_hearth::registry::ProcessExecutionFamily;
use deep_hearth::spatial::VoxelCoord;
use deep_hearth::{AppState, Registries};

use super::super::view;

pub(super) struct ProductionFeedback {
    pub(super) message: String,
    pub(super) manual: bool,
}

/// A completed job is not a global notification. Only landings in carried or
/// exact-local custody at the settlement frame may contribute to the message.
/// Classify manual receipts by execution family: a final-tick resume can finish
/// without installing a persistent player-work hold.
pub(super) fn completion(
    registries: &Registries,
    state: &AppState,
    observer: VoxelCoord,
    receipt: &ProcessCompletion,
) -> Option<ProductionFeedback> {
    let topology = registries.process_topology(receipt.process())?;
    let (label, manual) = match topology.execution_family() {
        ProcessExecutionFamily::ManualCraft => ("Craft completed", true),
        ProcessExecutionFamily::ManualComminution | ProcessExecutionFamily::ManualSeparation => {
            ("Ore processing completed", true)
        }
        ProcessExecutionFamily::PoweredCraft => ("Powered craft completed", false),
        ProcessExecutionFamily::Comminution
        | ProcessExecutionFamily::Screening
        | ProcessExecutionFamily::ConstituentSeparation
        | ProcessExecutionFamily::Precipitation => ("Ore processing completed", false),
        ProcessExecutionFamily::SensibleHeating
        | ProcessExecutionFamily::SensibleCooling
        | ProcessExecutionFamily::Melting
        | ProcessExecutionFamily::Casting => ("Thermal batch completed", false),
    };
    let mut parcels = String::new();
    for landing in receipt.landings() {
        let Some(destination) =
            super::local_output_destination(state, observer, landing.destination())
        else {
            continue;
        };
        for parcel in landing.parcels() {
            let item = parcel.output();
            let _ = writeln!(
                parcels,
                "  Received {} g {} ({destination}).",
                view::grams(item.mass()),
                registries
                    .materials()
                    .commodity_name(item.commodity())
                    .unwrap_or("unidentified material"),
            );
        }
    }
    if parcels.is_empty() {
        return None;
    }
    let process = registries
        .production()
        .get_process(receipt.process())
        .expect("registered production topology has a process definition")
        .name();
    Some(ProductionFeedback {
        message: format!("{label}: {process}.\n{parcels}"),
        manual,
    })
}
