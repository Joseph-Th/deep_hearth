//! Registry-derived pre-admission packages for ordinary settlement episodes.

use std::collections::BTreeMap;

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::energy::EnergyStoreDefinitionId;
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::inventory::{StockpileId, StockpileStorageProfile};
use deep_hearth::material::{CommodityKey, MaterialAssemblyProfile};
use deep_hearth::registry::Registries;

use super::environment::ROOM_TEMPERATURE;

fn add_profile(
    requirements: &mut BTreeMap<CommodityKey, Mass>,
    profile: &MaterialAssemblyProfile,
    context: &'static str,
) {
    for input in profile.inputs() {
        let total = requirements.entry(input.commodity()).or_insert(Mass::ZERO);
        *total = total
            .checked_add(input.mass())
            .unwrap_or_else(|| panic!("{context} inherited assembly material overflowed"));
    }
}

/// Seeds the exact disclosed matter for already-owned settlement machinery before player admission.
///
/// `upgrade_targets` contributes only each target definition's authored additive upgrade material;
/// it does not imply that the upgrade has already happened.
pub(super) fn seed_inherited_workshop_package(
    registries: &Registries,
    state: &mut AppState,
    equipment: &[EquipmentDefinitionId],
    energy_stores: &[EnergyStoreDefinitionId],
    upgrade_targets: &[EquipmentDefinitionId],
    context: &'static str,
) -> StockpileId {
    let mut requirements = BTreeMap::<CommodityKey, Mass>::new();
    for &definition in equipment {
        let profile = registries
            .equipment()
            .get_equipment(definition)
            .and_then(|record| record.assembly_profile())
            .unwrap_or_else(|| {
                panic!(
                    "{context} inherited equipment {} lost its assembly profile",
                    definition.value()
                )
            });
        add_profile(&mut requirements, profile, context);
    }
    for &definition in energy_stores {
        let profile = registries
            .energy()
            .get_store(definition)
            .and_then(|record| record.assembly_profile())
            .unwrap_or_else(|| {
                panic!(
                    "{context} inherited energy store {} lost its assembly profile",
                    definition.value()
                )
            });
        add_profile(&mut requirements, profile, context);
    }
    for &target in upgrade_targets {
        let additions = registries
            .equipment()
            .get_equipment(target)
            .and_then(|record| record.upgrade_profile())
            .map(|upgrade| upgrade.additions())
            .unwrap_or_else(|| {
                panic!(
                    "{context} inherited upgrade target {} lost its additive profile",
                    target.value()
                )
            });
        add_profile(&mut requirements, additions, context);
    }

    let capacity = requirements
        .values()
        .copied()
        .try_fold(Mass::ZERO, Mass::checked_add)
        .filter(|mass| !mass.is_zero())
        .unwrap_or_else(|| panic!("{context} inherited package has no authored matter"));
    let stockpile = seed_stockpile(
        state,
        capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for (commodity, mass) in requirements {
        let _ = seed_lot(
            registries,
            state,
            stockpile,
            commodity,
            mass,
            ROOM_TEMPERATURE,
        );
    }
    stockpile
}
