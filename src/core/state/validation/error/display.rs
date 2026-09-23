//! Human-readable diagnostics for root-state validation failures.

use std::fmt::{Display, Formatter};

use crate::structural::StructuralElementId;

use super::StateValidationError;

impl Display for StateValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Energy(error) => write!(formatter, "invalid energy state: {error}"),
            Self::Fluid(error) => write!(formatter, "invalid fluid state: {error}"),
            Self::Equipment(error) => write!(formatter, "invalid equipment state: {error}"),
            Self::Structure(error) => write!(formatter, "invalid structural state: {error}"),
            Self::StructureAnalysis(error) => {
                write!(formatter, "structural state cannot be analyzed: {error}")
            }
            Self::UnresolvedStructuralDamage { event } => write!(
                formatter,
                "structural element {} has unresolved canonical damage",
                event.element().value()
            ),
            Self::Geology(error) => write!(formatter, "invalid geology state: {error}"),
            Self::GeologicalKnowledge(error) => {
                write!(formatter, "invalid geological knowledge state: {error}")
            }
            Self::Inventory(error) => write!(formatter, "invalid inventory state: {error}"),
            Self::StorageEnclosure(error) => {
                write!(formatter, "invalid storage enclosure state: {error}")
            }
            Self::Production(error) => write!(formatter, "invalid production state: {error}"),
            Self::Mining(error) => write!(formatter, "invalid mining state: {error}"),
            Self::MiningJob(error) => write!(formatter, "invalid mining job: {error}"),
            Self::PlayerWork(error) => write!(formatter, "invalid player-work state: {error}"),
            Self::Survival(error) => write!(formatter, "invalid survival state: {error}"),
            Self::UnknownStoredCommodity {
                stockpile,
                commodity,
            } => write!(
                formatter,
                "stockpile {} references unknown material {} or form {}",
                stockpile.value(),
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::LotCreatedInFuture {
                lot,
                created_at,
                current,
            } => write!(
                formatter,
                "material lot {} was created at tick {} after current tick {}",
                lot.value(),
                created_at.value(),
                current.value()
            ),
            Self::LotProvenanceInFuture {
                lot,
                latest_created_at,
                current,
            } => write!(
                formatter,
                "material lot {} contains provenance through tick {} after current tick {}",
                lot.value(),
                latest_created_at.value(),
                current.value()
            ),
            Self::UnknownLotCompositionMaterial { lot, material } => write!(
                formatter,
                "material lot {} composition references unknown material {}",
                lot.value(),
                material.value()
            ),
            Self::UnknownJobConsumedCommodity { job, commodity } => write!(
                formatter,
                "production job {} consumed unknown material {} or form {}",
                job.value(),
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::InvalidJobConsumedParticleSizeState { job, error } => write!(
                formatter,
                "production job {} consumed invalid particle-size state: {error}",
                job.value()
            ),
            Self::InvalidJobConsumedPhaseState { job, error } => write!(
                formatter,
                "production job {} consumed invalid material phase state: {error}",
                job.value()
            ),
            Self::UnknownJobProcess { job, process } => write!(
                formatter,
                "production job {} references unknown process {}",
                job.value(),
                process.value()
            ),
            Self::MissingJobProcessTopology { job, process } => write!(
                formatter,
                "production job {} process {} has no execution topology",
                job.value(),
                process.value()
            ),
            Self::UnknownJobSource { job, stockpile } => write!(
                formatter,
                "production job {} references missing source stockpile {}",
                job.value(),
                stockpile.value()
            ),
            Self::JobEnergyTopologyMismatch { job, process } => write!(
                formatter,
                "production job {} energy resources do not match process {} execution topology",
                job.value(),
                process.value()
            ),
            Self::JobEquipmentTopologyMismatch { job, process } => write!(
                formatter,
                "production job {} equipment resources do not match process {} execution topology",
                job.value(),
                process.value()
            ),
            Self::UnknownJobDestination { job, stockpile } => write!(
                formatter,
                "production job {} references missing destination stockpile {}",
                job.value(),
                stockpile.value()
            ),
            Self::UnknownJobEnergySource { job, store } => write!(
                formatter,
                "production job {} traces missing energy store {}",
                job.value(),
                store.value()
            ),
            Self::JobEnergyDefinitionMismatch {
                job,
                traced,
                stored,
            } => write!(
                formatter,
                "production job {} traces energy definition {} but source store references {}",
                job.value(),
                traced.value(),
                stored.value()
            ),
            Self::JobEnergyCarrierMismatch {
                job,
                traced,
                authored,
            } => write!(
                formatter,
                "production job {} traces {traced:?} energy but source definition is {authored:?}",
                job.value()
            ),
            Self::UnknownJobEnergySink { job, store } => write!(
                formatter,
                "production job {} traces missing released-energy sink {}",
                job.value(),
                store.value()
            ),
            Self::JobReleasedEnergyDefinitionMismatch {
                job,
                traced,
                stored,
            } => write!(
                formatter,
                "production job {} traces released-energy definition {} but sink store references {}",
                job.value(),
                traced.value(),
                stored.value()
            ),
            Self::JobReleasedEnergyCarrierMismatch {
                job,
                traced,
                authored,
            } => write!(
                formatter,
                "production job {} traces released {traced:?} energy but sink definition is {authored:?}",
                job.value()
            ),
            Self::JobReleasedEnergySinkHasNoInputPower { job, store } => write!(
                formatter,
                "production job {} reserves energy sink {} whose definition accepts no input power",
                job.value(),
                store.value()
            ),
            Self::JobReleasedEnergyCapacityOverflow { job, store } => write!(
                formatter,
                "production job {} released-energy reservation overflows sink {} accounting",
                job.value(),
                store.value()
            ),
            Self::JobReleasedEnergyCapacityExceeded {
                job,
                store,
                stored,
                released,
                capacity,
            } => write!(
                formatter,
                "production job {} reserves {} nJ into sink {} containing {} nJ above capacity {} nJ",
                job.value(),
                released.nanojoules(),
                store.value(),
                stored.nanojoules(),
                capacity.nanojoules()
            ),
            Self::UnknownJobEquipment { job, equipment } => write!(
                formatter,
                "production job {} references missing equipment {}",
                job.value(),
                equipment.value()
            ),
            Self::JobEquipmentDefinitionMismatch {
                job,
                traced,
                stored,
            } => write!(
                formatter,
                "production job {} traces equipment definition {} but provider record references {}",
                job.value(),
                traced.value(),
                stored.value()
            ),
            Self::JobEquipmentConditionMismatch {
                job,
                traced,
                stored,
            } => write!(
                formatter,
                "production job {} traces equipment condition {} ppm but provider record is {} ppm",
                job.value(),
                traced.parts_per_million(),
                stored.parts_per_million()
            ),
            Self::JobEquipmentSupportRequirementMissing {
                job,
                equipment,
                definition,
            } => write!(
                formatter,
                "production job {} uses structurally installed equipment {} definition {} but does not preserve its active-support requirement",
                job.value(),
                equipment.value(),
                definition.value()
            ),
            Self::JobEquipmentSupportStateMismatch {
                job,
                equipment,
                requires_active_support,
                supported_by,
            } => write!(
                formatter,
                "running production job {} stores requires_active_support={} for equipment {} but current support is {:?}",
                job.value(),
                requires_active_support,
                equipment.value(),
                supported_by.map(StructuralElementId::value)
            ),
            Self::UnknownEquipmentSupport { equipment, element } => write!(
                formatter,
                "equipment {} references missing structural support element {}",
                equipment.value(),
                element.value()
            ),
            Self::EquipmentSupportedByPlannedElement { equipment, element } => write!(
                formatter,
                "equipment {} is assigned to planned structural element {} before activation",
                equipment.value(),
                element.value()
            ),
            Self::MountedEquipmentMassOverflow { element } => write!(
                formatter,
                "mounted equipment mass overflows aggregate accounting on structural element {}",
                element.value()
            ),
            Self::MountedEquipmentWeightOverflow { element } => write!(
                formatter,
                "mounted equipment weight exceeds structural force range on element {}",
                element.value()
            ),
            Self::EquipmentStructuralLoadMismatch {
                element,
                stored,
                expected,
            } => write!(
                formatter,
                "structural element {} stores {} mN equipment load but mounted equipment requires {} mN",
                element.value(),
                stored.millinewtons(),
                expected.millinewtons()
            ),
            Self::UnknownFluidSupport { store, element } => write!(
                formatter,
                "fluid store {} references missing structural support element {}",
                store.value(),
                element.value()
            ),
            Self::FluidSupportedByPlannedElement { store, element } => write!(
                formatter,
                "fluid store {} is assigned to planned structural element {} before activation",
                store.value(),
                element.value()
            ),
            Self::FluidStructuralLoad(error) => {
                write!(
                    formatter,
                    "invalid supported-fluid structural load: {error}"
                )
            }
            Self::UnknownStockpileSupport { stockpile, element } => write!(
                formatter,
                "stockpile {} references missing structural support element {}",
                stockpile.value(),
                element.value()
            ),
            Self::StockpileSupportedByPlannedElement { stockpile, element } => write!(
                formatter,
                "stockpile {} is assigned to planned structural element {} before activation",
                stockpile.value(),
                element.value()
            ),
            Self::StoredMatterMassOverflow { element } => write!(
                formatter,
                "stored matter mass overflows aggregate accounting on structural element {}",
                element.value()
            ),
            Self::StoredMatterWeightOverflow { element } => write!(
                formatter,
                "stored matter weight exceeds structural force range on element {}",
                element.value()
            ),
            Self::StoredMatterStructuralLoadMismatch {
                element,
                stored,
                expected,
            } => write!(
                formatter,
                "structural element {} stores {} mN stored-matter load but supported stockpiles require {} mN",
                element.value(),
                stored.millinewtons(),
                expected.millinewtons()
            ),
            Self::ComminutionJob(error) => {
                write!(formatter, "invalid comminution production job: {error}")
            }
            Self::ConstituentSeparationJob(error) => {
                write!(
                    formatter,
                    "invalid constituent-separation production job: {error}"
                )
            }
            Self::ScreeningJob(error) => {
                write!(formatter, "invalid screening production job: {error}")
            }
            Self::ThermalJob(error) => write!(formatter, "invalid thermal production job: {error}"),
            Self::CraftingJob(error) => {
                write!(formatter, "invalid crafting production job: {error}")
            }
            Self::NonManualJobSuspendedForPlayerLabor { job, process } => write!(
                formatter,
                "production job {} for non-manual process {} claims player-labor suspension",
                job.value(),
                process.value()
            ),
            Self::ReservedMassOverflow { stockpile } => write!(
                formatter,
                "expected inbound reservations overflow stockpile {} mass accounting",
                stockpile.value()
            ),
            Self::UnknownJobOutputCommodity { job, commodity } => write!(
                formatter,
                "production job {} promises unknown material {} or form {}",
                job.value(),
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::JobOutputStorage { job, error } => write!(
                formatter,
                "production job {} reserved output is incompatible with its destination: {error}",
                job.value()
            ),
            Self::JobOutputMassOverflow { job } => write!(
                formatter,
                "production job {} output mass overflows authoritative quantity storage",
                job.value()
            ),
            Self::ProductionInventoryRevisionCapacityExhausted {
                revision,
                completion_buckets,
            } => write!(
                formatter,
                "inventory revision {revision} cannot reserve {completion_buckets} scheduled production completion revisions"
            ),
            Self::ProductionEquipmentRevisionCapacityExhausted {
                revision,
                completion_buckets,
            } => write!(
                formatter,
                "equipment revision {revision} cannot reserve {completion_buckets} scheduled production wear revisions"
            ),
            Self::ProductionEnergyRevisionCapacityExhausted {
                revision,
                completion_buckets,
            } => write!(
                formatter,
                "energy revision {revision} cannot reserve {completion_buckets} scheduled production release revisions"
            ),
            Self::ProductionStructureRevisionCapacityExhausted {
                revision,
                completion_buckets,
            } => write!(
                formatter,
                "structural revision {revision} cannot reserve {completion_buckets} scheduled supported-output revisions"
            ),
            Self::FutureMaterialLotIdCapacityExhausted {
                next_lot_id,
                required,
            } => write!(
                formatter,
                "material lot identity cursor {next_lot_id} cannot reserve {required} already-admitted future parcel identities"
            ),
            Self::FutureMaterialLotIdDemandOverflow => formatter.write_str(
                "already-admitted future material-lot identity demand exceeds the representable u64 range",
            ),
            Self::FutureInventoryRevisionCapacityExhausted { revision, required } => write!(
                formatter,
                "inventory revision {revision} cannot reserve {required} already-admitted future revisions"
            ),
            Self::FutureInventoryRevisionDemandOverflow => formatter.write_str(
                "already-admitted future inventory revision demand exceeds the representable u64 range",
            ),
            Self::FutureEnergyRevisionCapacityExhausted { revision, required } => write!(
                formatter,
                "energy revision {revision} cannot reserve {required} already-admitted future revisions"
            ),
            Self::FutureEnergyRevisionDemandOverflow => formatter.write_str(
                "already-admitted future energy revision demand exceeds the representable u64 range",
            ),
            Self::FutureEquipmentRevisionCapacityExhausted { revision, required } => write!(
                formatter,
                "equipment revision {revision} cannot reserve {required} already-admitted future revisions"
            ),
            Self::FutureEquipmentRevisionDemandOverflow => formatter.write_str(
                "already-admitted future equipment revision demand exceeds the representable u64 range",
            ),
            Self::FutureMiningRevisionCapacityExhausted { revision, required } => write!(
                formatter,
                "mining revision {revision} cannot reserve {required} already-admitted future revisions"
            ),
            Self::FutureMiningRevisionDemandOverflow => formatter.write_str(
                "already-admitted future mining revision demand exceeds the representable u64 range",
            ),
            Self::FutureStructureRevisionCapacityExhausted { revision, required } => write!(
                formatter,
                "structural revision {revision} cannot reserve {required} already-admitted future revisions"
            ),
            Self::FutureStructureRevisionDemandOverflow => formatter.write_str(
                "already-admitted future structural revision demand exceeds the representable u64 range",
            ),
            Self::ReservedInboundMismatch {
                stockpile,
                reserved,
                expected,
            } => write!(
                formatter,
                "stockpile {} reserves {} mg inbound but active jobs require {} mg",
                stockpile.value(),
                reserved.milligrams(),
                expected.milligrams()
            ),
        }
    }
}
