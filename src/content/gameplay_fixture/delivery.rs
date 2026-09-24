//! Opaque preauthorized controlled-delivery event for capability evaluation.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    StockpileId, ValidatedMaterialRelocation, validate_consumption_selection,
    validate_material_relocation_from_selection,
};
use crate::material::{CommodityKey, MaterialInputSpec};
use crate::registry::Registries;

/// Harness-only logistics authorization for one controlled material-delivery event.
///
/// Create this during scenario setup, before the acting policy starts. Authorization first proves
/// the exact relocation is physically valid against the setup state, but does not move matter or
/// reveal event timing to the actor. Event-time commit revalidates the same relocation against live
/// state. This is a controlled audit authorization because world logistics is outside current
/// production scope and ordinary runtime cannot create pathless relocations.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ControlledMaterialDelivery {
    source: StockpileId,
    destination: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
}

fn validate_controlled_material_delivery(
    registries: &Registries,
    state: &AppState,
    delivery: &ControlledMaterialDelivery,
) -> ValidatedMaterialRelocation {
    assert!(
        !delivery.mass.is_zero(),
        "controlled material delivery mass must be nonzero"
    );
    let selection = validate_consumption_selection(
        state.inventory(),
        delivery.source,
        &[MaterialInputSpec::new(delivery.commodity, delivery.mass)],
    )
    .unwrap_or_else(|error| {
        panic!("gameplay controlled delivery material selection failed: {error:?}")
    });
    validate_material_relocation_from_selection(registries, state, delivery.destination, selection)
        .unwrap_or_else(|error| panic!("gameplay controlled delivery relocation failed: {error}"))
}

pub fn authorize_controlled_material_delivery(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
    destination: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
) -> ControlledMaterialDelivery {
    super::assert_pre_admission(state, "controlled-delivery authorization");
    let delivery = ControlledMaterialDelivery {
        source,
        destination,
        commodity,
        mass,
    };
    let _validated_setup_relocation =
        validate_controlled_material_delivery(registries, state, &delivery);
    delivery
}

/// Applies one previously authorized controlled delivery through canonical inventory validation.
///
/// Keeping the inventory relocation proof private prevents the gameplay harness from manufacturing
/// arbitrary pathless logistics after actor admission. The scenario controller can only retain and
/// later consume this opaque authorization created during controlled setup.
pub fn commit_controlled_material_delivery(
    registries: &Registries,
    state: &mut AppState,
    delivery: ControlledMaterialDelivery,
) {
    assert!(
        state.survival().player().is_some(),
        "gameplay controlled delivery may only commit after actor admission"
    );
    validate_controlled_material_delivery(registries, state, &delivery)
        .commit(state)
        .unwrap_or_else(|error| panic!("gameplay controlled delivery commit failed: {error}"));
}
