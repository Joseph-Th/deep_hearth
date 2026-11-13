//! Convert canonical direct-control receipts into actionable player feedback.
//! No receipt is used to decide legality or mutate state.

use deep_hearth::combat::{
    PrimitiveAttackError, PrimitiveAttackOutcome, PrimitivePreparationOutcome,
};
use deep_hearth::labor::PlayerWorkStartError;
use deep_hearth::simulation::{
    ActiveSelectionDropOutcome, PrimaryActionOutcome, QuickAccessSelectionOutcome,
    SecondaryActionOutcome,
};
use deep_hearth::world::{BlockExcavationError, TreeFellingError, TreeFellingStartError};
use deep_hearth::{AppState, Registries};

use super::super::{fire, storage, view};

pub(super) fn slot(
    registries: &Registries,
    state: &AppState,
    outcome: &QuickAccessSelectionOutcome,
) -> String {
    match outcome {
        QuickAccessSelectionOutcome::Selected { slot, selection } => format!(
            "{} ({})",
            slot.index() + 1,
            view::hand_label(registries, state, *selection)
        ),
        QuickAccessSelectionOutcome::Rejected(error) => {
            format!("rejected: {error}")
        }
    }
}

pub(super) fn primary(outcome: &PrimaryActionOutcome) -> String {
    match outcome {
        PrimaryActionOutcome::PlayerNotAdmitted => "No active player.".to_owned(),
        PrimaryActionOutcome::PlayerDead => "Cannot act while dead. Use 'respawn'.".to_owned(),
        PrimaryActionOutcome::Shove(contact) => contact_label("Shove", contact),
        PrimaryActionOutcome::Strike(contact) => contact_label("Strike", contact),
        PrimaryActionOutcome::PreparedStrike(contact) => contact_label("Prepared strike", contact),
        PrimaryActionOutcome::ShoveRejected(error)
        | PrimaryActionOutcome::StrikeRejected(error)
        | PrimaryActionOutcome::PreparationRejected(error)
        | PrimaryActionOutcome::PreparedStrikeRejected(error) => combat_blocker(*error),
        PrimaryActionOutcome::Preparation(PrimitivePreparationOutcome::Prepared { ready_at }) => {
            format!(
                "Strike prepared; ready at control tick {}.",
                ready_at.value()
            )
        }
        PrimaryActionOutcome::Preparation(PrimitivePreparationOutcome::Cancelled) => {
            "Prepared strike cancelled.".to_owned()
        }
        PrimaryActionOutcome::BlockExcavationStarted(_) => {
            "Block breaking started; work will consume time.".to_owned()
        }
        PrimaryActionOutcome::BlockExcavationRejected(BlockExcavationError::NoActiveEquipment) => {
            "Block breaking requires a carried tool selected in the hotbar.".to_owned()
        }
        PrimaryActionOutcome::BlockExcavationRejected(
            BlockExcavationError::ActiveSelectionNotEquipment { .. },
        ) => "Block breaking requires a tool, not held material.".to_owned(),
        PrimaryActionOutcome::BlockExcavationRejected(error) => error.to_string(),
        PrimaryActionOutcome::TreeFellingStarted(_) => "Tree felling started.".to_owned(),
        PrimaryActionOutcome::TreeFellingRejected(error) => tree_felling_blocker(error),
        PrimaryActionOutcome::StructuralDismantlingStarted(_) => {
            "Structural dismantling started.".to_owned()
        }
        PrimaryActionOutcome::StructuralDismantlingRejected(error) => error.to_string(),
        PrimaryActionOutcome::AmbiguousStructuralTarget { count } => {
            format!("{count} structures overlap the target. Aim more precisely.")
        }
        PrimaryActionOutcome::QueuedUntilSimulationBoundary => {
            "Action queued for the next simulation boundary.".to_owned()
        }
        PrimaryActionOutcome::BufferedContextChanged => {
            "The target or held item changed before the queued action; try again.".to_owned()
        }
        PrimaryActionOutcome::BufferRejected(error) => error.to_string(),
    }
}

fn tree_felling_blocker(error: &TreeFellingStartError) -> String {
    use TreeFellingError as Tree;
    use TreeFellingStartError as Start;
    match error {
        Start::NoActiveChoppingTool => "Select a carried chopping tool before felling a tree.",
        Start::UnsupportedChoppingTool => "The selected tool cannot fell a tree.",
        Start::ChoppingToolUnavailable => "The chopping tool is unavailable.",
        Start::ChoppingToolTooWorn => "The chopping tool needs repair or replacement.",
        Start::EquipmentRevisionExhausted => "The tool cannot reserve further wear.",
        Start::CompletionTickOverflow => "The felling work cannot be scheduled.",
        Start::Work(error) => return work_blocker(error),
        Start::Tree(Tree::PlayerNotAdmitted) => "No active player.",
        Start::Tree(Tree::PlayerDead) => "Cannot fell trees while dead.",
        Start::Tree(Tree::PlayerBusy) => "Finish the current hand work first.",
        Start::Tree(Tree::TreeUnavailable { .. }) => "No standing tree at that target.",
        Start::Tree(Tree::TooFar { .. }) => "Move next to the tree before felling it.",
        Start::Tree(Tree::WorldRevisionExhausted) => "World state cannot admit the felling.",
        Start::Tree(Tree::AtmosphericInputOverflow) => "Tree energy accounting exceeds capacity.",
        Start::Tree(Tree::SurfaceIdExhausted | Tree::SurfaceRevisionExhausted) => {
            "The ground cannot admit another wood parcel."
        }
        Start::Tree(Tree::SurfacePlacement(_)) => "The felled wood has no valid ground landing.",
    }
    .to_owned()
}

fn work_blocker(error: &PlayerWorkStartError) -> String {
    use PlayerWorkStartError as Work;
    match error {
        Work::SimulationBoundary(_) => {
            "Work must start on a simulation boundary. Use 'wait boundary'."
        }
        Work::SurvivalNotInitialized => "The player has no initialized survival state.",
        Work::PlayerDead => "Cannot start work while dead.",
        Work::PlayerAirborne => "Stand on fixed ground before starting work.",
        Work::PlayerMoving => "Stop moving before starting work; use 'wait 10' to settle.",
        Work::PreparedStrikeActive => "Cancel the prepared strike before starting work.",
        Work::Busy { .. } => "Finish or cancel the current task before starting another.",
        Work::MetabolicCostOverflow { .. } | Work::HydrationCostOverflow { .. } => {
            "The requested work exceeds the supported exertion range."
        }
        Work::InsufficientMetabolicEnergy { .. } => {
            "Not enough metabolic energy to complete the requested work."
        }
        Work::InsufficientHydration { .. } => {
            "Not enough hydration to complete the requested work."
        }
        Work::RevisionExhausted | Work::SurvivalRevisionExhausted { .. } => {
            "The work cannot reserve another state change."
        }
    }
    .to_owned()
}

fn contact_label(action: &str, outcome: &PrimitiveAttackOutcome) -> String {
    let result = match outcome {
        PrimitiveAttackOutcome::Miss => "missed",
        PrimitiveAttackOutcome::Startled { .. } => "startled a resident",
        PrimitiveAttackOutcome::Staggered { .. } => "staggered a resident",
    };
    format!("{action} {result}.")
}

fn combat_blocker(error: PrimitiveAttackError) -> String {
    use PrimitiveAttackError as Error;
    match error {
        Error::PlayerNotAdmitted => "No active player.",
        Error::PlayerDead => "Cannot attack while dead.",
        Error::PlayerBusy | Error::OccupiedHand => "Finish current hand work first.",
        Error::NoHeldWeapon => "Select a carried weapon first.",
        Error::WeaponNotCarried => "The selected weapon is not carried.",
        Error::UnsupportedWeapon => "The held tool cannot be used as a weapon.",
        Error::BrokenWeapon => "The held weapon is broken.",
        Error::WeaponBusy => "The selected weapon is occupied by another task.",
        Error::AlreadyPreparing => "A strike is already being prepared.",
        Error::NotPreparing => "No prepared strike is active.",
        Error::PreparationNotReady { .. } => "The prepared strike is not ready yet.",
        Error::PreparationWeaponChanged => "The prepared weapon has changed.",
        Error::PreparationAimChanged => "The prepared strike no longer faces the same direction.",
        Error::EquipmentRevisionExhausted => "The weapon cannot reserve more condition changes.",
        Error::InsufficientMetabolicEnergy => "Not enough energy to attack.",
        Error::InsufficientHydration => "Not enough hydration to attack.",
        Error::ExertionOverflow => "The attack would exceed physical exertion capacity.",
        Error::SurvivalRevisionExhausted => "The survival state cannot admit this attack.",
        Error::Recovering { .. } => "Still recovering from the last attack.",
        Error::RevisionExhausted => "The attack cannot reserve its combat state change.",
        Error::Target(_) => "The target is not reachable with the current aim.",
        Error::AmbiguousContact { .. } => "More than one resident occupies the contact point.",
    }
    .to_owned()
}

pub(super) fn secondary(registries: &Registries, outcome: &SecondaryActionOutcome) -> String {
    use SecondaryActionOutcome as Action;
    match outcome {
        Action::EatingStarted(_) => "Eating started.".to_owned(),
        Action::DrinkingStarted(_) => "Drinking started.".to_owned(),
        Action::SurfaceGatheringStarted(_) => "Gathering started.".to_owned(),
        Action::EnergyStoreAssembled { definition, .. } => format!(
            "Built {}.",
            registries
                .energy()
                .get_store(*definition)
                .map_or("energy store", |record| record.name())
        ),
        Action::EnergyStoreAssemblyRejected { error, .. } => {
            fire::assembly_blocker(registries, error.clone())
        }
        Action::CombustionFuelAdded { released, .. } => {
            format!("Added {} of fuel energy.", fire::energy_label(*released))
        }
        Action::CombustionFuelRejected { error, .. } => error.to_string(),
        Action::StructuralConstructionStarted(_) => "Structural construction started.".to_owned(),
        Action::StoragePlaced { definition, .. } => format!(
            "Placed {}.",
            registries
                .storage()
                .get(*definition)
                .map_or("storage enclosure", |record| record.name())
        ),
        Action::StoragePlacementRejected { error, .. } => {
            storage::placement_blocker(registries, error.clone())
        }
        Action::StorageTargetRejected(error) | Action::StructuralTargetRejected(error) => {
            error.to_string()
        }
        Action::NoStoragePlacementTarget => "No storage placement target under the aim.".to_owned(),
        Action::StoragePlacementCoordinateOverflow
        | Action::StructuralPlacementCoordinateOverflow => {
            "The target lies outside the supported world coordinates.".to_owned()
        }
        Action::StoragePlacementWorldUnavailable { .. }
        | Action::StructuralPlacementWorldUnavailable { .. } => {
            "The target terrain is not available.".to_owned()
        }
        Action::StoragePlacementOccupied { .. }
        | Action::StoragePlacementLocationOccupied { .. }
        | Action::StructuralPlacementOccupied { .. }
        | Action::StructuralPlacementLocationOccupied { .. } => {
            "The target location is occupied.".to_owned()
        }
        Action::StoragePlacementIntersectsPlayer { .. }
        | Action::StructuralPlacementIntersectsPlayer { .. } => {
            "The construction would intersect the player.".to_owned()
        }
        Action::StoragePlacementUnsupported { .. } => {
            "Storage needs stable ground support.".to_owned()
        }
        Action::NoStructuralPlacementTarget => {
            "No structural placement target under the aim.".to_owned()
        }
        Action::StructuralPlacementRequiresTopFace { .. } => {
            "Aim at the top face of the supporting block.".to_owned()
        }
        Action::AmbiguousStructuralPlacementTarget { count } => {
            format!("{count} structures overlap the placement target. Aim more precisely.")
        }
        Action::StructuralPlacementRejected(error) => error.to_string(),
        Action::AlreadySatisfied => "The target is already complete.".to_owned(),
        Action::QueuedUntilSimulationBoundary | Action::DeferredUntilSimulationBoundary => {
            "Action queued for the next simulation boundary.".to_owned()
        }
        Action::BufferedContextChanged => {
            "The target or held item changed before the queued use; try again.".to_owned()
        }
        Action::BufferRejected(error) => error.to_string(),
        Action::PlayerNotAdmitted => "No active player.".to_owned(),
        Action::PlayerDead => "Cannot use items while dead. Use 'respawn'.".to_owned(),
        Action::NoActiveSelection => {
            "Nothing usable is selected here. Inspect 'gather' or 'drink' for local options."
                .to_owned()
        }
        Action::ActiveEquipmentNotContextual => {
            "The held equipment has no contextual use at this target.".to_owned()
        }
        Action::MaterialUnavailable => {
            "The selected material is no longer in carried inventory.".to_owned()
        }
        Action::AmbiguousMaterialStacks { count } => {
            format!("{count} material stacks match. Choose one in 'inventory'.")
        }
        Action::AmbiguousStorageDefinitions { count } => {
            format!("{count} storage choices match. Use 'storage' and 'place-store'.")
        }
        Action::AmbiguousLocalCombustionStores { count } => {
            format!("{count} local fires match. Use 'fire' and 'feed-fire'.")
        }
        Action::AmbiguousCombustionDefinitions { count } => {
            format!("{count} fire construction choices match. Use 'fire'.")
        }
        Action::AmbiguousLocalDrinkSources { count } => {
            format!("{count} local drink sources match. Use 'drink' to choose one.")
        }
        Action::AmbiguousLocalSurfaceResources { count } => {
            format!("{count} local gathering sources match. Use 'gather' to choose one.")
        }
        Action::AmbiguousSurfaceGatheringMethods { count } => {
            format!("{count} gathering methods match. Use 'gather' to choose one.")
        }
        Action::AmbiguousEmptyHandContext {
            drink_sources,
            surface_resources,
        } => format!(
            "{drink_sources} drink sources and {surface_resources} gathering sources are available. Use 'drink' or 'gather'."
        ),
        Action::MaterialNotContextual => "The held material has no contextual use here.".to_owned(),
        Action::EatingRejected(error) => error.to_string(),
        Action::DrinkingRejected(error) => error.to_string(),
        Action::SurfaceGatheringRejected(error) => error.to_string(),
    }
}

pub(super) fn drop(registries: &Registries, outcome: &ActiveSelectionDropOutcome) -> String {
    match outcome {
        ActiveSelectionDropOutcome::PlayerNotAdmitted => "No active player.".to_owned(),
        ActiveSelectionDropOutcome::PlayerDead => "Cannot drop while dead.".to_owned(),
        ActiveSelectionDropOutcome::EquipmentDropped { position, .. } => format!(
            "Dropped held equipment at ({},{},{}).",
            position.x(),
            position.y(),
            position.z()
        ),
        ActiveSelectionDropOutcome::MaterialDropped(record) => format!(
            "Dropped {} g {} at ({},{},{}).",
            view::grams(record.dropped_mass()),
            registries
                .materials()
                .commodity_name(record.commodity())
                .unwrap_or("unidentified material"),
            record.position().x(),
            record.position().y(),
            record.position().z()
        ),
        ActiveSelectionDropOutcome::NoActiveSelection => "Nothing is selected to drop.".to_owned(),
        ActiveSelectionDropOutcome::EquipmentRejected(error) => error.to_string(),
        ActiveSelectionDropOutcome::MaterialRejected(error) => error.to_string(),
    }
}
