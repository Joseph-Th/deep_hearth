//! Actor-visible carried food cohort selection for contextual secondary use.

use crate::core::state::AppState;
use crate::inventory::project_material_stacks;
use crate::material::CommodityKey;
use crate::registry::Registries;
use crate::survival::validate_eat_stack_to_full_control_rate;

use super::SecondaryActionOutcome;

/// Resolves a single visible carried stack without selecting an arbitrary physical lot.
pub(in crate::simulation::direct_actions) fn resolve_unique_carried_stack(
    registries: &Registries,
    state: &AppState,
    commodity: CommodityKey,
) -> Result<crate::inventory::MaterialStackProjection, SecondaryActionOutcome> {
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: active material selection requires player")
        });
    let mut matches = project_material_stacks(registries, state, carried)
        .unwrap_or_else(|error| {
            panic!("runtime invariant broken: carried stack projection failed: {error}")
        })
        .into_iter()
        .filter(|stack| stack.commodity() == commodity);
    let Some(first) = matches.next() else {
        return Err(SecondaryActionOutcome::MaterialUnavailable);
    };
    let Some(second) = matches.next() else {
        // Preserve the food owner's specific rejection or already-satisfied
        // outcome when there is only one physical cohort to use.
        return Ok(first);
    };
    if registries.survival().get_food(commodity).is_none() {
        // Timber placement also uses this resolver. It has no eating admission
        // and must continue requiring an explicit choice of physical cohort.
        return Err(SecondaryActionOutcome::AmbiguousMaterialStacks {
            count: 2 + matches.count(),
        });
    }
    let mut observed = 0;
    let mut available_meals = 0;
    let mut sole_meal = None;
    let mut satisfied = None;
    for stack in [first, second].into_iter().chain(matches) {
        observed += 1;
        match validate_eat_stack_to_full_control_rate(registries, state, &stack) {
            Ok(Some(_)) => {
                available_meals += 1;
                sole_meal = Some(stack);
            }
            Ok(None) => satisfied = Some(stack),
            Err(_) => {}
        }
    }
    if available_meals == 1 {
        // Current meal validation, not commodity equality or private lot ID,
        // chooses the one usable physical cohort. Multiple viable cohorts
        // still demand an explicit stack choice.
        return Ok(sole_meal.expect("one admitted meal was retained"));
    }
    if available_meals == 0
        && let Some(stack) = satisfied
    {
        // Ok(None) means the reserve is already full, not that a meal can
        // compete for attention. Preserve the owner's AlreadySatisfied result
        // instead of inventing an ambiguous choice between no-op stacks.
        return Ok(stack);
    }
    Err(SecondaryActionOutcome::AmbiguousMaterialStacks {
        count: if available_meals > 1 {
            available_meals
        } else {
            observed
        },
    })
}
