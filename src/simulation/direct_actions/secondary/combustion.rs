//! Selection and execution of a physical local combustion or fire assembly target.

use crate::core::state::AppState;
use crate::energy::{validate_assemble_energy_store, validate_combust_selected_fuel};
use crate::material::CommodityKey;
use crate::registry::Registries;

use super::SecondaryActionOutcome;

/// One read-only physical combustion choice, reused by the input-frame
/// deferral gate and the canonical slow-boundary action. No duplicate policy
/// can make the preview and eventual transaction select different stores.
enum SelectedCombustionTarget {
    Existing(crate::energy::EnergyStoreId),
    AmbiguousStores(usize),
    Assembly(crate::energy::EnergyStoreDefinitionId),
    AmbiguousDefinitions(usize),
}

fn resolve_selected_combustion_target(
    registries: &Registries,
    state: &AppState,
    commodity: CommodityKey,
) -> Option<SelectedCombustionTarget> {
    let mut stores = state
        .available_local_energy_stores()
        .filter(|store| {
            registries
                .energy()
                .get_store(store.definition())
                .is_some_and(|definition| definition.combustion_profile_for(commodity).is_some())
        })
        .map(|store| store.id());
    if let Some(first) = stores.next() {
        let Some(second) = stores.next() else {
            // Keep the owner's typed rejection when the only observed fire is
            // full, occupied, or otherwise unable to accept the chosen fuel.
            return Some(SelectedCombustionTarget::Existing(first));
        };
        let source = state
            .logistics()
            .player()
            .expect("observed local combustion stores require an admitted player")
            .carried_stockpile();
        let mut observed = 0;
        let mut admissible = 0;
        let mut sole_admissible = None;
        for store in [first, second].into_iter().chain(stores) {
            observed += 1;
            if validate_combust_selected_fuel(registries, state, store, source, commodity).is_ok() {
                admissible += 1;
                sole_admissible = Some(store);
            }
        }
        // A full or occupied fire must not hide the one usable physical
        // destination. Two admissible stores remain a genuine player choice;
        // two blocked stores still report the observed ambiguity.
        return Some(if admissible == 1 {
            SelectedCombustionTarget::Existing(sole_admissible.unwrap())
        } else {
            SelectedCombustionTarget::AmbiguousStores(if admissible > 1 {
                admissible
            } else {
                observed
            })
        });
    }
    let mut definitions = registries
        .energy()
        .definitions()
        .filter(|definition| definition.combustion_profile().is_some())
        .filter(|definition| {
            definition.assembly_profile().is_some_and(|profile| {
                let inputs = profile.inputs();
                inputs.len() == 1 && inputs[0].commodity() == commodity
            })
        })
        .map(|definition| definition.id());
    let definition = definitions.next()?;
    let additional = definitions.count();
    Some(if additional == 0 {
        SelectedCombustionTarget::Assembly(definition)
    } else {
        SelectedCombustionTarget::AmbiguousDefinitions(additional + 1)
    })
}

pub(super) fn attempt_selected_combustion(
    registries: &Registries,
    state: &mut AppState,
    commodity: CommodityKey,
) -> Option<SecondaryActionOutcome> {
    match resolve_selected_combustion_target(registries, state, commodity)? {
        SelectedCombustionTarget::Existing(store) => {
            let source = state
                .logistics()
                .player()
                .map(|player| player.carried_stockpile())
                .unwrap_or_else(|| {
                    panic!("runtime invariant broken: local combustion store requires player")
                });
            Some(
                match validate_combust_selected_fuel(registries, state, store, source, commodity) {
                    Ok(validated) => {
                        let released = validated.commit(state).unwrap_or_else(|error| {
                        panic!("runtime invariant broken: synchronous combustion became stale: {error}")
                    });
                        SecondaryActionOutcome::CombustionFuelAdded { store, released }
                    }
                    Err(error) => SecondaryActionOutcome::CombustionFuelRejected { store, error },
                },
            )
        }
        SelectedCombustionTarget::AmbiguousStores(count) => {
            Some(SecondaryActionOutcome::AmbiguousLocalCombustionStores { count })
        }
        SelectedCombustionTarget::Assembly(definition) => {
            let source = state
                .logistics()
                .player()
                .map(|player| player.carried_stockpile())
                .unwrap_or_else(|| {
                    panic!("runtime invariant broken: selected combustion assembly requires player")
                });
            Some(
                match validate_assemble_energy_store(registries, state, definition, source) {
                    Ok(validated) => {
                        let store = validated.commit(state).unwrap_or_else(|error| {
                        panic!("runtime invariant broken: synchronous combustion-store assembly became stale: {error}")
                    });
                        SecondaryActionOutcome::EnergyStoreAssembled { definition, store }
                    }
                    Err(error) => {
                        SecondaryActionOutcome::EnergyStoreAssemblyRejected { definition, error }
                    }
                },
            )
        }
        SelectedCombustionTarget::AmbiguousDefinitions(count) => {
            Some(SecondaryActionOutcome::AmbiguousCombustionDefinitions { count })
        }
    }
}
/// Whether an actual local target exists without exposing private selection identities.
pub(super) fn has_selected_combustion_target(
    registries: &Registries,
    state: &AppState,
    commodity: CommodityKey,
) -> bool {
    resolve_selected_combustion_target(registries, state, commodity).is_some()
}
