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

/// Projects a concrete sequence of possibly different charge requests while carrying provider
/// wear forward. This is the actor-facing primitive for workloads that pack productive batches
/// into finite storage instead of pretending every productive batch requires a separate charge.
pub(super) fn project_manual_power_workload(
    registries: &Registries,
    method: ManualPowerMethodId,
    equipment: EquipmentDefinitionId,
    starting_condition: Condition,
    store: EnergyStoreDefinitionId,
    charges: impl IntoIterator<Item = Energy>,
    context: &'static str,
) -> ManualPowerSequenceProjection {
    let mut condition = starting_condition;
    let mut attention_ticks = 0_u64;
    let mut first_charge_ticks = None;
    let mut charge_count = 0_u64;
    for energy in charges {
        assert!(
            !energy.is_zero(),
            "gameplay harness {context} manual-power workload contains a zero charge"
        );
        let projection =
            project_manual_power(registries, method, equipment, condition, store, energy)
                .unwrap_or_else(|error| {
                    panic!("gameplay harness {context} manual-power projection failed: {error}")
                });
        let ticks = projection.duration().value();
        first_charge_ticks.get_or_insert(ticks);
        attention_ticks = attention_ticks
            .checked_add(ticks)
            .unwrap_or_else(|| panic!("gameplay harness {context} attention overflowed"));
        condition = projection.condition_after();
        charge_count = charge_count
            .checked_add(1)
            .unwrap_or_else(|| panic!("gameplay harness {context} charge count overflowed"));
    }
    assert!(
        charge_count > 0,
        "gameplay harness {context} manual-power workload requires at least one charge"
    );
    ManualPowerSequenceProjection {
        attention_ticks,
        first_charge_ticks: first_charge_ticks
            .unwrap_or_else(|| unreachable!("positive charge count projected one charge")),
        condition_after: condition,
    }
}

#[derive(Clone, Copy)]
#[allow(
    dead_code,
    reason = "shared gameplay module is included by targets that use packed variable charges; other gameplay targets still use equal-charge projections"
)]
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
#[allow(
    dead_code,
    reason = "shared gameplay module is included by targets that use packed variable charges; other gameplay targets still use equal-charge projections"
)]
pub(super) fn project_manual_power_sequence(
    registries: &Registries,
    request: ManualPowerSequenceRequest,
    context: &'static str,
) -> ManualPowerSequenceProjection {
    assert!(
        request.charges > 0,
        "gameplay harness {context} manual-power sequence requires at least one charge"
    );
    project_manual_power_workload(
        registries,
        request.method,
        request.equipment,
        request.starting_condition,
        request.store,
        std::iter::repeat_n(request.energy_per_charge, request.charges as usize),
        context,
    )
}
