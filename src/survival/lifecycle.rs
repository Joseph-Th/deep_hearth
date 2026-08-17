//! Player admission, tick metabolism, and read-only survival assessment.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::registry::Registries;

use super::Vitality;
use super::state::{PlayerSurvivalRecord, player_record};

/// Qualitative energy state derived from authored physiology thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HungerState {
    Fed,
    Hungry,
    Starving,
}

/// Additional per-tick physiological cost of the player's current physical work.
///
/// Basal metabolism remains authored by `PhysiologyDefinition`; work owners contribute only the
/// incremental cost above rest so simulation can combine them without creating a second metabolism
/// path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurvivalExertion {
    energy_cost_per_tick: Energy,
    hydration_loss_per_tick: Volume,
}

impl SurvivalExertion {
    pub const REST: Self = Self {
        energy_cost_per_tick: Energy::ZERO,
        hydration_loss_per_tick: Volume::ZERO,
    };

    #[must_use]
    pub const fn new(energy_cost_per_tick: Energy, hydration_loss_per_tick: Volume) -> Self {
        Self {
            energy_cost_per_tick,
            hydration_loss_per_tick,
        }
    }

    #[must_use]
    pub const fn energy_cost_per_tick(self) -> Energy {
        self.energy_cost_per_tick
    }

    #[must_use]
    pub const fn hydration_loss_per_tick(self) -> Volume {
        self.hydration_loss_per_tick
    }
}

/// Qualitative hydration state derived from authored physiology thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HydrationState {
    Hydrated,
    Thirsty,
    Dehydrated,
}

/// Read-only survival projection suitable for UI and gameplay policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurvivalAssessment {
    metabolic_energy: Energy,
    hydration: Volume,
    vitality: Vitality,
    hunger: HungerState,
    hydration_state: HydrationState,
}

impl SurvivalAssessment {
    #[must_use]
    pub const fn metabolic_energy(self) -> Energy {
        self.metabolic_energy
    }

    #[must_use]
    pub const fn hydration(self) -> Volume {
        self.hydration
    }

    #[must_use]
    pub const fn vitality(self) -> Vitality {
        self.vitality
    }

    #[must_use]
    pub const fn hunger(self) -> HungerState {
        self.hunger
    }

    #[must_use]
    pub const fn hydration_state(self) -> HydrationState {
        self.hydration_state
    }

    #[must_use]
    pub const fn is_alive(self) -> bool {
        self.vitality.parts_per_million() > 0
    }
}

/// Failure while admitting the local player into survival simulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitializeSurvivalError {
    AlreadyInitialized,
    RevisionExhausted,
}

impl Display for InitializeSurvivalError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyInitialized => {
                formatter.write_str("player survival is already initialized")
            }
            Self::RevisionExhausted => formatter.write_str("survival revision space is exhausted"),
        }
    }
}

impl Error for InitializeSurvivalError {}

/// Starts the local player's survival state at authored full reserves.
pub fn initialize_player_survival(
    registries: &Registries,
    state: &mut AppState,
) -> Result<(), InitializeSurvivalError> {
    if state.survival().player().is_some() {
        return Err(InitializeSurvivalError::AlreadyInitialized);
    }
    let expected_revision = state.survival().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(InitializeSurvivalError::RevisionExhausted)?;
    let physiology = registries.survival().physiology();
    state.survival_state_mut().apply_player(
        expected_revision,
        next_revision,
        player_record(
            physiology.maximum_metabolic_energy(),
            physiology.maximum_hydration(),
            Vitality::MAXIMUM,
        ),
    );
    Ok(())
}

/// Returns the current survival projection when a player has been admitted.
#[must_use]
pub fn assess_survival(registries: &Registries, state: &AppState) -> Option<SurvivalAssessment> {
    state
        .survival()
        .player()
        .copied()
        .map(|player| assess_record(registries, player))
}

fn assess_record(registries: &Registries, player: PlayerSurvivalRecord) -> SurvivalAssessment {
    let physiology = registries.survival().physiology();
    let hunger = if player.metabolic_energy().is_zero() {
        HungerState::Starving
    } else if player.metabolic_energy() < physiology.hungry_below() {
        HungerState::Hungry
    } else {
        HungerState::Fed
    };
    let hydration_state = if player.hydration().is_zero() {
        HydrationState::Dehydrated
    } else if player.hydration() < physiology.thirsty_below() {
        HydrationState::Thirsty
    } else {
        HydrationState::Hydrated
    };
    SurvivalAssessment {
        metabolic_energy: player.metabolic_energy(),
        hydration: player.hydration(),
        vitality: player.vitality(),
        hunger,
        hydration_state,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SurvivalTickError {
    RevisionExhausted,
    EnergyCostOverflow,
    HydrationCostOverflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SurvivalTickPlan {
    expected_revision: u64,
    next_revision: u64,
    after: PlayerSurvivalRecord,
    assessment: SurvivalAssessment,
}

pub(crate) fn decide_survival_tick(
    registries: &Registries,
    state: &AppState,
    exertion: SurvivalExertion,
) -> Result<Option<SurvivalTickPlan>, SurvivalTickError> {
    let Some(before) = state.survival().player().copied() else {
        return Ok(None);
    };
    if before.vitality() == Vitality::ZERO {
        return Ok(None);
    }
    let physiology = registries.survival().physiology();
    let energy_cost = physiology
        .basal_energy_cost_per_tick()
        .checked_add(exertion.energy_cost_per_tick())
        .ok_or(SurvivalTickError::EnergyCostOverflow)?;
    let hydration_loss = physiology
        .hydration_loss_per_tick()
        .checked_add(exertion.hydration_loss_per_tick())
        .ok_or(SurvivalTickError::HydrationCostOverflow)?;
    let energy_after = before
        .metabolic_energy()
        .checked_sub(energy_cost)
        .unwrap_or(Energy::ZERO);
    let hydration_after = before
        .hydration()
        .checked_sub(hydration_loss)
        .unwrap_or(Volume::ZERO);
    let mut vitality_loss = 0_u32;
    if energy_after.is_zero() {
        vitality_loss =
            vitality_loss.saturating_add(physiology.starvation_vitality_loss_ppm_per_tick());
    }
    if hydration_after.is_zero() {
        vitality_loss =
            vitality_loss.saturating_add(physiology.dehydration_vitality_loss_ppm_per_tick());
    }
    let vitality_after = Vitality::from_parts_per_million_unchecked(
        before
            .vitality()
            .parts_per_million()
            .saturating_sub(vitality_loss),
    );
    let expected_revision = state.survival().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(SurvivalTickError::RevisionExhausted)?;
    let after = player_record(energy_after, hydration_after, vitality_after);
    Ok(Some(SurvivalTickPlan {
        expected_revision,
        next_revision,
        after,
        assessment: assess_record(registries, after),
    }))
}

pub(crate) fn apply_survival_tick(
    state: &mut AppState,
    plan: Option<SurvivalTickPlan>,
) -> Option<SurvivalAssessment> {
    let plan = plan?;
    state
        .survival_state_mut()
        .apply_player(plan.expected_revision, plan.next_revision, plan.after);
    Some(plan.assessment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::build_registries;
    use crate::core::time::WorldSeed;

    #[test]
    fn work_exertion_adds_exactly_to_basal_survival_cost() {
        let registries = build_registries();
        let mut resting = AppState::new(WorldSeed::new(0x5100_0002));
        initialize_player_survival(&registries, &mut resting)
            .unwrap_or_else(|error| panic!("resting survival initialization failed: {error}"));
        let mut working = resting.clone();
        let exertion = SurvivalExertion::new(
            Energy::from_nanojoules(7_000_000_000_000),
            Volume::from_microliters(900),
        );

        let resting_plan = decide_survival_tick(&registries, &resting, SurvivalExertion::REST)
            .unwrap_or_else(|error| panic!("resting survival tick failed: {error:?}"));
        let working_plan = decide_survival_tick(&registries, &working, exertion)
            .unwrap_or_else(|error| panic!("working survival tick failed: {error:?}"));
        let resting_after = apply_survival_tick(&mut resting, resting_plan)
            .unwrap_or_else(|| panic!("resting survival player disappeared"));
        let working_after = apply_survival_tick(&mut working, working_plan)
            .unwrap_or_else(|| panic!("working survival player disappeared"));

        assert_eq!(
            resting_after
                .metabolic_energy()
                .checked_sub(working_after.metabolic_energy()),
            Some(exertion.energy_cost_per_tick())
        );
        assert_eq!(
            resting_after
                .hydration()
                .checked_sub(working_after.hydration()),
            Some(exertion.hydration_loss_per_tick())
        );
    }

    #[test]
    fn survival_initialization_and_tick_are_deterministic_and_visible() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5100_0001));
        initialize_player_survival(&registries, &mut state)
            .unwrap_or_else(|error| panic!("survival initialization failed: {error}"));
        let before = assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("initialized survival record is missing"));

        let plan = decide_survival_tick(&registries, &state, SurvivalExertion::REST)
            .unwrap_or_else(|error| panic!("survival tick failed: {error:?}"))
            .unwrap_or_else(|| panic!("survival tick did not produce a plan"));
        let after = apply_survival_tick(&mut state, Some(plan))
            .unwrap_or_else(|| panic!("survival tick lost the player"));

        assert!(after.metabolic_energy() < before.metabolic_energy());
        assert!(after.hydration() < before.hydration());
        assert_eq!(after.vitality(), Vitality::MAXIMUM);
    }
}
