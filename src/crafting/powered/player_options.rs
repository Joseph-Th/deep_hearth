//! Exact-local workshop choices derived from authored transforms and physical machines.

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU64;

use crate::core::quantity::{Energy, Mass};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;
use crate::logistics::{PlayerStockpileAccessError, validate_player_stockpile_access};
use crate::production::{ProcessId, ValidatedStartProcess};
use crate::registry::Registries;

use super::{
    PoweredCraftRequest, StartPoweredCraftError, project_powered_craft_equipment_work,
    validate_start_powered_craft,
};
use crate::crafting::{
    ManualCraftInputPlanError, assess_manual_craft_inputs, plan_manual_craft_from_stockpile,
};

/// One currently feasible machine-and-energy pairing for a familiar material transformation.
///
/// A row distinguishes available material and stored charge. It never claims that a future
/// start will succeed: output capacity, occupancy, survival and revisions remain live checks.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerPoweredCraftOption {
    transform: ProcessId,
    process: ProcessId,
    source: StockpileId,
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
    batch_mass: Mass,
    available_batches: u64,
    required_energy: Energy,
    duration: TickSpan,
    charge_ready: bool,
}

impl PlayerPoweredCraftOption {
    #[must_use]
    pub const fn transform(self) -> ProcessId {
        self.transform
    }
    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }
    #[must_use]
    pub const fn source(self) -> StockpileId {
        self.source
    }
    #[must_use]
    pub const fn equipment(self) -> EquipmentId {
        self.equipment
    }
    #[must_use]
    pub const fn energy_store(self) -> EnergyStoreId {
        self.energy_store
    }
    #[must_use]
    pub const fn batch_mass(self) -> Mass {
        self.batch_mass
    }
    #[must_use]
    pub const fn available_batches(self) -> u64 {
        self.available_batches
    }
    #[must_use]
    pub const fn required_energy(self) -> Energy {
        self.required_energy
    }
    #[must_use]
    pub const fn duration(self) -> TickSpan {
        self.duration
    }
    #[must_use]
    pub const fn charge_ready(self) -> bool {
        self.charge_ready
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerPoweredCraftOptionError {
    PlayerNotAdmitted,
    StockpileAccess(PlayerStockpileAccessError),
    Input(ManualCraftInputPlanError),
    Start(StartPoweredCraftError),
}

impl Display for PlayerPoweredCraftOptionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => f.write_str("player is not admitted"),
            Self::StockpileAccess(error) => write!(f, "powered workshop stockpile access: {error}"),
            Self::Input(error) => write!(f, "powered workshop input: {error}"),
            Self::Start(error) => write!(f, "powered workshop start: {error}"),
        }
    }
}

impl Error for PlayerPoweredCraftOptionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PlayerNotAdmitted => None,
            Self::StockpileAccess(error) => Some(error),
            Self::Input(error) => Some(error),
            Self::Start(error) => Some(error),
        }
    }
}

/// Discovers executable one-batch machine variants from a manual transform and local custody.
///
/// A machine or energy source that cannot physically support the work is omitted, while missing
/// input and missing charge remain visible decisions on otherwise feasible pairings. The
/// projected duration uses the ordinary powered-craft physics, including machine wear.
pub fn player_powered_craft_options(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
    transform: ProcessId,
) -> Result<Vec<PlayerPoweredCraftOption>, PlayerPoweredCraftOptionError> {
    if state.logistics().player().is_none() {
        return Err(PlayerPoweredCraftOptionError::PlayerNotAdmitted);
    }
    validate_player_stockpile_access(state, source)
        .map_err(PlayerPoweredCraftOptionError::StockpileAccess)?;
    let input = assess_manual_craft_inputs(registries, state, transform, source)
        .map_err(PlayerPoweredCraftOptionError::Input)?;
    let mut rows = Vec::new();
    for variant in registries.crafting().powered_variants(transform) {
        for equipment in state.available_local_equipment() {
            for store in state.available_local_energy_stores() {
                let Ok(projection) = project_powered_craft_equipment_work(
                    registries,
                    state,
                    variant.process(),
                    input.batch_mass(),
                    equipment.definition(),
                    equipment.condition(),
                    store.id(),
                ) else {
                    continue;
                };
                rows.push(PlayerPoweredCraftOption {
                    transform,
                    process: variant.process(),
                    source,
                    equipment: equipment.id(),
                    energy_store: store.id(),
                    batch_mass: input.batch_mass(),
                    available_batches: input.maximum_batches(),
                    required_energy: projection.required_energy(),
                    duration: projection.duration(),
                    charge_ready: store.stored() >= projection.required_energy(),
                });
            }
        }
    }
    rows.sort_by_key(|row| (row.process, row.equipment, row.energy_store));
    Ok(rows)
}

/// Applies a formerly observed choice through canonical material selection and powered-work
/// admission. The row is not an authorization token and can become stale or unaffordable.
pub fn validate_start_player_powered_craft_option(
    registries: &Registries,
    state: &AppState,
    option: PlayerPoweredCraftOption,
    destination: StockpileId,
) -> Result<ValidatedStartProcess, PlayerPoweredCraftOptionError> {
    validate_player_stockpile_access(state, option.source)
        .map_err(PlayerPoweredCraftOptionError::StockpileAccess)?;
    let craft = plan_manual_craft_from_stockpile(
        registries,
        state,
        option.transform,
        option.source,
        NonZeroU64::new(1).unwrap_or_else(|| unreachable!()),
    )
    .map_err(PlayerPoweredCraftOptionError::Input)?;
    validate_start_powered_craft(
        registries,
        state,
        PoweredCraftRequest::new(
            option.process,
            option.source,
            craft.selections().to_vec(),
            option.equipment,
            option.energy_store,
        ),
        destination,
    )
    .map_err(PlayerPoweredCraftOptionError::Start)
}
