//! Settlement workload projection over repeated manual-power charges.

use deep_hearth::core::quantity::Energy;
use deep_hearth::energy::EnergyStoreDefinitionId;
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::labor::{ManualPowerMethodId, project_manual_power};
use deep_hearth::maintenance::Condition;
use deep_hearth::registry::Registries;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ManualPowerSequenceProjection {
    pub(super) attention_ticks: u64,
    pub(super) first_charge_ticks: u64,
    pub(super) condition_after: Condition,
}

#[derive(Clone, Copy)]
pub(super) struct ManualPowerSequenceRequest {
    pub(super) method: ManualPowerMethodId,
    pub(super) equipment: EquipmentDefinitionId,
    pub(super) starting_condition: Condition,
    pub(super) store: EnergyStoreDefinitionId,
    pub(super) energy_per_charge: Energy,
    pub(super) charges: u64,
}

/// Projects repeated equal manual-power charges while carrying canonical equipment wear forward.
///
/// The harness owns only the declared workload horizon. Per-charge power, duration, survival cost,
/// and condition loss remain production-owned through `project_manual_power`.
pub(super) fn project_manual_power_sequence(
    registries: &Registries,
    request: ManualPowerSequenceRequest,
    context: &'static str,
) -> ManualPowerSequenceProjection {
    assert!(
        request.charges > 0,
        "gameplay harness {context} manual-power sequence requires at least one charge"
    );
    let mut condition = request.starting_condition;
    let mut attention_ticks = 0_u64;
    let mut first_charge_ticks = None;
    for _ in 0..request.charges {
        let projection = project_manual_power(
            registries,
            request.method,
            request.equipment,
            condition,
            request.store,
            request.energy_per_charge,
        )
        .unwrap_or_else(|error| {
            panic!("gameplay harness {context} manual-power projection failed: {error}")
        });
        let ticks = projection.duration().value();
        first_charge_ticks.get_or_insert(ticks);
        attention_ticks = attention_ticks
            .checked_add(ticks)
            .unwrap_or_else(|| panic!("gameplay harness {context} attention overflowed"));
        condition = projection.condition_after();
    }
    ManualPowerSequenceProjection {
        attention_ticks,
        first_charge_ticks: first_charge_ticks
            .unwrap_or_else(|| unreachable!("positive charge count projected one charge")),
        condition_after: condition,
    }
}
