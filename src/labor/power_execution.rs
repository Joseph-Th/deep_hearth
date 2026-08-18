//! Direct player-power transactions; lifecycle owns exclusivity while energy/equipment owners commit completion consequences.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityValue, CapabilityValueKind};
use crate::core::quantity::{Energy, Power};
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::energy::{
    EnergyCarrier, EnergySinkError, EnergyStoreId, EnergyStoreRecord,
    apply_released_energy_outcomes, calculate_power_duration_ceiling, validate_energy_sink,
};
use crate::equipment::{EquipmentId, EquipmentProviderError, resolve_equipment_provider};
use crate::maintenance::calculate_condition_after_active_ticks;
use crate::mining::MiningJobId;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::registry::Registries;

use super::power_physics::{
    ManualPowerMetabolicDurationError, calculate_metabolic_duration, metabolic_output_per_tick,
};
use super::{
    ManualPowerMethodId, ManualPowerWork, PlayerWork, PlayerWorkCommitError, PlayerWorkStartError,
    ValidatedPlayerWorkStart, validate_player_work_start,
};

/// Direct-labor request to place an exact quantity of generated work into one finite store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualPowerRequest {
    method: ManualPowerMethodId,
    equipment: EquipmentId,
    destination: EnergyStoreId,
    energy: Energy,
}

impl ManualPowerRequest {
    #[must_use]
    pub const fn new(
        method: ManualPowerMethodId,
        equipment: EquipmentId,
        destination: EnergyStoreId,
        energy: Energy,
    ) -> Self {
        Self {
            method,
            equipment,
            destination,
            energy,
        }
    }
}

/// Failure while resolving one direct player-power work order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualPowerError {
    UnknownMethod {
        method: ManualPowerMethodId,
    },
    Work(PlayerWorkStartError),
    Equipment(EquipmentProviderError),
    EquipmentBusyProduction {
        equipment: EquipmentId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    MissingPowerCapability {
        equipment: EquipmentId,
        capability: crate::capability::CapabilityId,
    },
    PowerCapabilityKindMismatch {
        equipment: EquipmentId,
        capability: crate::capability::CapabilityId,
        found: CapabilityValueKind,
    },
    ZeroEquipmentPower {
        equipment: EquipmentId,
        capability: crate::capability::CapabilityId,
    },
    EnergySink(EnergySinkError),
    WrongCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    ZeroTransferPower {
        equipment: EquipmentId,
        destination: EnergyStoreId,
    },
    PowerDuration {
        energy: Energy,
        power: Power,
    },
    MetabolicConversionTooSmall {
        method: ManualPowerMethodId,
    },
    MetabolicDurationOverflow {
        method: ManualPowerMethodId,
        energy: Energy,
    },
    CompletionTickOverflow {
        method: ManualPowerMethodId,
    },
}

impl Display for ManualPowerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMethod { method } => {
                write!(formatter, "unknown manual power method {}", method.value())
            }
            Self::Work(error) => write!(formatter, "manual power labor admission failed: {error}"),
            Self::Equipment(error) => write!(formatter, "manual power equipment failed: {error}"),
            Self::EquipmentBusyProduction {
                equipment,
                job,
                release,
            } => write!(
                formatter,
                "manual power equipment {} is occupied by production job {} {release}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "manual power equipment {} is occupied by mining job {}",
                equipment.value(),
                job.value()
            ),
            Self::MissingPowerCapability {
                equipment,
                capability,
            } => write!(
                formatter,
                "manual power equipment {} lacks authored power capability {}",
                equipment.value(),
                capability.value()
            ),
            Self::PowerCapabilityKindMismatch {
                equipment,
                capability,
                found,
            } => write!(
                formatter,
                "manual power equipment {} capability {} has {found:?} value kind instead of Power",
                equipment.value(),
                capability.value()
            ),
            Self::ZeroEquipmentPower {
                equipment,
                capability,
            } => write!(
                formatter,
                "manual power equipment {} capability {} currently resolves zero output power",
                equipment.value(),
                capability.value()
            ),
            Self::EnergySink(error) => {
                write!(formatter, "manual power destination failed: {error}")
            }
            Self::WrongCarrier { required, provided } => write!(
                formatter,
                "manual power method requires {required:?} storage but destination is {provided:?}"
            ),
            Self::ZeroTransferPower {
                equipment,
                destination,
            } => write!(
                formatter,
                "manual power equipment {} and destination store {} have no common transfer power",
                equipment.value(),
                destination.value()
            ),
            Self::PowerDuration { energy, power } => write!(
                formatter,
                "manual power output of {} nJ at {} pW cannot be transferred within the authoritative tick range",
                energy.nanojoules(),
                power.picowatts()
            ),
            Self::MetabolicConversionTooSmall { method } => write!(
                formatter,
                "manual power method {} metabolic conversion produces less than one nanojoule per active tick",
                method.value()
            ),
            Self::MetabolicDurationOverflow { method, energy } => write!(
                formatter,
                "manual power method {} requires more than the authoritative tick range to generate {} nJ",
                method.value(),
                energy.nanojoules()
            ),
            Self::CompletionTickOverflow { method } => write!(
                formatter,
                "manual power method {} completion exceeds the world clock range",
                method.value()
            ),
        }
    }
}

impl Error for ManualPowerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Work(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::EnergySink(error) => Some(error),
            Self::UnknownMethod { method: _ }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::MissingPowerCapability { .. }
            | Self::PowerCapabilityKindMismatch { .. }
            | Self::ZeroEquipmentPower { .. }
            | Self::WrongCarrier { .. }
            | Self::ZeroTransferPower { .. }
            | Self::PowerDuration { .. }
            | Self::MetabolicConversionTooSmall { .. }
            | Self::MetabolicDurationOverflow { .. }
            | Self::CompletionTickOverflow { .. } => None,
        }
    }
}

/// Commit-time conflict for a resolved direct player-power start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualPowerCommitError {
    Work(PlayerWorkCommitError),
    StaleEquipmentRevision {
        expected: u64,
        actual: u64,
    },
    StaleEnergyRevision {
        expected: u64,
        actual: u64,
    },
    StaleStructureRevision {
        expected: u64,
        actual: u64,
    },
    EquipmentBusyProduction {
        equipment: EquipmentId,
        job: ProductionJobId,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    EnergyBusyProduction {
        store: EnergyStoreId,
        job: ProductionJobId,
    },
}

impl Display for ManualPowerCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Work(error) => write!(
                formatter,
                "manual power labor changed after validation: {error}"
            ),
            Self::StaleEquipmentRevision { expected, actual } => write!(
                formatter,
                "manual power expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::StaleEnergyRevision { expected, actual } => write!(
                formatter,
                "manual power expected energy revision {expected} but current revision is {actual}"
            ),
            Self::StaleStructureRevision { expected, actual } => write!(
                formatter,
                "manual power expected structural revision {expected} but current revision is {actual}"
            ),
            Self::EquipmentBusyProduction { equipment, job } => write!(
                formatter,
                "manual power equipment {} became occupied by production job {} after validation",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "manual power equipment {} became occupied by mining job {} after validation",
                equipment.value(),
                job.value()
            ),
            Self::EnergyBusyProduction { store, job } => write!(
                formatter,
                "manual power destination store {} became occupied by production job {} after validation",
                store.value(),
                job.value()
            ),
        }
    }
}

impl Error for ManualPowerCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Work(error) => Some(error),
            Self::StaleEquipmentRevision { .. }
            | Self::StaleEnergyRevision { .. }
            | Self::StaleStructureRevision { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EnergyBusyProduction { .. } => None,
        }
    }
}

/// Revision-bound admission token for direct player-powered generation.
#[must_use]
pub struct ValidatedManualPowerStart {
    work_start: ValidatedPlayerWorkStart,
    work: ManualPowerWork,
    expected_equipment_revision: u64,
    expected_energy_revision: u64,
    expected_structure_revision: Option<u64>,
}

impl ValidatedManualPowerStart {
    #[must_use]
    pub const fn work(&self) -> ManualPowerWork {
        self.work
    }

    pub fn commit(self, state: &mut AppState) -> Result<ManualPowerWork, ManualPowerCommitError> {
        self.work_start
            .precheck(state)
            .map_err(ManualPowerCommitError::Work)?;
        if state.equipment().revision() != self.expected_equipment_revision {
            return Err(ManualPowerCommitError::StaleEquipmentRevision {
                expected: self.expected_equipment_revision,
                actual: state.equipment().revision(),
            });
        }
        if state.energy().revision() != self.expected_energy_revision {
            return Err(ManualPowerCommitError::StaleEnergyRevision {
                expected: self.expected_energy_revision,
                actual: state.energy().revision(),
            });
        }
        if let Some(expected) = self.expected_structure_revision
            && state.structures().revision() != expected
        {
            return Err(ManualPowerCommitError::StaleStructureRevision {
                expected,
                actual: state.structures().revision(),
            });
        }
        if let Some(job) = state
            .production()
            .get_equipment_occupant(self.work.equipment())
        {
            return Err(ManualPowerCommitError::EquipmentBusyProduction {
                equipment: self.work.equipment(),
                job: job.id(),
            });
        }
        if let Some(job) = state.mining().get_equipment_occupant(self.work.equipment()) {
            return Err(ManualPowerCommitError::EquipmentBusyMining {
                equipment: self.work.equipment(),
                job,
            });
        }
        if let Some(job) = state
            .production()
            .get_energy_occupant(self.work.destination())
        {
            return Err(ManualPowerCommitError::EnergyBusyProduction {
                store: self.work.destination(),
                job,
            });
        }
        self.work_start.apply(state);
        Ok(self.work)
    }
}

/// Resolves and admits a direct player-power work order without creating energy before work finishes.
pub fn validate_start_manual_power(
    registries: &Registries,
    state: &AppState,
    request: ManualPowerRequest,
) -> Result<ValidatedManualPowerStart, ManualPowerError> {
    let definition = registries
        .labor()
        .get_manual_power(request.method)
        .copied()
        .ok_or(ManualPowerError::UnknownMethod {
            method: request.method,
        })?;
    let provider = resolve_equipment_provider(registries, state, request.equipment)
        .map_err(ManualPowerError::Equipment)?;
    if let Some(job) = state.production().get_equipment_occupant(request.equipment) {
        return Err(ManualPowerError::EquipmentBusyProduction {
            equipment: request.equipment,
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    if let Some(job) = state.mining().get_equipment_occupant(request.equipment) {
        return Err(ManualPowerError::EquipmentBusyMining {
            equipment: request.equipment,
            job,
        });
    }
    let power_value = provider
        .get_capability(definition.power_capability())
        .ok_or(ManualPowerError::MissingPowerCapability {
            equipment: request.equipment,
            capability: definition.power_capability(),
        })?;
    let CapabilityValue::Power(equipment_power) = power_value else {
        return Err(ManualPowerError::PowerCapabilityKindMismatch {
            equipment: request.equipment,
            capability: definition.power_capability(),
            found: power_value.kind(),
        });
    };
    if equipment_power.is_zero() {
        return Err(ManualPowerError::ZeroEquipmentPower {
            equipment: request.equipment,
            capability: definition.power_capability(),
        });
    }
    let sink = validate_energy_sink(registries, state, request.destination, request.energy)
        .map_err(ManualPowerError::EnergySink)?;
    if sink.trace().carrier() != definition.carrier() {
        return Err(ManualPowerError::WrongCarrier {
            required: definition.carrier(),
            provided: sink.trace().carrier(),
        });
    }
    let transfer_power = std::cmp::min(equipment_power, sink.max_input_power());
    if transfer_power == Power::ZERO {
        return Err(ManualPowerError::ZeroTransferPower {
            equipment: request.equipment,
            destination: request.destination,
        });
    }
    let power_duration = calculate_power_duration_ceiling(
        transfer_power,
        request.energy,
        registries.core().ticks_per_second(),
    )
    .map_err(|_error| ManualPowerError::PowerDuration {
        energy: request.energy,
        power: transfer_power,
    })?;
    let metabolic_output = metabolic_output_per_tick(
        definition.exertion().energy_cost_per_tick(),
        definition.metabolic_efficiency_ppm(),
    );
    let metabolic_duration = calculate_metabolic_duration(request.energy, metabolic_output)
        .map_err(|error| match error {
            ManualPowerMetabolicDurationError::ZeroOutput => {
                ManualPowerError::MetabolicConversionTooSmall {
                    method: request.method,
                }
            }
            ManualPowerMetabolicDurationError::DurationOverflow => {
                ManualPowerError::MetabolicDurationOverflow {
                    method: request.method,
                    energy: request.energy,
                }
            }
        })?;
    let duration = std::cmp::max(power_duration, metabolic_duration);
    let completes_at = state.tick().checked_add_span(duration).ok_or(
        ManualPowerError::CompletionTickOverflow {
            method: request.method,
        },
    )?;
    let equipment_use = provider.validated_use();
    let condition_after = calculate_condition_after_active_ticks(
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
        duration,
    );
    let work = ManualPowerWork::new(
        request.method,
        equipment_use.trace(),
        condition_after,
        sink.trace(),
        state.tick(),
        completes_at,
    );
    let work_start = validate_player_work_start(
        registries,
        state,
        PlayerWork::ManualPower { work },
        duration,
        definition.exertion(),
    )
    .map_err(ManualPowerError::Work)?;
    Ok(ValidatedManualPowerStart {
        work_start,
        work,
        expected_equipment_revision: equipment_use.expected_equipment_revision(),
        expected_energy_revision: state.energy().revision(),
        expected_structure_revision: equipment_use.expected_structure_revision(),
    })
}

/// Observable completion of one direct player-powered generation work order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualPowerOutcome {
    method: ManualPowerMethodId,
    equipment: EquipmentId,
    destination: EnergyStoreId,
    energy: Energy,
}

impl ManualPowerOutcome {
    #[must_use]
    pub const fn method(self) -> ManualPowerMethodId {
        self.method
    }
    #[must_use]
    pub const fn equipment(self) -> EquipmentId {
        self.equipment
    }
    #[must_use]
    pub const fn destination(self) -> EnergyStoreId {
        self.destination
    }
    #[must_use]
    pub const fn energy(self) -> Energy {
        self.energy
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ManualPowerTickError {
    EnergyRevisionExhausted,
    EquipmentRevisionExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ManualPowerTickPlan {
    work: ManualPowerWork,
    stored_before: Energy,
}

pub(crate) fn decide_manual_power_tick(
    state: &AppState,
    next_tick: SimulationTick,
) -> Result<Option<ManualPowerTickPlan>, ManualPowerTickError> {
    let Some(PlayerWork::ManualPower { work }) = state.player_work().active() else {
        return Ok(None);
    };
    if work.completes_at() != next_tick {
        return Ok(None);
    }
    state
        .energy()
        .revision()
        .checked_add(2)
        .ok_or(ManualPowerTickError::EnergyRevisionExhausted)?;
    state
        .equipment()
        .revision()
        .checked_add(2)
        .ok_or(ManualPowerTickError::EquipmentRevisionExhausted)?;
    let stored_before = state
        .energy()
        .get_store(work.destination())
        .unwrap_or_else(|| panic!("runtime invariant broken: manual power destination disappeared"))
        .stored();
    Ok(Some(ManualPowerTickPlan {
        work,
        stored_before,
    }))
}

pub(crate) fn apply_manual_power_tick(
    state: &mut AppState,
    plan: Option<ManualPowerTickPlan>,
) -> Option<ManualPowerOutcome> {
    let plan = plan?;
    let work = plan.work;
    let equipment = state
        .equipment()
        .get_equipment(work.equipment())
        .unwrap_or_else(|| panic!("runtime invariant broken: manual power equipment disappeared"));
    assert_eq!(
        equipment.condition(),
        work.equipment_trace().condition(),
        "manual power occupancy must prevent equipment condition mutation while work is active"
    );
    assert_eq!(
        state
            .energy()
            .get_store(work.destination())
            .map(EnergyStoreRecord::stored),
        Some(plan.stored_before),
        "manual power occupancy must prevent destination mutation while work is active"
    );

    let energy_revision = state.energy().revision();
    let next_energy_revision = energy_revision
        .checked_add(1)
        .unwrap_or_else(|| panic!("prevalidated manual power energy revision exhausted"));
    apply_released_energy_outcomes(
        state.energy_state_mut(),
        energy_revision,
        next_energy_revision,
        &[work.output()],
    );

    let equipment_revision = state.equipment().revision();
    let next_equipment_revision = equipment_revision
        .checked_add(1)
        .unwrap_or_else(|| panic!("prevalidated manual power equipment revision exhausted"));
    state.equipment_state_mut().apply_condition_change(
        work.equipment(),
        work.equipment_trace().condition(),
        work.condition_after(),
        next_equipment_revision,
    );

    Some(ManualPowerOutcome {
        method: work.method(),
        equipment: work.equipment(),
        destination: work.destination(),
        energy: work.output().energy(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{
        ENERGY_MECHANICAL_LARGE_DRIVE, ENERGY_MECHANICAL_SMALL_DRIVE,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK, EQUIPMENT_STONE_HAND_CRANK, FORM_FLYWHEEL,
        FORM_HANDLE, FORM_LOG, FORM_LUMP, FORM_REINFORCEMENT, MANUAL_POWER_HAND_CRANK,
        MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD, PROCESS_SHAPE_STONE_FLYWHEEL,
        PROCESS_SHAPE_WOOD_HANDLE, build_registries,
    };
    use crate::core::quantity::{Mass, Temperature};
    use crate::core::state::{StateValidationError, validate_loaded_state};
    use crate::core::time::WorldSeed;
    use crate::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
    use crate::energy::{
        EnergySupplyError, EnergyTransferCommitError, add_energy_store,
        add_energy_store_with_initial_for_test, make_test_energy_transfer_resolution,
        validate_energy_supply, validate_energy_transfer,
    };
    use crate::equipment::{
        EquipmentConditionPlanError, decide_equipment_wear, validate_assemble_equipment,
    };
    use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
    use crate::labor::PlayerWorkValidationError;
    use crate::material::CommodityKey;
    use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
    use crate::simulation::advance_tick;
    use crate::survival::{assess_survival, initialize_player_survival};

    fn advance_exact(registries: &Registries, state: &mut AppState, ticks: u64) {
        for _ in 0..ticks {
            advance_tick(registries, state)
                .unwrap_or_else(|error| panic!("manual power setup tick failed: {error}"));
        }
    }

    fn assemble_crank_fixture(
        registries: &Registries,
        state: &mut AppState,
        definition: crate::equipment::EquipmentDefinitionId,
        with_copper: bool,
    ) -> EquipmentId {
        let capacity = if with_copper {
            Mass::from_milligrams(1_120_000)
        } else {
            Mass::from_milligrams(1_100_000)
        };
        let source = add_solid_stockpile_for_test(state, capacity)
            .unwrap_or_else(|error| panic!("crank comparison source failed: {error}"));
        for (commodity, mass) in [
            Some((
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                Mass::from_milligrams(900_000),
            )),
            Some((
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            )),
            with_copper.then_some((
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                Mass::from_milligrams(20_000),
            )),
        ]
        .into_iter()
        .flatten()
        {
            deposit_lot_for_test(
                registries,
                state,
                source,
                commodity,
                mass,
                Temperature::from_millikelvin(293_150),
            )
            .unwrap_or_else(|error| panic!("crank comparison material failed: {error}"));
        }
        validate_assemble_equipment(registries, state, definition, source)
            .unwrap_or_else(|error| panic!("crank comparison assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("crank comparison assembly commit failed: {error}"))
    }

    #[test]
    fn copper_reinforced_crank_halves_manual_charge_time_without_changing_energy_yield() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A80_0002));
        initialize_player_survival(&registries, &mut state)
            .unwrap_or_else(|error| panic!("crank comparison survival setup failed: {error}"));
        let stone_crank =
            assemble_crank_fixture(&registries, &mut state, EQUIPMENT_STONE_HAND_CRANK, false);
        let reinforced_crank = assemble_crank_fixture(
            &registries,
            &mut state,
            EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
            true,
        );
        let bottleneck_drive =
            add_energy_store(&registries, &mut state, ENERGY_MECHANICAL_SMALL_DRIVE)
                .unwrap_or_else(|error| {
                    panic!("bottleneck crank comparison drive failed: {error}")
                });
        let stone_drive = add_energy_store(&registries, &mut state, ENERGY_MECHANICAL_LARGE_DRIVE)
            .unwrap_or_else(|error| panic!("stone crank comparison drive failed: {error}"));
        let reinforced_drive =
            add_energy_store(&registries, &mut state, ENERGY_MECHANICAL_LARGE_DRIVE)
                .unwrap_or_else(|error| {
                    panic!("reinforced crank comparison drive failed: {error}")
                });
        let requested = Energy::from_nanojoules(25_000_000_000);

        let bottlenecked = validate_start_manual_power(
            &registries,
            &state,
            ManualPowerRequest::new(
                MANUAL_POWER_HAND_CRANK,
                reinforced_crank,
                bottleneck_drive,
                requested,
            ),
        )
        .unwrap_or_else(|error| panic!("bottleneck crank comparison validation failed: {error}"));
        let stone = validate_start_manual_power(
            &registries,
            &state,
            ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, stone_crank, stone_drive, requested),
        )
        .unwrap_or_else(|error| panic!("stone crank comparison validation failed: {error}"));
        let reinforced = validate_start_manual_power(
            &registries,
            &state,
            ManualPowerRequest::new(
                MANUAL_POWER_HAND_CRANK,
                reinforced_crank,
                reinforced_drive,
                requested,
            ),
        )
        .unwrap_or_else(|error| panic!("reinforced crank comparison validation failed: {error}"));

        assert_eq!(
            bottlenecked.work().completes_at().value() - bottlenecked.work().started_at().value(),
            10
        );
        assert_eq!(
            stone.work().completes_at().value() - stone.work().started_at().value(),
            10
        );
        assert_eq!(
            reinforced.work().completes_at().value() - reinforced.work().started_at().value(),
            5
        );
        reinforced
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("reinforced crank comparison commit failed: {error}"));
        let mut completion = None;
        for _ in 0..5 {
            completion = advance_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("reinforced crank comparison tick failed: {error}"))
                .manual_power();
        }
        assert_eq!(completion.map(ManualPowerOutcome::energy), Some(requested));
        assert_eq!(
            state
                .energy()
                .get_store(reinforced_drive)
                .map(EnergyStoreRecord::stored),
            Some(requested)
        );
        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("reinforced crank comparison audit failed: {error}"));
    }

    #[test]
    fn primitive_hand_crank_turns_player_work_into_finite_mechanical_energy() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A80_0001));
        initialize_player_survival(&registries, &mut state)
            .unwrap_or_else(|error| panic!("manual power survival initialization failed: {error}"));
        let raw = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
            .unwrap_or_else(|error| panic!("manual power raw stockpile failed: {error}"));
        let shaped = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
            .unwrap_or_else(|error| panic!("manual power shaped stockpile failed: {error}"));
        deposit_lot_for_test(
            &registries,
            &mut state,
            raw,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(1_000_000),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("manual power stone fixture failed: {error}"));
        deposit_lot_for_test(
            &registries,
            &mut state,
            raw,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1_000_000),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("manual power wood fixture failed: {error}"));

        validate_start_manual_craft(
            &registries,
            &state,
            ManualCraftStartRequest::single(PROCESS_SHAPE_STONE_FLYWHEEL, raw, shaped),
        )
        .unwrap_or_else(|error| panic!("flywheel shaping validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("flywheel shaping commit failed: {error}"));
        advance_exact(&registries, &mut state, 60);
        validate_start_manual_craft(
            &registries,
            &state,
            ManualCraftStartRequest::single(PROCESS_SHAPE_WOOD_HANDLE, raw, shaped),
        )
        .unwrap_or_else(|error| panic!("crank handle shaping validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("crank handle shaping commit failed: {error}"));
        advance_exact(&registries, &mut state, 40);

        let crank =
            validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_HAND_CRANK, shaped)
                .unwrap_or_else(|error| panic!("hand crank assembly validation failed: {error}"))
                .commit(&mut state)
                .unwrap_or_else(|error| panic!("hand crank assembly commit failed: {error}"));
        let drive = add_energy_store(&registries, &mut state, ENERGY_MECHANICAL_SMALL_DRIVE)
            .unwrap_or_else(|error| panic!("manual power drive allocation failed: {error}"));
        let donor = add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            ENERGY_MECHANICAL_SMALL_DRIVE,
            Energy::from_nanojoules(1_000),
        )
        .unwrap_or_else(|error| panic!("manual power donor fixture failed: {error}"));
        let stale_transfer = validate_energy_transfer(
            &registries,
            &state,
            make_test_energy_transfer_resolution(donor, drive, Energy::from_nanojoules(100)),
        )
        .unwrap_or_else(|error| panic!("stale transfer setup failed: {error}"));
        let survival_before = assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("manual power survival state disappeared"));
        let condition_before = state
            .equipment()
            .get_equipment(crank)
            .unwrap_or_else(|| panic!("assembled hand crank disappeared"))
            .condition();

        let requested = Energy::from_nanojoules(25_000_000_000);
        let base_save = serde_json::to_value(SaveEnvelope::new(&registries, &state))
            .unwrap_or_else(|error| panic!("manual power reserve serialization failed: {error}"));
        let mut low_energy = base_save.clone();
        low_energy["state"]["systems"]["survival"]["player"]["metabolic_energy"] =
            serde_json::json!(1_u64);
        let low_energy: LoadedSaveEnvelope = serde_json::from_value(low_energy)
            .unwrap_or_else(|error| panic!("low-energy manual power decode failed: {error}"));
        let low_energy = low_energy
            .into_state(&registries)
            .unwrap_or_else(|error| panic!("low-energy manual power load failed: {error}"));
        assert!(matches!(
            validate_start_manual_power(
                &registries,
                &low_energy,
                ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested,),
            ),
            Err(ManualPowerError::Work(
                PlayerWorkStartError::InsufficientMetabolicEnergy { .. }
            ))
        ));

        let mut low_hydration = base_save;
        low_hydration["state"]["systems"]["survival"]["player"]["hydration"] =
            serde_json::json!(1_u64);
        let low_hydration: LoadedSaveEnvelope = serde_json::from_value(low_hydration)
            .unwrap_or_else(|error| panic!("low-hydration manual power decode failed: {error}"));
        let low_hydration = low_hydration
            .into_state(&registries)
            .unwrap_or_else(|error| panic!("low-hydration manual power load failed: {error}"));
        assert!(matches!(
            validate_start_manual_power(
                &registries,
                &low_hydration,
                ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested,),
            ),
            Err(ManualPowerError::Work(
                PlayerWorkStartError::InsufficientHydration { .. }
            ))
        ));

        let mut stale_survival_state = state.clone();
        let stale_survival = validate_start_manual_power(
            &registries,
            &stale_survival_state,
            ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested),
        )
        .unwrap_or_else(|error| panic!("stale-survival manual power setup failed: {error}"));
        advance_tick(&registries, &mut stale_survival_state)
            .unwrap_or_else(|error| panic!("stale-survival setup tick failed: {error}"));
        assert_eq!(
            stale_survival.commit(&mut stale_survival_state),
            Err(ManualPowerCommitError::Work(
                PlayerWorkCommitError::StaleSurvivalRevision {
                    expected: state.survival().revision(),
                    actual: stale_survival_state.survival().revision(),
                }
            ))
        );

        let token = validate_start_manual_power(
            &registries,
            &state,
            ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested),
        )
        .unwrap_or_else(|error| panic!("manual power validation failed: {error}"));
        let work = token.work();
        assert_eq!(work.completes_at().value() - work.started_at().value(), 10);
        token
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("manual power commit failed: {error}"));

        assert_eq!(
            state
                .energy()
                .get_store(drive)
                .map(EnergyStoreRecord::stored),
            Some(Energy::ZERO)
        );
        assert_eq!(
            validate_energy_supply(&registries, &state, drive, Energy::from_nanojoules(1)),
            Err(EnergySupplyError::StoreBusyManualPower { store: drive })
        );
        assert_eq!(
            decide_equipment_wear(&state, crank, 1),
            Err(EquipmentConditionPlanError::EquipmentBusyManualPower { equipment: crank })
        );
        assert_eq!(
            stale_transfer.commit(&mut state),
            Err(EnergyTransferCommitError::DestinationBusyManualPower { store: drive })
        );

        advance_exact(&registries, &mut state, 5);
        assert_eq!(
            state
                .energy()
                .get_store(drive)
                .map(EnergyStoreRecord::stored),
            Some(Energy::ZERO)
        );
        let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state))
            .unwrap_or_else(|error| panic!("manual power tamper serialization failed: {error}"));
        tampered["state"]["systems"]["player_work"]["active"]["ManualPower"]["work"]["completes_at"] =
            serde_json::json!(work.completes_at().value() + 1);
        let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
            .unwrap_or_else(|error| panic!("manual power tamper decode failed: {error}"));
        assert_eq!(
            tampered.into_state(&registries),
            Err(LoadError::InvalidState(StateValidationError::PlayerWork(
                PlayerWorkValidationError::ManualPowerDurationMismatch
            )))
        );
        let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
            .unwrap_or_else(|error| panic!("active manual power serialization failed: {error}"));
        let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
            .unwrap_or_else(|error| panic!("active manual power decode failed: {error}"));
        let mut loaded = decoded
            .into_state(&registries)
            .unwrap_or_else(|error| panic!("active manual power load validation failed: {error}"));
        assert_eq!(loaded, state);

        let mut completion = None;
        for _ in 0..5 {
            completion = advance_tick(&registries, &mut loaded)
                .unwrap_or_else(|error| panic!("manual power completion tick failed: {error}"))
                .manual_power();
        }
        assert_eq!(completion.map(ManualPowerOutcome::energy), Some(requested));
        assert_eq!(
            loaded
                .energy()
                .get_store(drive)
                .map(EnergyStoreRecord::stored),
            Some(requested)
        );
        assert_eq!(loaded.player_work().active(), None);
        assert!(
            loaded
                .equipment()
                .get_equipment(crank)
                .unwrap_or_else(|| panic!("hand crank disappeared after completion"))
                .condition()
                < condition_before
        );
        assert!(
            assess_survival(&registries, &loaded)
                .unwrap_or_else(|| panic!(
                    "manual power survival state disappeared after completion"
                ))
                .metabolic_energy()
                < survival_before.metabolic_energy()
        );
        let generated_supply = validate_energy_supply(&registries, &loaded, drive, requested)
            .unwrap_or_else(|error| {
                panic!("generated mechanical energy was not consumable: {error}")
            });
        assert_eq!(generated_supply.trace().energy(), requested);
        validate_loaded_state(&registries, &loaded)
            .unwrap_or_else(|error| panic!("manual power final audit failed: {error}"));
    }
}
