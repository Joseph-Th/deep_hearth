//! Repeated casting resolution and passive thermal-sink recovery for the first foundry.

use deep_hearth::content::{
    ENERGY_STONE_THERMAL_SINK, FORM_MOLTEN, MATERIAL_COPPER, PROCESS_CAST_PURE_COPPER,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::energy::{EnergyStoreId, passive_dissipation_ticks_until_empty};
use deep_hearth::equipment::EquipmentId;
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::thermal::{
    CastingLotMassConstraint, CastingLotMassRequest, CastingRequest, ResolvedCasting,
    assess_casting_lot_mass_envelope, resolve_casting_process,
};

use super::super::tick_observation::{TickEventAllowance, assert_tick_events_within};
use crate::first_foundry_fabrication::select_commodity_mass;

fn advance_passive_cooldown_tick(registries: &Registries, state: &mut AppState) {
    assert_eq!(
        state.player_work().active(),
        None,
        "first foundry passive cooldown cannot advance while player work is active"
    );
    let outcome = advance_tick(registries, state)
        .unwrap_or_else(|error| panic!("first foundry passive cooldown tick failed: {error}"));
    assert_tick_events_within(
        &outcome,
        TickEventAllowance::default(),
        "first foundry passive casting cooldown",
    );
}

pub(super) fn resolve_full_cast_after_cooldown(
    registries: &Registries,
    state: &mut AppState,
    molten: StockpileId,
    mold: EquipmentId,
    heat_sink: EnergyStoreId,
    batch: Mass,
) -> (ResolvedCasting, u64) {
    let stored = state
        .energy()
        .get_store(heat_sink)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("first foundry heat sink disappeared before casting"));
    let maximum_wait =
        passive_dissipation_ticks_until_empty(registries, ENERGY_STONE_THERMAL_SINK, stored)
            .unwrap_or_else(|error| panic!("first foundry cooldown horizon failed: {error}"))
            .map_or(0, |duration| duration.value());

    for waited in 0..=maximum_wait {
        let selections = select_commodity_mass(
            state,
            molten,
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
            batch,
            "first foundry molten copper feed",
        );
        let [selection] = selections.as_slice() else {
            panic!("first foundry full-batch cast requires one homogeneous molten lot");
        };
        let envelope = assess_casting_lot_mass_envelope(
            registries,
            state,
            CastingLotMassRequest::new(
                PROCESS_CAST_PURE_COPPER,
                molten,
                *selection,
                mold,
                heat_sink,
            ),
        )
        .unwrap_or_else(|error| panic!("first foundry casting envelope failed: {error}"));
        if envelope.maximum_mass() >= batch {
            return (
                resolve_casting_process(
                    registries,
                    state,
                    CastingRequest::new(
                        PROCESS_CAST_PURE_COPPER,
                        molten,
                        selections.as_slice(),
                        mold,
                        heat_sink,
                    ),
                )
                .unwrap_or_else(|error| {
                    panic!("first foundry full-batch casting resolution failed: {error}")
                }),
                waited,
            );
        }
        assert_eq!(
            envelope.limiting_constraint(),
            Some(CastingLotMassConstraint::ThermalSinkCapacity),
            "first foundry repeated casting may wait only for passive thermal-sink recovery"
        );
        assert!(waited < maximum_wait);
        advance_passive_cooldown_tick(registries, state);
    }
    unreachable!("bounded first-foundry cooldown loop must resolve or fail explicitly")
}
