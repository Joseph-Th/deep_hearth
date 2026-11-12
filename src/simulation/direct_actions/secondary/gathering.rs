//! Physical local gathering and drinking context, typed rejection, and input-frame work admission.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::player::{PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::surface::{
    PlayerSurfaceGatheringOption, SurfaceGatheringError, SurfaceResourceId,
    assess_player_surface_gathering_option_admission, assess_player_surface_gathering_reservation,
    player_surface_gathering_options, validate_start_player_surface_gathering_option_control_rate,
};
use crate::survival::{
    assess_drink_store_to_full_admission, available_local_drink_sources,
    validate_drink_store_to_full_control_rate,
};

use super::SecondaryActionOutcome;

enum DirectSurfaceGatheringResolution {
    None,
    One(PlayerSurfaceGatheringOption),
    AmbiguousResources { count: usize },
    AmbiguousMethods { count: usize },
}

fn resolve_direct_surface_gathering(
    registries: &Registries,
    state: &AppState,
    allow_unavailable_fallback: bool,
) -> DirectSurfaceGatheringResolution {
    let options = player_surface_gathering_options(registries, state).unwrap_or_else(|error| {
        panic!("runtime invariant broken: direct gathering option projection failed: {error}")
    });
    // Inadmissible sources must not compete with a viable local gather.
    // The owner checks temperature, storage, revisions and work resources.
    let actionable = options
        .iter()
        .copied()
        .filter(|option| {
            assess_player_surface_gathering_option_admission(
                registries,
                state,
                *option,
                direct_gathering_request_mass(state, *option),
            )
            .is_ok()
        })
        .collect::<Vec<_>>();
    if actionable.is_empty() && allow_unavailable_fallback {
        // With neither a feasible drink nor a feasible gather, preserve the
        // observed source's typed rejection rather than reporting no context.
        classify_direct_surface_gathering_options(&options)
    } else {
        classify_direct_surface_gathering_options(&actionable)
    }
}

fn classify_direct_surface_gathering_options(
    options: &[PlayerSurfaceGatheringOption],
) -> DirectSurfaceGatheringResolution {
    // Resolve physical sources before their possible gathering methods. Multiple
    // methods for one source must not mask a different local source, and method
    // multiplicity must not inflate the number of physical resources reported.
    let mut resources = options.chunk_by(|left, right| left.resource() == right.resource());
    let Some(methods) = resources.next() else {
        return DirectSurfaceGatheringResolution::None;
    };
    let additional_resources = resources.count();
    if additional_resources != 0 {
        DirectSurfaceGatheringResolution::AmbiguousResources {
            count: additional_resources + 1,
        }
    } else if methods.len() > 1 {
        DirectSurfaceGatheringResolution::AmbiguousMethods {
            count: methods.len(),
        }
    } else {
        DirectSurfaceGatheringResolution::One(methods[0])
    }
}

pub(in crate::simulation::direct_actions) fn attempt_empty_hand_secondary_action(
    registries: &Registries,
    state: &mut AppState,
) -> SecondaryActionOutcome {
    let observed_drink_sources =
        available_local_drink_sources(registries, state).collect::<Vec<_>>();
    let physiology = registries.survival().physiology();
    let needs_hydration = state.survival().player().is_some_and(|player| {
        player.hydration() < physiology.maximum_hydration()
            || state.logistics().player().is_some_and(|logistics| {
                !logistics
                    .locomotion_exertion()
                    .pending()
                    .hydration_loss_per_tick()
                    .is_zero()
            })
    });
    // A sub-serving final remnant is actionable when the drinking owner can
    // consume the entire finite source. Never reimplement the old serving
    // threshold here, since that would hide a real choice beside gathering.
    // Owner admission still filters unavailable, unsafe, and unnecessary drinks.
    let actionable_drink_sources = observed_drink_sources
        .iter()
        .copied()
        .filter(|source| {
            needs_hydration
                && assess_drink_store_to_full_admission(registries, state, source.store())
        })
        .collect::<Vec<_>>();
    let gathering =
        resolve_direct_surface_gathering(registries, state, actionable_drink_sources.is_empty());
    let surface_resources = match gathering {
        DirectSurfaceGatheringResolution::AmbiguousResources { count } => count,
        DirectSurfaceGatheringResolution::One(_)
        | DirectSurfaceGatheringResolution::AmbiguousMethods { .. } => 1,
        DirectSurfaceGatheringResolution::None => 0,
    };
    if surface_resources != 0 && !actionable_drink_sources.is_empty() {
        return SecondaryActionOutcome::AmbiguousEmptyHandContext {
            drink_sources: actionable_drink_sources.len(),
            surface_resources,
        };
    }
    if let DirectSurfaceGatheringResolution::AmbiguousMethods { count } = gathering {
        return SecondaryActionOutcome::AmbiguousSurfaceGatheringMethods { count };
    }
    if surface_resources > 1 {
        return SecondaryActionOutcome::AmbiguousLocalSurfaceResources {
            count: surface_resources,
        };
    }
    if let DirectSurfaceGatheringResolution::One(target) = gathering {
        return attempt_direct_surface_gathering(registries, state, target);
    }
    // A fully hydrated player has no drink to select. Multiple water stores
    // cannot create a physical-choice ambiguity between equivalent no-ops;
    // retain gathering priority above, and leave all water custody unchanged.
    if !needs_hydration && state.survival().player().is_some() && !observed_drink_sources.is_empty()
    {
        return SecondaryActionOutcome::AlreadySatisfied;
    }
    // With no gathering choice, keep the original source-specific rejection
    // instead of pretending observed water vanished just because the current
    // player cannot consume it.
    let drink_sources = if actionable_drink_sources.is_empty() {
        &observed_drink_sources
    } else {
        &actionable_drink_sources
    };
    match drink_sources.as_slice() {
        [] => SecondaryActionOutcome::NoActiveSelection,
        [source] => attempt_direct_drinking(registries, state, source.store()),
        sources => SecondaryActionOutcome::AmbiguousLocalDrinkSources {
            count: sources.len(),
        },
    }
}

fn direct_gathering_request_mass(state: &AppState, option: PlayerSurfaceGatheringOption) -> Mass {
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: local surface resource requires player")
        });
    let available_capacity = state
        .inventory()
        .get_stockpile(carried)
        .map(|stockpile| stockpile.available_capacity())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: admitted player lost carried stockpile")
        });
    let desired = option.source_mass().min(option.direct_batch_mass());
    let gathered_mass = desired.min(available_capacity);
    if gathered_mass.is_zero() {
        desired
    } else {
        gathered_mass
    }
}

fn attempt_direct_surface_gathering(
    registries: &Registries,
    state: &mut AppState,
    option: PlayerSurfaceGatheringOption,
) -> SecondaryActionOutcome {
    if let Some(blocker) = option.hand_handling_blocker() {
        return SecondaryActionOutcome::SurfaceGatheringRejected(blocker);
    }
    let request_mass = direct_gathering_request_mass(state, option);
    if let Err(error) = assess_player_surface_gathering_reservation(state, request_mass) {
        return SecondaryActionOutcome::SurfaceGatheringRejected(error);
    }
    match validate_start_player_surface_gathering_option_control_rate(
        registries,
        state,
        option,
        request_mass,
    ) {
        Ok(validated) => {
            validated.commit(state).unwrap_or_else(|error| {
                panic!(
                    "runtime invariant broken: synchronous surface gathering became stale: {error}"
                )
            });
            let work = match state.player_work().active() {
                Some(crate::labor::PlayerWork::SurfaceGathering { work }) => work,
                other => {
                    panic!("runtime invariant broken: direct surface gathering started {other:?}")
                }
            };
            SecondaryActionOutcome::SurfaceGatheringStarted(work)
        }
        Err(error) => SecondaryActionOutcome::SurfaceGatheringRejected(error),
    }
}

/// A catalog gesture names the exact observed natural source and amount. The
/// option is re-resolved on execution, so a stolen/depleted or remote source
/// cannot be silently replaced by a different one.
pub(in crate::simulation::direct_actions) fn attempt_explicit_surface_gathering(
    registries: &Registries,
    state: &mut AppState,
    resource: SurfaceResourceId,
    mass: Mass,
) -> SecondaryActionOutcome {
    match validate_player_action(state) {
        Ok(_) => {}
        Err(PlayerActionValidationError::PlayerNotAdmitted) => {
            return SecondaryActionOutcome::PlayerNotAdmitted;
        }
        Err(PlayerActionValidationError::PlayerDead) => {
            return SecondaryActionOutcome::PlayerDead;
        }
    }
    if mass.is_zero() {
        return SecondaryActionOutcome::SurfaceGatheringRejected(SurfaceGatheringError::ZeroMass);
    }
    let options = player_surface_gathering_options(registries, state).unwrap_or_else(|error| {
        panic!("runtime invariant broken: explicit gathering catalog failed: {error}")
    });
    let mut matching = options
        .iter()
        .copied()
        .filter(|option| option.resource() == resource);
    let Some(first_option) = matching.next() else {
        return SecondaryActionOutcome::SurfaceGatheringRejected(
            SurfaceGatheringError::ResourceUnavailableAtPlayer { resource },
        );
    };
    // Reject impossible physical requests on the input frame, using the
    // largest authored method rather than whichever method sorts first.
    let widest_option = matching.fold(first_option, |widest, option| {
        if option.maximum_batch_mass() > widest.maximum_batch_mass() {
            option
        } else {
            widest
        }
    });
    let maximum_batch_mass = widest_option.maximum_batch_mass();
    if mass > maximum_batch_mass {
        return SecondaryActionOutcome::SurfaceGatheringRejected(
            SurfaceGatheringError::BatchTooLarge {
                requested: mass,
                maximum: maximum_batch_mass,
            },
        );
    }
    if mass > first_option.source_mass() {
        return SecondaryActionOutcome::SurfaceGatheringRejected(
            SurfaceGatheringError::InsufficientResource {
                requested: mass,
                remaining: first_option.source_mass(),
            },
        );
    }
    if let Some(blocker) = first_option.hand_handling_blocker() {
        return SecondaryActionOutcome::SurfaceGatheringRejected(blocker);
    }
    if let Err(error) = assess_player_surface_gathering_reservation(state, mass) {
        return SecondaryActionOutcome::SurfaceGatheringRejected(error);
    }
    // Only feasible methods are choices. The ordinary contextual gathering
    // gesture already uses owner admission to avoid false ambiguity; explicit
    // resource-and-mass gestures must obey the same rule.
    let mut admissible = options.into_iter().filter(|option| {
        option.resource() == resource
            && mass <= option.maximum_batch_mass()
            && assess_player_surface_gathering_option_admission(registries, state, *option, mass)
                .is_ok()
    });
    let sole_admissible = admissible.next();
    let additional = admissible.count();
    if additional > 0 {
        return SecondaryActionOutcome::AmbiguousSurfaceGatheringMethods {
            count: additional + 1,
        };
    }
    // All methods rejected: preserve a canonical typed owner error from a
    // method capable of handling the amount rather than claim ambiguity.
    let option = sole_admissible.unwrap_or(widest_option);
    match validate_start_player_surface_gathering_option_control_rate(
        registries, state, option, mass,
    ) {
        Ok(validated) => {
            validated.commit(state).unwrap_or_else(|error| {
                panic!("runtime invariant broken: explicit surface gathering became stale: {error}")
            });
            match state.player_work().active() {
                Some(crate::labor::PlayerWork::SurfaceGathering { work }) => {
                    SecondaryActionOutcome::SurfaceGatheringStarted(work)
                }
                other => panic!("explicit surface gathering started unexpected work: {other:?}"),
            }
        }
        Err(error) => SecondaryActionOutcome::SurfaceGatheringRejected(error),
    }
}

fn attempt_direct_drinking(
    registries: &Registries,
    state: &mut AppState,
    store: crate::fluid::FluidStoreId,
) -> SecondaryActionOutcome {
    match validate_drink_store_to_full_control_rate(registries, state, store) {
        Ok(Some(validated)) => {
            let outcome = validated.commit(state).unwrap_or_else(|error| {
                panic!(
                    "runtime invariant broken: synchronous local-drink use became stale: {error}"
                )
            });
            SecondaryActionOutcome::DrinkingStarted(outcome)
        }
        Ok(None) => SecondaryActionOutcome::AlreadySatisfied,
        Err(error) => SecondaryActionOutcome::DrinkingRejected(error),
    }
}

#[cfg(test)]
#[path = "gathering_tests.rs"]
mod tests;
