//! Familiar recipe-input planning over exact inventory lots.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU64;

use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::inventory::{MaterialLotId, MaterialLotSelection, StockpileId};
use crate::material::{CommodityKey, MaterialComposition};
use crate::production::ProcessId;
use crate::registry::{ProcessEquipmentRole, Registries};

use super::ManualCraftRequest;

#[derive(Clone, Debug, PartialEq, Eq)]
struct CompatibleInputGroup {
    temperature: Temperature,
    mass: Mass,
    lots: Vec<(MaterialLotId, Mass)>,
}

/// How one recipe's material input can be presented to an ordinary inventory caller.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManualCraftInputMode {
    /// Durable homogeneous matter can be selected deterministically behind the recipe action.
    Automatic(ManualCraftInputAvailability),
    /// More than one temperature cohort can independently satisfy at least one batch.
    ///
    /// Choosing between them changes output temperature, so presentation must keep that choice
    /// visible instead of allowing persistent lot identity to decide it implicitly.
    TemperatureChoice(ManualCraftInputAvailability),
    /// Stack-local state such as food age is gameplay-relevant and must remain player-selected.
    ExplicitStackChoice {
        input: CommodityKey,
        batch_mass: Mass,
    },
}

/// One stable recipe-book row for a stockpile, without claiming runtime tool availability.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualCraftStockpileOption {
    process: ProcessId,
    input_mode: ManualCraftInputMode,
    equipment_role: ProcessEquipmentRole,
}

impl ManualCraftStockpileOption {
    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }

    pub const fn input_mode(self) -> ManualCraftInputMode {
        self.input_mode
    }

    #[must_use]
    pub const fn equipment_role(self) -> ProcessEquipmentRole {
        self.equipment_role
    }
}

/// Read-only stockpile availability for one authored manual recipe.
///
/// `maximum_batches` is based on the largest temperature-compatible pool because manual shaping
/// cannot silently mix different material temperatures. `total_eligible_mass` remains visible so a
/// caller can distinguish a true shortage from matter that exists but is split across incompatible
/// thermal states.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualCraftInputAvailability {
    input: CommodityKey,
    batch_mass: Mass,
    total_eligible_mass: Mass,
    largest_compatible_mass: Mass,
    maximum_batches: u64,
    craftable_temperature_groups: u64,
}

/// Builds a deterministic manual-recipe catalog for one inventory custody location.
///
/// Entries are ordered by stable process ID through the crafting registry. The catalog reports
/// material availability and the static tool relationship separately: a recipe with enough matter
/// and `Required` equipment is not claimed to be executable until a runtime equipment instance is
/// chosen and validated by the normal crafting path.
pub fn manual_craft_options_from_stockpile(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
) -> Result<Vec<ManualCraftStockpileOption>, ManualCraftInputPlanError> {
    if state.inventory().get_stockpile(source).is_none() {
        return Err(ManualCraftInputPlanError::UnknownStockpile { stockpile: source });
    }
    let mut options = Vec::new();
    for definition in registries.crafting().definitions() {
        let process = definition.process();
        let input_mode = if input_requires_explicit_stack_choice(registries, definition.input()) {
            ManualCraftInputMode::ExplicitStackChoice {
                input: definition.input(),
                batch_mass: definition.input_mass(),
            }
        } else {
            let availability = assess_manual_craft_inputs(registries, state, process, source)?;
            if availability.craftable_temperature_groups() > 1 {
                ManualCraftInputMode::TemperatureChoice(availability)
            } else {
                ManualCraftInputMode::Automatic(availability)
            }
        };
        let topology = registries.process_topology(process).unwrap_or_else(|| {
            panic!(
                "registered manual craft {} lost its process topology",
                process.value()
            )
        });
        options.push(ManualCraftStockpileOption {
            process,
            input_mode,
            equipment_role: topology.equipment_role(),
        });
    }
    Ok(options)
}

impl ManualCraftInputAvailability {
    #[must_use]
    pub const fn input(self) -> CommodityKey {
        self.input
    }

    #[must_use]
    pub const fn batch_mass(self) -> Mass {
        self.batch_mass
    }

    #[must_use]
    pub const fn total_eligible_mass(self) -> Mass {
        self.total_eligible_mass
    }

    #[must_use]
    pub const fn largest_compatible_mass(self) -> Mass {
        self.largest_compatible_mass
    }

    #[must_use]
    pub const fn maximum_batches(self) -> u64 {
        self.maximum_batches
    }

    /// Number of distinct input temperatures that can each supply at least one complete batch.
    #[must_use]
    pub const fn craftable_temperature_groups(self) -> u64 {
        self.craftable_temperature_groups
    }
}

/// Failure while turning a familiar recipe/batch choice into exact lot selections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualCraftInputPlanError {
    UnknownManualProcess {
        process: ProcessId,
    },
    UnknownStockpile {
        stockpile: StockpileId,
    },
    AgeSensitiveInputRequiresExplicitSelection {
        input: CommodityKey,
    },
    InputMassOverflow {
        process: ProcessId,
        batches: NonZeroU64,
    },
    InsufficientInput {
        input: CommodityKey,
        available: Mass,
        required: Mass,
    },
    SplitTemperatureInput {
        input: CommodityKey,
        available: Mass,
        largest_compatible: Mass,
        required: Mass,
    },
    MultipleCompatibleInputTemperatures {
        input: CommodityKey,
        required: Mass,
        temperatures: Vec<Temperature>,
    },
}

impl Display for ManualCraftInputPlanError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownManualProcess { process } => write!(
                formatter,
                "process {} is not authored as a manual craft",
                process.value()
            ),
            Self::UnknownStockpile { stockpile } => {
                write!(
                    formatter,
                    "unknown manual-craft source stockpile {}",
                    stockpile.value()
                )
            }
            Self::AgeSensitiveInputRequiresExplicitSelection { input } => write!(
                formatter,
                "manual craft input material {} form {} is age-sensitive and requires an explicit stack choice",
                input.material().value(),
                input.form().value()
            ),
            Self::InputMassOverflow { process, batches } => write!(
                formatter,
                "manual craft process {} input mass overflows for {} batches",
                process.value(),
                batches.get()
            ),
            Self::InsufficientInput {
                input,
                available,
                required,
            } => write!(
                formatter,
                "manual craft needs {} mg of material {} form {} but only {} mg of compatible input is available",
                required.milligrams(),
                input.material().value(),
                input.form().value(),
                available.milligrams()
            ),
            Self::SplitTemperatureInput {
                input,
                available,
                largest_compatible,
                required,
            } => write!(
                formatter,
                "manual craft has {} mg of material {} form {} in total, but only {} mg shares one temperature and {} mg is required",
                available.milligrams(),
                input.material().value(),
                input.form().value(),
                largest_compatible.milligrams(),
                required.milligrams()
            ),
            Self::MultipleCompatibleInputTemperatures {
                input,
                required,
                temperatures,
            } => write!(
                formatter,
                "manual craft has {} separate temperature cohorts of material {} form {} that can each supply the required {} mg; choose an input stack explicitly",
                temperatures.len(),
                input.material().value(),
                input.form().value(),
                required.milligrams()
            ),
        }
    }
}

impl Error for ManualCraftInputPlanError {}

pub(super) fn input_requires_explicit_stack_choice(
    registries: &Registries,
    input: CommodityKey,
) -> bool {
    registries.survival().get_food(input).is_some()
}

fn scan_input_groups(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    source: StockpileId,
) -> Result<(ManualCraftInputAvailability, Vec<CompatibleInputGroup>), ManualCraftInputPlanError> {
    let definition = registries
        .crafting()
        .get_manual(process)
        .ok_or(ManualCraftInputPlanError::UnknownManualProcess { process })?;
    if state.inventory().get_stockpile(source).is_none() {
        return Err(ManualCraftInputPlanError::UnknownStockpile { stockpile: source });
    }
    let input = definition.input();
    if input_requires_explicit_stack_choice(registries, input) {
        return Err(
            ManualCraftInputPlanError::AgeSensitiveInputRequiresExplicitSelection { input },
        );
    }

    let expected_composition = MaterialComposition::pure(input.material());
    let mut groups_by_temperature = BTreeMap::<Temperature, CompatibleInputGroup>::new();
    let mut total_eligible_mass = Mass::ZERO;
    for lot_id in state.inventory().lot_ids_for_commodity(source, input) {
        let lot = state.inventory().get_lot(lot_id).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: stockpile {} indexes missing lot {}",
                source.value(),
                lot_id.value()
            )
        });
        debug_assert_eq!(
            lot.commodity(),
            input,
            "commodity index must only return lots for the requested recipe input"
        );
        if lot.composition() != &expected_composition {
            continue;
        }
        total_eligible_mass = total_eligible_mass
            .checked_add(lot.mass())
            .unwrap_or_else(|| panic!("validated stockpile eligible manual-craft mass overflowed"));
        let temperature = lot.temperature();
        let group =
            groups_by_temperature
                .entry(temperature)
                .or_insert_with(|| CompatibleInputGroup {
                    temperature,
                    mass: Mass::ZERO,
                    lots: Vec::new(),
                });
        group.mass = group.mass.checked_add(lot.mass()).unwrap_or_else(|| {
            panic!("validated stockpile compatible manual-craft mass overflowed")
        });
        group.lots.push((lot_id, lot.mass()));
    }
    let groups = groups_by_temperature.into_values().collect::<Vec<_>>();

    let largest_compatible_mass = groups
        .iter()
        .map(|group| group.mass)
        .max()
        .unwrap_or(Mass::ZERO);
    let batch_mass = definition.input_mass();
    let craftable_temperature_groups = groups
        .iter()
        .filter(|group| group.mass >= batch_mass)
        .count()
        .try_into()
        .unwrap_or_else(|_| panic!("manual-craft temperature group count exceeds u64"));
    Ok((
        ManualCraftInputAvailability {
            input,
            batch_mass,
            total_eligible_mass,
            largest_compatible_mass,
            maximum_batches: largest_compatible_mass.milligrams() / batch_mass.milligrams(),
            craftable_temperature_groups,
        },
        groups,
    ))
}

/// Reports how many complete batches one stockpile can supply without mixing input temperatures.
pub fn assess_manual_craft_inputs(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    source: StockpileId,
) -> Result<ManualCraftInputAvailability, ManualCraftInputPlanError> {
    scan_input_groups(registries, state, process, source)
        .map(|(availability, _groups)| availability)
}

/// Converts a recipe-and-batch choice into exact deterministic lot slices.
///
/// This is the player/UI convenience boundary for ordinary fixed-input crafting. It deliberately
/// stops before resolution/admission, so all existing crafting physics, equipment checks, labor,
/// reservations, and commit semantics remain authoritative. Age-sensitive food stays explicit
/// because freshness is a meaningful stack-level decision.
pub fn plan_manual_craft_from_stockpile(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    source: StockpileId,
    batches: NonZeroU64,
) -> Result<ManualCraftRequest, ManualCraftInputPlanError> {
    let (availability, groups) = scan_input_groups(registries, state, process, source)?;
    let required = availability
        .batch_mass()
        .milligrams()
        .checked_mul(batches.get())
        .map(Mass::from_milligrams)
        .ok_or(ManualCraftInputPlanError::InputMassOverflow { process, batches })?;
    if availability.total_eligible_mass() < required {
        return Err(ManualCraftInputPlanError::InsufficientInput {
            input: availability.input(),
            available: availability.total_eligible_mass(),
            required,
        });
    }
    let mut candidates = groups.iter().filter(|group| group.mass >= required);
    let group = candidates
        .next()
        .ok_or(ManualCraftInputPlanError::SplitTemperatureInput {
            input: availability.input(),
            available: availability.total_eligible_mass(),
            largest_compatible: availability.largest_compatible_mass(),
            required,
        })?;
    if candidates.next().is_some() {
        let temperatures = groups
            .iter()
            .filter(|group| group.mass >= required)
            .map(|group| group.temperature)
            .collect::<Vec<_>>();
        return Err(
            ManualCraftInputPlanError::MultipleCompatibleInputTemperatures {
                input: availability.input(),
                required,
                temperatures,
            },
        );
    }

    let mut remaining = required;
    let mut selections = Vec::new();
    for (lot, mass) in &group.lots {
        if remaining.is_zero() {
            break;
        }
        let selected = (*mass).min(remaining);
        selections.push(MaterialLotSelection::new(*lot, selected));
        remaining = remaining.checked_sub(selected).unwrap_or_else(|| {
            unreachable!("selected manual-craft input cannot exceed remaining mass")
        });
    }
    assert!(
        remaining.is_zero(),
        "selected compatible group must satisfy requested batch mass"
    );
    Ok(ManualCraftRequest::new(process, source, selections))
}
