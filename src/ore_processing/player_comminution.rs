//! Actor-visible powered ore worksite choices over exact local custody.
//!
//! No resource is invented: each executable option uses real material lots,
//! authored equipment, finite charged energy, and an explicit output owner.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::{MaterialStackProjection, MaterialStackProjectionError, StockpileId};
use crate::logistics::{PlayerInventoryProjectionError, project_player_inventory};
use crate::material::{CommodityKey, FormId};
use crate::player::{PlayerActionValidationError, validate_player_action};
use crate::production::{
    ProcessId, StartPlayerProcessError, StartProcessError, ValidatedStartProcess,
    validate_start_player_process,
};
use crate::registry::Registries;

use super::{
    ComminutionRequest, ComminutionResolutionError, assess_powered_ore_mass_envelope,
    resolve_comminution_process,
};

/// A currently possible local feed, equipment, power store, and output route.
/// Retains the private lot slices and inventory revision so starting a stale
/// choice never substitutes ore of the same visible name.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerComminutionOption {
    stack: MaterialStackProjection,
    process: ProcessId,
    equipment: EquipmentId,
    store: EnergyStoreId,
    destination: StockpileId,
    maximum_mass: Mass,
    duration: TickSpan,
    required_energy: Energy,
    deferred_until_boundary: bool,
}

impl PlayerComminutionOption {
    pub const fn source(&self) -> StockpileId {
        self.stack.stockpile()
    }
    pub const fn feed(&self) -> CommodityKey {
        self.stack.commodity()
    }
    pub const fn process(&self) -> ProcessId {
        self.process
    }
    pub const fn equipment(&self) -> EquipmentId {
        self.equipment
    }
    pub const fn energy_store(&self) -> EnergyStoreId {
        self.store
    }
    pub const fn destination(&self) -> StockpileId {
        self.destination
    }
    pub const fn maximum_mass(&self) -> Mass {
        self.maximum_mass
    }
    pub const fn duration_for_maximum(&self) -> TickSpan {
        self.duration
    }
    pub const fn required_energy_for_maximum(&self) -> Energy {
        self.required_energy
    }
    /// This is a read-only next-boundary preview, not an admitted job.
    /// The caller must revalidate when the control clock reaches its slow boundary.
    pub const fn deferred_until_boundary(&self) -> bool {
        self.deferred_until_boundary
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PlayerComminutionOptionError {
    ZeroMass,
    AboveCurrentMaximum { requested: Mass, maximum: Mass },
    Stack(MaterialStackProjectionError),
    Comminution(ComminutionResolutionError),
    Start(StartPlayerProcessError),
}

impl Display for PlayerComminutionOptionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "player powered ore option unavailable: {self:?}")
    }
}

impl Error for PlayerComminutionOptionError {}

/// Exact-local powered ore choices at any 20 Hz control time.
/// At a slow boundary the canonical start validates the full route. Between
/// boundaries, projections are explicitly deferred: they have validated feed,
/// equipment, finite charge and visible output headroom, but other start-time
/// prerequisites remain conditional until the next slow boundary. A preview
/// never grants permission to start early or reserve capacity.
pub fn player_local_powered_comminution_options(
    registries: &Registries,
    state: &AppState,
) -> Result<Vec<PlayerComminutionOption>, PlayerInventoryProjectionError> {
    let inventory = project_player_inventory(registries, state)?;
    let stores = state
        .available_local_energy_stores()
        .map(|record| record.id())
        .collect::<Vec<_>>();
    let equipment = inventory
        .equipment()
        .iter()
        .map(|record| record.equipment())
        .chain(
            inventory
                .local_equipment()
                .iter()
                .map(|record| record.equipment()),
        )
        .collect::<Vec<_>>();
    let sources = std::iter::once(inventory.carried()).chain(inventory.local_containers());
    let destinations = std::iter::once(inventory.carried().stockpile())
        .chain(
            inventory
                .local_containers()
                .iter()
                .map(|record| record.stockpile()),
        )
        .collect::<Vec<_>>();
    let mut options = Vec::new();
    // Capability, condition, and energy envelopes do not depend on feed lots or
    // output destinations. A worksite may hold many separate grade/temperature
    // stacks of one ore form: assess its physical providers once per input form
    // during this read-only menu request, not once for every stack.
    let mut capacities_by_form = BTreeMap::<FormId, Vec<AvailableComminutionCapacity>>::new();
    for source in sources {
        for stack in source.stacks() {
            let capacities = capacities_by_form
                .entry(stack.commodity().form())
                .or_insert_with(|| {
                    available_capacities_for_form(
                        registries,
                        state,
                        stack.commodity().form(),
                        &equipment,
                        &stores,
                    )
                });
            for &capacity in capacities.iter() {
                for &destination in &destinations {
                    if let Some(option) =
                        ready_option_for_route(registries, state, stack, capacity, destination)
                    {
                        options.push(option);
                    }
                }
            }
        }
    }
    Ok(options)
}

/// Shared process, machine, and power limit for every stack of an input form.
/// Destination capacity and exact feed selection are still checked separately.
#[derive(Clone, Copy)]
struct AvailableComminutionCapacity {
    process: ProcessId,
    machine: EquipmentId,
    store: EnergyStoreId,
    maximum_mass: Mass,
}

/// Each combination is checked by canonical process admission rather than by
/// guessing whether a container can accept a future product lot.
fn ready_option_for_route(
    registries: &Registries,
    state: &AppState,
    stack: &MaterialStackProjection,
    capacity: AvailableComminutionCapacity,
    destination: StockpileId,
) -> Option<PlayerComminutionOption> {
    let AvailableComminutionCapacity {
        process,
        machine,
        store,
        maximum_mass,
    } = capacity;
    let stockpile = state.inventory().get_stockpile(destination)?;
    let headroom = if destination == stack.stockpile() {
        stockpile.available_capacity().checked_add(stack.mass())?
    } else {
        stockpile.available_capacity()
    };
    let mass = stack.mass().min(maximum_mass).min(headroom);
    if mass.is_zero() {
        return None;
    }
    let selections = stack
        .selections_for_mass(state, stack.stockpile(), mass)
        .ok()?;
    let resolved = resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(process, stack.stockpile(), &selections, machine, store),
    )
    .ok()?;
    let deferred_until_boundary = match validate_start_player_process(
        registries,
        state,
        resolved.process_resolution(),
        stack.stockpile(),
        destination,
    ) {
        Ok(_) => false,
        Err(StartPlayerProcessError::Process(StartProcessError::SimulationBoundary(_))) => true,
        Err(_) => return None,
    };
    Some(PlayerComminutionOption {
        stack: stack.clone(),
        process,
        equipment: machine,
        store,
        destination,
        maximum_mass: mass,
        duration: resolved.process_resolution().duration(),
        required_energy: resolved.required_energy(),
        deferred_until_boundary,
    })
}

fn available_capacities_for_form(
    registries: &Registries,
    state: &AppState,
    form: FormId,
    equipment: &[EquipmentId],
    stores: &[EnergyStoreId],
) -> Vec<AvailableComminutionCapacity> {
    let mut capacities = Vec::new();
    for definition in registries.ore_processing().comminution_definitions() {
        if form != definition.input_form() {
            continue;
        }
        for &machine in equipment {
            for &store in stores {
                let Some(envelope) = assess_powered_ore_mass_envelope(
                    registries,
                    state,
                    definition.process(),
                    machine,
                    store,
                )
                .ok() else {
                    continue;
                };
                let capacity = AvailableComminutionCapacity {
                    process: definition.process(),
                    machine,
                    store,
                    maximum_mass: envelope.maximum_mass(),
                };
                if !capacity.maximum_mass.is_zero() {
                    capacities.push(capacity);
                }
            }
        }
    }
    capacities
}

/// Resolves the chosen ore quantity anew and returns the ordinary, revision-
/// bound production start. The caller never manipulates inventory lot IDs.
pub fn validate_start_player_comminution_option(
    registries: &Registries,
    state: &AppState,
    option: &PlayerComminutionOption,
    mass: Mass,
) -> Result<ValidatedStartProcess, PlayerComminutionOptionError> {
    // An observed menu row is not an authorization. Recheck the living actor
    // before exposing its quantity limit or the private inventory revision.
    validate_player_action(state).map_err(|error| {
        PlayerComminutionOptionError::Start(match error {
            PlayerActionValidationError::PlayerNotAdmitted => {
                StartPlayerProcessError::PlayerNotAdmitted
            }
            PlayerActionValidationError::PlayerDead => StartPlayerProcessError::PlayerDead,
        })
    })?;
    if mass.is_zero() {
        return Err(PlayerComminutionOptionError::ZeroMass);
    }
    if mass > option.maximum_mass {
        return Err(PlayerComminutionOptionError::AboveCurrentMaximum {
            requested: mass,
            maximum: option.maximum_mass,
        });
    }
    let selections = option
        .stack
        .selections_for_mass(state, option.source(), mass)
        .map_err(PlayerComminutionOptionError::Stack)?;
    let resolved = resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            option.process,
            option.source(),
            &selections,
            option.equipment,
            option.store,
        ),
    )
    .map_err(PlayerComminutionOptionError::Comminution)?;
    validate_start_player_process(
        registries,
        state,
        resolved.process_resolution(),
        option.source(),
        option.destination,
    )
    .map_err(PlayerComminutionOptionError::Start)
}

#[cfg(test)]
#[path = "player_comminution_tests.rs"]
mod tests;
