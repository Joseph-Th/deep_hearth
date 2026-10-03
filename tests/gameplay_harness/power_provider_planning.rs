//! Pre-action workload planning for primitive and settlement human power.

use deep_hearth::content::{
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_TIMBER_TREADLE_DRIVE, EQUIPMENT_TIMBER_TREADLE_DYNAMO,
    EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE, MANUAL_POWER_FOOT_TREADLE, MANUAL_POWER_HAND_CRANK,
    MANUAL_POWER_WALKING_WHEEL, MATERIAL_COPPER, PROCESS_CRUSH_ORE,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::AppState;
use deep_hearth::energy::EnergyStoreDefinitionId;
use deep_hearth::equipment::{EquipmentDefinitionId, EquipmentId};
use deep_hearth::inventory::StockpileId;
use deep_hearth::labor::ManualPowerProjection;
use deep_hearth::maintenance::Condition;
use deep_hearth::ore_processing::{
    PoweredOreOrderMaintenancePolicy, PoweredOreOrderRequest, project_powered_ore_order,
};
use deep_hearth::registry::Registries;

use super::super::capital_investment_policy::{CapitalInvestmentPolicy, clears_attention_return};
use super::super::manual_assembly_planning::project_manual_assembly_package;

#[path = "power_provider_planning/lifecycle.rs"]
mod lifecycle;
use lifecycle::{ManualPowerRoute, charge_events_for_declared_work};

#[cfg(not(test))]
const MAX_PRIMITIVE_CROSSOVER_CHARGES: u64 = 512;
const MAX_PRIMITIVE_PROJECT_BATCHES: u64 = 1_024;
const MAX_SETTLEMENT_CROSSOVER_CHARGES: u64 = 160;

pub(super) const fn primitive_project_batch_limit() -> u64 {
    MAX_PRIMITIVE_PROJECT_BATCHES
}

pub(super) const fn settlement_crossover_search_limit() -> u64 {
    MAX_SETTLEMENT_CROSSOVER_CHARGES
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ShapedBuild {
    pub(super) attention_ticks: u64,
    pub(super) input_mass_mg: u64,
    pub(super) embodied_mass_mg: u64,
    pub(super) metabolic_nj: u128,
    pub(super) hydration_ul: u64,
}

impl ShapedBuild {
    pub(super) fn checked_add(self, other: Self, context: &'static str) -> Self {
        Self {
            attention_ticks: self
                .attention_ticks
                .checked_add(other.attention_ticks)
                .unwrap_or_else(|| panic!("{context} attention overflowed")),
            input_mass_mg: self
                .input_mass_mg
                .checked_add(other.input_mass_mg)
                .unwrap_or_else(|| panic!("{context} input mass overflowed")),
            embodied_mass_mg: self
                .embodied_mass_mg
                .checked_add(other.embodied_mass_mg)
                .unwrap_or_else(|| panic!("{context} embodied mass overflowed")),
            metabolic_nj: self
                .metabolic_nj
                .checked_add(other.metabolic_nj)
                .unwrap_or_else(|| panic!("{context} metabolism overflowed")),
            hydration_ul: self
                .hydration_ul
                .checked_add(other.hydration_ul)
                .unwrap_or_else(|| panic!("{context} hydration overflowed")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PrimitivePowerChoice {
    Crank,
    Treadle,
    WalkingWheel,
}

impl PrimitivePowerChoice {
    pub(super) const ALL: [Self; 3] = [Self::Crank, Self::Treadle, Self::WalkingWheel];

    pub(super) const fn equipment(self) -> EquipmentDefinitionId {
        match self {
            Self::Crank => EQUIPMENT_STONE_HAND_CRANK,
            Self::Treadle => EQUIPMENT_TIMBER_TREADLE_DRIVE,
            Self::WalkingWheel => EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
        }
    }

    pub(super) const fn method(self) -> deep_hearth::labor::ManualPowerMethodId {
        match self {
            Self::Crank => MANUAL_POWER_HAND_CRANK,
            Self::Treadle => MANUAL_POWER_FOOT_TREADLE,
            Self::WalkingWheel => MANUAL_POWER_WALKING_WHEEL,
        }
    }

    #[cfg(not(test))]
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Crank => "crank",
            Self::Treadle => "treadle",
            Self::WalkingWheel => "walking-wheel",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettlementPowerChoice {
    StoneCrank,
    CopperCrank,
    Treadle,
    TreadleDynamo,
    WalkingWheel,
}

impl SettlementPowerChoice {
    pub(super) const ALL: [Self; 5] = [
        Self::StoneCrank,
        Self::CopperCrank,
        Self::Treadle,
        Self::TreadleDynamo,
        Self::WalkingWheel,
    ];

    pub(super) const fn equipment(self) -> EquipmentDefinitionId {
        match self {
            Self::StoneCrank => EQUIPMENT_STONE_HAND_CRANK,
            Self::CopperCrank => EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
            Self::Treadle => EQUIPMENT_TIMBER_TREADLE_DRIVE,
            Self::TreadleDynamo => EQUIPMENT_TIMBER_TREADLE_DYNAMO,
            Self::WalkingWheel => EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
        }
    }

    pub(super) const fn method(self) -> deep_hearth::labor::ManualPowerMethodId {
        match self {
            Self::StoneCrank | Self::CopperCrank => MANUAL_POWER_HAND_CRANK,
            Self::Treadle | Self::TreadleDynamo => MANUAL_POWER_FOOT_TREADLE,
            Self::WalkingWheel => MANUAL_POWER_WALKING_WHEEL,
        }
    }

    pub(super) fn uses_copper(self, registries: &Registries) -> bool {
        registries
            .equipment()
            .get_equipment(self.equipment())
            .and_then(|definition| definition.assembly_profile())
            .is_some_and(|assembly| {
                assembly
                    .inputs()
                    .iter()
                    .any(|input| input.commodity().material() == MATERIAL_COPPER)
            })
    }

    #[cfg(not(test))]
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::StoneCrank => "stone-crank",
            Self::CopperCrank => "copper-crank",
            Self::Treadle => "treadle",
            Self::TreadleDynamo => "treadle-dynamo",
            Self::WalkingWheel => "walking-wheel",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct PrimitivePowerPlan {
    pub(super) choice: PrimitivePowerChoice,
    pub(super) minimum_return_ppm: u64,
    pub(super) store_definition: EnergyStoreDefinitionId,
    pub(super) capacity_nj: u128,
    pub(super) declared_mass: Mass,
    pub(super) declared_work_nj: u128,
    pub(super) charge_events: u64,
    #[cfg_attr(
        test,
        allow(dead_code, reason = "exploratory power-provider report telemetry")
    )]
    pub(super) consumer_projected_batches: u64,
    pub(super) consumer_projected_services: u64,
    pub(super) crank_build: ShapedBuild,
    pub(super) treadle_build: ShapedBuild,
    pub(super) crank_charge: ManualPowerProjection,
    pub(super) treadle_charge: ManualPowerProjection,
    #[cfg_attr(
        test,
        allow(dead_code, reason = "exploratory power-provider report telemetry")
    )]
    pub(super) crank_lifecycle_attention: u64,
    #[cfg_attr(
        test,
        allow(dead_code, reason = "exploratory power-provider report telemetry")
    )]
    pub(super) treadle_lifecycle_attention: u64,
    #[cfg_attr(
        test,
        allow(dead_code, reason = "exploratory power-provider report telemetry")
    )]
    pub(super) crank_lifecycle_metabolic_nj: u128,
    #[cfg_attr(
        test,
        allow(dead_code, reason = "exploratory power-provider report telemetry")
    )]
    pub(super) treadle_lifecycle_metabolic_nj: u128,
    #[cfg_attr(
        test,
        allow(dead_code, reason = "exploratory power-provider report telemetry")
    )]
    pub(super) crank_lifecycle_hydration_ul: u64,
    #[cfg_attr(
        test,
        allow(dead_code, reason = "exploratory power-provider report telemetry")
    )]
    pub(super) treadle_lifecycle_hydration_ul: u64,
    #[cfg_attr(
        test,
        allow(dead_code, reason = "exploratory power-provider report telemetry")
    )]
    pub(super) crank_lifecycle_condition: Condition,
    #[cfg_attr(
        test,
        allow(dead_code, reason = "exploratory power-provider report telemetry")
    )]
    pub(super) treadle_lifecycle_condition: Condition,
    #[cfg_attr(
        test,
        allow(dead_code, reason = "exploratory power-provider report telemetry")
    )]
    pub(super) minimum_attention_return_ticks: u64,
}

#[derive(Clone, Copy)]
struct PrimitiveCandidateRoute {
    choice: PrimitivePowerChoice,
    build: ShapedBuild,
    route: ManualPowerRoute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettlementCopperPolicy {
    SpendAvailable,
    PreserveForOtherUses,
}

impl SettlementCopperPolicy {
    #[cfg(not(test))]
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::SpendAvailable => "spend-available",
            Self::PreserveForOtherUses => "preserve-for-other-uses",
        }
    }
}

#[derive(Clone, Copy)]
struct PrimitiveCandidateProjection {
    choice: PrimitivePowerChoice,
    build: ShapedBuild,
    lifecycle_attention: u64,
    lifecycle_metabolic_nj: u128,
    lifecycle_hydration_ul: u64,
    lifecycle_condition: Condition,
}

impl PrimitiveCandidateProjection {
    const fn setup_key(self) -> (u64, u64, u128, u64) {
        (
            self.build.attention_ticks,
            self.build.input_mass_mg,
            self.build.metabolic_nj,
            self.build.hydration_ul,
        )
    }

    const fn lifecycle_key(self) -> (u64, u128, u64, u64) {
        (
            self.lifecycle_attention,
            self.lifecycle_metabolic_nj,
            self.lifecycle_hydration_ul,
            self.build.input_mass_mg,
        )
    }
}

fn primitive_candidate_routes(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    shaped: StockpileId,
    store_definition: EnergyStoreDefinitionId,
    capacity_nj: u128,
) -> [PrimitiveCandidateRoute; 3] {
    PrimitivePowerChoice::ALL.map(|choice| PrimitiveCandidateRoute {
        choice,
        build: project_power_package(
            registries,
            state,
            raw,
            shaped,
            choice.equipment(),
            store_definition,
            "primitive provider package",
        ),
        route: ManualPowerRoute::new(
            choice.method(),
            choice.equipment(),
            store_definition,
            Energy::from_nanojoules(capacity_nj),
            "primitive provider",
        ),
    })
}

fn primitive_candidate_projection(
    candidate: PrimitiveCandidateRoute,
    lifecycle: lifecycle::ManualPowerLifecycleCost,
) -> PrimitiveCandidateProjection {
    PrimitiveCandidateProjection {
        choice: candidate.choice,
        build: candidate.build,
        lifecycle_attention: candidate
            .build
            .attention_ticks
            .checked_add(lifecycle.attention_ticks)
            .unwrap_or_else(|| panic!("primitive provider lifecycle attention overflowed")),
        lifecycle_metabolic_nj: candidate
            .build
            .metabolic_nj
            .checked_add(lifecycle.metabolic_nj)
            .unwrap_or_else(|| panic!("primitive provider lifecycle metabolism overflowed")),
        lifecycle_hydration_ul: candidate
            .build
            .hydration_ul
            .checked_add(lifecycle.hydration_ul)
            .unwrap_or_else(|| panic!("primitive provider lifecycle hydration overflowed")),
        lifecycle_condition: lifecycle.condition_after,
    }
}

fn select_primitive_candidate(
    investment_policy: CapitalInvestmentPolicy,
    candidates: &[PrimitiveCandidateProjection; 3],
) -> (PrimitivePowerChoice, u64) {
    let baseline_key = candidates
        .iter()
        .map(|candidate| candidate.setup_key())
        .min()
        .unwrap_or_else(|| unreachable!("primitive provider set is nonempty"));
    let mut baselines = candidates
        .iter()
        .copied()
        .filter(|candidate| candidate.setup_key() == baseline_key);
    let baseline = baselines
        .next()
        .unwrap_or_else(|| unreachable!("minimum primitive setup key came from one candidate"));
    assert!(
        baselines.next().is_none(),
        "primitive power providers tie on every actor-visible setup cost; author an explicit preference"
    );
    let eligible = candidates
        .iter()
        .copied()
        .filter(|candidate| {
            candidate.choice == baseline.choice
                || clears_attention_return(
                    baseline.lifecycle_attention,
                    candidate.lifecycle_attention,
                    investment_policy.minimum_attention_return(
                        baseline.build.attention_ticks,
                        candidate.build.attention_ticks,
                    ),
                )
        })
        .collect::<Vec<_>>();
    let best_key = eligible
        .iter()
        .map(|candidate| candidate.lifecycle_key())
        .min()
        .unwrap_or_else(|| unreachable!("primitive baseline is always eligible"));
    let mut best = eligible
        .into_iter()
        .filter(|candidate| candidate.lifecycle_key() == best_key);
    let selected = best
        .next()
        .unwrap_or_else(|| unreachable!("minimum primitive lifecycle key came from one candidate"));
    assert!(
        best.next().is_none(),
        "primitive power providers tie on every actor-visible lifecycle cost; author an explicit preference"
    );
    let minimum_return = investment_policy.minimum_attention_return(
        baseline.build.attention_ticks,
        selected.build.attention_ticks,
    );
    (selected.choice, minimum_return)
}

fn primitive_projection_for(
    candidates: &[PrimitiveCandidateProjection; 3],
    choice: PrimitivePowerChoice,
) -> PrimitiveCandidateProjection {
    candidates
        .iter()
        .copied()
        .find(|candidate| candidate.choice == choice)
        .unwrap_or_else(|| unreachable!("every primitive choice has one candidate projection"))
}

#[derive(Clone, Copy)]
pub(super) struct PrimitivePowerProject {
    pub(super) store_definition: EnergyStoreDefinitionId,
    pub(super) capacity_nj: u128,
    pub(super) consumer: EquipmentId,
    pub(super) declared_mass: Mass,
    pub(super) declared_work_nj: u128,
}

#[derive(Clone, Copy)]
pub(super) struct SettlementPowerProject {
    pub(super) capacity_nj: u128,
    pub(super) declared_mass: Mass,
    pub(super) declared_work_nj: u128,
}

#[derive(Clone, Copy)]
pub(super) struct SettlementPowerPlan {
    pub(super) choice: SettlementPowerChoice,
    pub(super) minimum_return_ppm: u64,
    pub(super) capacity_nj: u128,
    pub(super) declared_mass: Mass,
    pub(super) declared_work_nj: u128,
    pub(super) charge_events: u64,
    pub(super) treadle_build: ShapedBuild,
    pub(super) walking_build: ShapedBuild,
    pub(super) treadle_charge: ManualPowerProjection,
    pub(super) walking_charge: ManualPowerProjection,
    pub(super) stone_crank_lifecycle_attention: u64,
    pub(super) copper_crank_lifecycle_attention: u64,
    pub(super) treadle_lifecycle_attention: u64,
    pub(super) treadle_dynamo_lifecycle_attention: u64,
    pub(super) walking_lifecycle_attention: u64,
    pub(super) stone_crank_lifecycle_metabolic_nj: u128,
    pub(super) copper_crank_lifecycle_metabolic_nj: u128,
    pub(super) treadle_lifecycle_metabolic_nj: u128,
    pub(super) treadle_dynamo_lifecycle_metabolic_nj: u128,
    pub(super) walking_lifecycle_metabolic_nj: u128,
    pub(super) stone_crank_lifecycle_hydration_ul: u64,
    pub(super) copper_crank_lifecycle_hydration_ul: u64,
    pub(super) treadle_lifecycle_hydration_ul: u64,
    pub(super) treadle_dynamo_lifecycle_hydration_ul: u64,
    pub(super) walking_lifecycle_hydration_ul: u64,
    pub(super) stone_crank_lifecycle_condition: Condition,
    pub(super) copper_crank_lifecycle_condition: Condition,
    pub(super) treadle_lifecycle_condition: Condition,
    pub(super) treadle_dynamo_lifecycle_condition: Condition,
    pub(super) walking_lifecycle_condition: Condition,
    #[cfg_attr(
        test,
        allow(dead_code, reason = "exploratory power-provider report telemetry")
    )]
    pub(super) minimum_attention_return_ticks: u64,
}

#[derive(Clone, Copy)]
struct SettlementCandidateRoute {
    choice: SettlementPowerChoice,
    build: ShapedBuild,
    route: ManualPowerRoute,
    uses_copper: bool,
}

#[derive(Clone, Copy)]
struct SettlementCandidateProjection {
    choice: SettlementPowerChoice,
    build: ShapedBuild,
    charge: ManualPowerProjection,
    uses_copper: bool,
    lifecycle_attention: u64,
    lifecycle_metabolic_nj: u128,
    lifecycle_hydration_ul: u64,
    lifecycle_condition: Condition,
}

impl SettlementCandidateProjection {
    const fn setup_key(self) -> (u64, u64, u128, u64) {
        (
            self.build.attention_ticks,
            self.build.input_mass_mg,
            self.build.metabolic_nj,
            self.build.hydration_ul,
        )
    }

    const fn lifecycle_key(self) -> (u64, u128, u64, u64) {
        (
            self.lifecycle_attention,
            self.lifecycle_metabolic_nj,
            self.lifecycle_hydration_ul,
            self.build.input_mass_mg,
        )
    }
}

fn settlement_candidate_routes(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    shaped: StockpileId,
    capacity_nj: u128,
) -> [SettlementCandidateRoute; 5] {
    SettlementPowerChoice::ALL.map(|choice| SettlementCandidateRoute {
        choice,
        build: project_power_package(
            registries,
            state,
            raw,
            shaped,
            choice.equipment(),
            ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
            "settlement provider package",
        ),
        route: ManualPowerRoute::new(
            choice.method(),
            choice.equipment(),
            ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
            Energy::from_nanojoules(capacity_nj),
            "settlement provider",
        ),
        uses_copper: choice.uses_copper(registries),
    })
}

fn project_settlement_candidates(
    registries: &Registries,
    routes: [SettlementCandidateRoute; 5],
    declared_work: Energy,
) -> [SettlementCandidateProjection; 5] {
    routes.map(|candidate| {
        let lifecycle = candidate.route.project_lifecycle(registries, declared_work);
        SettlementCandidateProjection {
            choice: candidate.choice,
            build: candidate.build,
            charge: candidate.route.project(registries, Condition::PRISTINE),
            uses_copper: candidate.uses_copper,
            lifecycle_attention: candidate
                .build
                .attention_ticks
                .checked_add(lifecycle.attention_ticks)
                .unwrap_or_else(|| panic!("settlement provider lifecycle attention overflowed")),
            lifecycle_metabolic_nj: candidate
                .build
                .metabolic_nj
                .checked_add(lifecycle.metabolic_nj)
                .unwrap_or_else(|| panic!("settlement provider lifecycle metabolism overflowed")),
            lifecycle_hydration_ul: candidate
                .build
                .hydration_ul
                .checked_add(lifecycle.hydration_ul)
                .unwrap_or_else(|| panic!("settlement provider lifecycle hydration overflowed")),
            lifecycle_condition: lifecycle.condition_after,
        }
    })
}

fn select_settlement_candidate(
    investment_policy: CapitalInvestmentPolicy,
    copper_policy: SettlementCopperPolicy,
    candidates: &[SettlementCandidateProjection; 5],
) -> (SettlementPowerChoice, u64) {
    let available = candidates.iter().copied().filter(|candidate| {
        copper_policy == SettlementCopperPolicy::SpendAvailable || !candidate.uses_copper
    });
    let available = available.collect::<Vec<_>>();
    let baseline_key = available
        .iter()
        .map(|candidate| candidate.setup_key())
        .min()
        .unwrap_or_else(|| panic!("settlement copper policy removed every power provider"));
    let mut baselines = available
        .iter()
        .copied()
        .filter(|candidate| candidate.setup_key() == baseline_key);
    let baseline = baselines
        .next()
        .unwrap_or_else(|| unreachable!("minimum setup key came from one candidate"));
    assert!(
        baselines.next().is_none(),
        "settlement power providers tie on every actor-visible setup cost; author an explicit preference"
    );

    let eligible = available.iter().copied().filter(|candidate| {
        if candidate.choice == baseline.choice {
            return true;
        }
        clears_attention_return(
            baseline.lifecycle_attention,
            candidate.lifecycle_attention,
            investment_policy.minimum_attention_return(
                baseline.build.attention_ticks,
                candidate.build.attention_ticks,
            ),
        )
    });
    let eligible = eligible.collect::<Vec<_>>();
    let best_key = eligible
        .iter()
        .map(|candidate| candidate.lifecycle_key())
        .min()
        .unwrap_or_else(|| unreachable!("baseline settlement provider is always eligible"));
    let mut best = eligible
        .into_iter()
        .filter(|candidate| candidate.lifecycle_key() == best_key);
    let selected = best
        .next()
        .unwrap_or_else(|| unreachable!("minimum lifecycle key came from one candidate"));
    assert!(
        best.next().is_none(),
        "settlement power providers tie on every actor-visible lifecycle cost; author an explicit preference"
    );
    let minimum_return = investment_policy.minimum_attention_return(
        baseline.build.attention_ticks,
        selected.build.attention_ticks,
    );
    (selected.choice, minimum_return)
}

fn settlement_projection_for(
    candidates: &[SettlementCandidateProjection; 5],
    choice: SettlementPowerChoice,
) -> SettlementCandidateProjection {
    candidates
        .iter()
        .copied()
        .find(|candidate| candidate.choice == choice)
        .unwrap_or_else(|| unreachable!("every settlement choice has one candidate projection"))
}

pub(super) fn settlement_power_decision_frontier(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    shaped: StockpileId,
    capacity_nj: u128,
    copper_policy: SettlementCopperPolicy,
    investment_policy: CapitalInvestmentPolicy,
) -> Vec<(u64, SettlementPowerChoice)> {
    let routes = settlement_candidate_routes(registries, state, raw, shaped, capacity_nj);
    let lifecycle: [Vec<lifecycle::ManualPowerLifecycleCost>; 5] = std::array::from_fn(|index| {
        routes[index]
            .route
            .project_full_charge_series(registries, MAX_SETTLEMENT_CROSSOVER_CHARGES)
    });
    let mut frontier = Vec::new();
    let mut previous = None;
    for charges in 1..=MAX_SETTLEMENT_CROSSOVER_CHARGES {
        let index = usize::try_from(charges - 1)
            .unwrap_or_else(|_| unreachable!("bounded settlement charge index fits usize"));
        let candidates = std::array::from_fn(|candidate| {
            let route = routes[candidate];
            let lifecycle = lifecycle[candidate][index];
            SettlementCandidateProjection {
                choice: route.choice,
                build: route.build,
                charge: route.route.project(registries, Condition::PRISTINE),
                uses_copper: route.uses_copper,
                lifecycle_attention: route
                    .build
                    .attention_ticks
                    .checked_add(lifecycle.attention_ticks)
                    .unwrap_or_else(|| panic!("settlement frontier attention overflowed")),
                lifecycle_metabolic_nj: route
                    .build
                    .metabolic_nj
                    .checked_add(lifecycle.metabolic_nj)
                    .unwrap_or_else(|| panic!("settlement frontier metabolism overflowed")),
                lifecycle_hydration_ul: route
                    .build
                    .hydration_ul
                    .checked_add(lifecycle.hydration_ul)
                    .unwrap_or_else(|| panic!("settlement frontier hydration overflowed")),
                lifecycle_condition: lifecycle.condition_after,
            }
        });
        let choice = select_settlement_candidate(investment_policy, copper_policy, &candidates).0;
        if previous != Some(choice) {
            frontier.push((charges, choice));
            previous = Some(choice);
        }
    }
    frontier
}

pub(super) fn settlement_power_plan(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    shaped: StockpileId,
    project: SettlementPowerProject,
    copper_policy: SettlementCopperPolicy,
    investment_policy: CapitalInvestmentPolicy,
) -> SettlementPowerPlan {
    let routes = settlement_candidate_routes(registries, state, raw, shaped, project.capacity_nj);
    let charge_events = charge_events_for_declared_work(
        project.declared_work_nj,
        project.capacity_nj,
        "settlement project",
    );
    let declared_work = Energy::from_nanojoules(project.declared_work_nj);
    let candidates = project_settlement_candidates(registries, routes, declared_work);
    let (choice, minimum_attention_return_ticks) =
        select_settlement_candidate(investment_policy, copper_policy, &candidates);
    let stone = settlement_projection_for(&candidates, SettlementPowerChoice::StoneCrank);
    let copper = settlement_projection_for(&candidates, SettlementPowerChoice::CopperCrank);
    let treadle = settlement_projection_for(&candidates, SettlementPowerChoice::Treadle);
    let treadle_dynamo =
        settlement_projection_for(&candidates, SettlementPowerChoice::TreadleDynamo);
    let walking = settlement_projection_for(&candidates, SettlementPowerChoice::WalkingWheel);
    SettlementPowerPlan {
        choice,
        minimum_return_ppm: investment_policy.minimum_return_ppm(),
        capacity_nj: project.capacity_nj,
        declared_mass: project.declared_mass,
        declared_work_nj: project.declared_work_nj,
        charge_events,
        treadle_build: treadle.build,
        walking_build: walking.build,
        treadle_charge: treadle.charge,
        walking_charge: walking.charge,
        stone_crank_lifecycle_attention: stone.lifecycle_attention,
        copper_crank_lifecycle_attention: copper.lifecycle_attention,
        treadle_lifecycle_attention: treadle.lifecycle_attention,
        treadle_dynamo_lifecycle_attention: treadle_dynamo.lifecycle_attention,
        walking_lifecycle_attention: walking.lifecycle_attention,
        stone_crank_lifecycle_metabolic_nj: stone.lifecycle_metabolic_nj,
        copper_crank_lifecycle_metabolic_nj: copper.lifecycle_metabolic_nj,
        treadle_lifecycle_metabolic_nj: treadle.lifecycle_metabolic_nj,
        treadle_dynamo_lifecycle_metabolic_nj: treadle_dynamo.lifecycle_metabolic_nj,
        walking_lifecycle_metabolic_nj: walking.lifecycle_metabolic_nj,
        stone_crank_lifecycle_hydration_ul: stone.lifecycle_hydration_ul,
        copper_crank_lifecycle_hydration_ul: copper.lifecycle_hydration_ul,
        treadle_lifecycle_hydration_ul: treadle.lifecycle_hydration_ul,
        treadle_dynamo_lifecycle_hydration_ul: treadle_dynamo.lifecycle_hydration_ul,
        walking_lifecycle_hydration_ul: walking.lifecycle_hydration_ul,
        stone_crank_lifecycle_condition: stone.lifecycle_condition,
        copper_crank_lifecycle_condition: copper.lifecycle_condition,
        treadle_lifecycle_condition: treadle.lifecycle_condition,
        treadle_dynamo_lifecycle_condition: treadle_dynamo.lifecycle_condition,
        walking_lifecycle_condition: walking.lifecycle_condition,
        minimum_attention_return_ticks,
    }
}

fn project_power_package(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    shaped: StockpileId,
    equipment: EquipmentDefinitionId,
    store: EnergyStoreDefinitionId,
    context: &'static str,
) -> ShapedBuild {
    let equipment_profile = registries
        .equipment()
        .get_equipment(equipment)
        .and_then(|equipment| equipment.assembly_profile())
        .unwrap_or_else(|| {
            panic!(
                "power provider equipment {} lost authored assembly",
                equipment.value()
            )
        });
    let store_profile = registries
        .energy()
        .get_store(store)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| {
            panic!(
                "power provider store {} lost authored assembly",
                store.value()
            )
        });
    let projection = project_manual_assembly_package(
        registries,
        state,
        &[raw],
        shaped,
        &[equipment_profile, store_profile],
        context,
    );
    ShapedBuild {
        attention_ticks: projection.attention_ticks,
        input_mass_mg: projection.input_mass_mg,
        embodied_mass_mg: projection.embodied_mass_mg,
        metabolic_nj: projection.metabolic_nj,
        hydration_ul: projection.hydration_ul,
    }
}

#[cfg(not(test))]
pub(super) fn primitive_power_decision_frontier(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    shaped: StockpileId,
    store_definition: EnergyStoreDefinitionId,
    capacity_nj: u128,
    investment_policy: CapitalInvestmentPolicy,
) -> Vec<(u64, PrimitivePowerChoice)> {
    let routes = primitive_candidate_routes(
        registries,
        state,
        raw,
        shaped,
        store_definition,
        capacity_nj,
    );
    let lifecycle: [Vec<lifecycle::ManualPowerLifecycleCost>; 3] = std::array::from_fn(|index| {
        routes[index]
            .route
            .project_full_charge_series(registries, MAX_PRIMITIVE_CROSSOVER_CHARGES)
    });
    let mut frontier = Vec::new();
    let mut previous = None;
    for charges in 1..=MAX_PRIMITIVE_CROSSOVER_CHARGES {
        let index = usize::try_from(charges - 1)
            .unwrap_or_else(|_| unreachable!("bounded primitive charge index fits usize"));
        let candidates = std::array::from_fn(|candidate| {
            primitive_candidate_projection(routes[candidate], lifecycle[candidate][index])
        });
        let choice = select_primitive_candidate(investment_policy, &candidates).0;
        if previous != Some(choice) {
            frontier.push((charges, choice));
            previous = Some(choice);
        }
    }
    frontier
}

pub(super) fn primitive_power_plan(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    shaped: StockpileId,
    project: PrimitivePowerProject,
    investment_policy: CapitalInvestmentPolicy,
) -> PrimitivePowerPlan {
    let routes = primitive_candidate_routes(
        registries,
        state,
        raw,
        shaped,
        project.store_definition,
        project.capacity_nj,
    );
    let crank_route = routes[0].route;
    let treadle_route = routes[1].route;
    let crank_charge = crank_route.project(registries, Condition::PRISTINE);
    let treadle_charge = treadle_route.project(registries, Condition::PRISTINE);
    // The project owns a fixed amount of useful mechanical work. Buffer choice only determines
    // how many charging events are needed; it cannot silently resize the player's project.
    let charge_events = charge_events_for_declared_work(
        project.declared_work_nj,
        project.capacity_nj,
        "primitive project",
    );
    let consumer_record = state
        .equipment()
        .get_equipment(project.consumer)
        .unwrap_or_else(|| panic!("power-provider primitive consumer disappeared before planning"));
    let consumer_order = project_powered_ore_order(
        registries,
        PROCESS_CRUSH_ORE,
        consumer_record.definition(),
        project.store_definition,
        PoweredOreOrderRequest::new(
            consumer_record.condition(),
            project.declared_mass,
            MAX_PRIMITIVE_PROJECT_BATCHES,
            PoweredOreOrderMaintenancePolicy::ServiceAtCritical,
        ),
    )
    .unwrap_or_else(|error| {
        panic!("power-provider primitive consumer order projection failed: {error}")
    });
    let consumer_projected_batches = u64::try_from(consumer_order.batches().len())
        .unwrap_or_else(|_| panic!("power-provider projected batch count exceeds u64"));
    let projected_work_nj = consumer_order
        .batches()
        .iter()
        .try_fold(0_u128, |total, batch| {
            total.checked_add(batch.required_energy().nanojoules())
        })
        .unwrap_or_else(|| panic!("power-provider projected consumer work overflowed"));
    assert_eq!(
        projected_work_nj, project.declared_work_nj,
        "consumer-aware batch projection must preserve the declared useful work"
    );
    let candidates = routes.map(|candidate| {
        primitive_candidate_projection(
            candidate,
            candidate.route.project_lifecycle(
                registries,
                Energy::from_nanojoules(project.declared_work_nj),
            ),
        )
    });
    let (choice, minimum_attention_return_ticks) =
        select_primitive_candidate(investment_policy, &candidates);
    let crank = primitive_projection_for(&candidates, PrimitivePowerChoice::Crank);
    let treadle = primitive_projection_for(&candidates, PrimitivePowerChoice::Treadle);
    PrimitivePowerPlan {
        choice,
        minimum_return_ppm: investment_policy.minimum_return_ppm(),
        store_definition: project.store_definition,
        capacity_nj: project.capacity_nj,
        declared_mass: project.declared_mass,
        declared_work_nj: project.declared_work_nj,
        charge_events,
        consumer_projected_batches,
        consumer_projected_services: consumer_order.services(),
        crank_build: crank.build,
        treadle_build: treadle.build,
        crank_charge,
        treadle_charge,
        crank_lifecycle_attention: crank.lifecycle_attention,
        treadle_lifecycle_attention: treadle.lifecycle_attention,
        crank_lifecycle_metabolic_nj: crank.lifecycle_metabolic_nj,
        treadle_lifecycle_metabolic_nj: treadle.lifecycle_metabolic_nj,
        crank_lifecycle_hydration_ul: crank.lifecycle_hydration_ul,
        treadle_lifecycle_hydration_ul: treadle.lifecycle_hydration_ul,
        crank_lifecycle_condition: crank.lifecycle_condition,
        treadle_lifecycle_condition: treadle.lifecycle_condition,
        minimum_attention_return_ticks,
    }
}
