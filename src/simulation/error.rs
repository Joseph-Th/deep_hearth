//! Public failure surface for the canonical synchronous simulation tick.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::time::{SimulationTick, TickSpan};
use crate::inventory::{StockpileId, StockpileStructuralLoadError};
use crate::production::ProductionJobId;
use crate::structural::StructuralCommitError;

/// Failure returned before any mutation when a simulation tick cannot advance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TickError {
    /// The authoritative tick counter has reached its representable maximum.
    ClockExhausted { current: SimulationTick },
    /// Due output lots cannot be allocated without exhausting persistent lot identity space.
    MaterialLotIdExhausted,
    /// Inventory cannot advance its persisted revision for this tick's consequences.
    InventoryRevisionExhausted,
    /// Production cannot advance its persisted revision for this tick's consequences.
    ProductionRevisionExhausted,
    /// Equipment cannot advance its persisted revision for completed-operation wear.
    EquipmentRevisionExhausted,
    /// Energy storage cannot advance its persisted revision for this tick's ingress or passive loss.
    EnergyRevisionExhausted,
    /// Player survival cannot advance its persisted revision for this tick.
    SurvivalRevisionExhausted,
    /// Authored basal and work energy costs cannot be represented together.
    SurvivalEnergyCostOverflow,
    /// Authored basal and work hydration losses cannot be represented together.
    SurvivalHydrationCostOverflow,
    /// Exclusive player-work ownership cannot release at this tick.
    PlayerWorkRevisionExhausted,
    /// Geology cannot advance its persisted revision for a mining completion this tick.
    GeologyRevisionExhausted,
    /// Mining cannot advance its persisted scheduling revision for this tick.
    MiningRevisionExhausted,
    /// Direct player-powered generation cannot advance its energy owner revision this tick.
    ManualPowerEnergyRevisionExhausted,
    /// Direct player-powered generation cannot advance its equipment owner revision this tick.
    ManualPowerEquipmentRevisionExhausted,
    /// Field prospecting cannot allocate another persistent observation identity.
    GeologicalObservationIdExhausted,
    /// Field prospecting cannot advance acquired geological knowledge.
    GeologicalKnowledgeRevisionExhausted,
    /// A suspended operation cannot schedule its remaining active time within the world clock.
    ProductionResumeTickOverflow {
        job: ProductionJobId,
        current: SimulationTick,
        remaining: TickSpan,
    },
    /// Due output mass cannot be aggregated in its destination stockpile.
    DestinationMassOverflow { stockpile: StockpileId },
    /// In-flight material perishability exposure cannot be represented at completion.
    ProductionStorageAgeOverflow { job: ProductionJobId },
    /// Due output weight cannot be resolved against its structural support.
    StructuralLoad(StockpileStructuralLoadError),
    /// Inventory changed after completion planning and before commit.
    StaleInventoryRevision { expected: u64, actual: u64 },
    /// Production changed after completion planning and before commit.
    StaleProductionRevision { expected: u64, actual: u64 },
    /// Equipment changed after a wear-bearing completion was planned and before commit.
    StaleEquipmentRevision { expected: u64, actual: u64 },
    /// Energy storage changed after a released-energy completion was planned and before commit.
    StaleEnergyRevision { expected: u64, actual: u64 },
    /// Structure changed after a stored-matter load completion was planned and before commit.
    StaleStructureRevision { expected: u64, actual: u64 },
    /// Player-work ownership changed after manual production resumption was planned.
    StalePlayerWorkRevision { expected: u64, actual: u64 },
    /// Survival reserves changed after manual production resumption was planned.
    StaleSurvivalRevision { expected: u64, actual: u64 },
    /// A validated stored-matter structural consequence could not commit.
    Structure(StructuralCommitError),
}

impl Display for TickError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClockExhausted { current } => write!(
                formatter,
                "simulation clock exhausted at tick {}",
                current.value()
            ),
            Self::SurvivalRevisionExhausted => {
                formatter.write_str("survival state revision space is exhausted")
            }
            Self::SurvivalEnergyCostOverflow => {
                formatter.write_str("combined survival energy cost overflows authoritative storage")
            }
            Self::SurvivalHydrationCostOverflow => formatter
                .write_str("combined survival hydration loss overflows authoritative storage"),
            Self::PlayerWorkRevisionExhausted => {
                formatter.write_str("player-work revision space is exhausted")
            }
            Self::GeologyRevisionExhausted => {
                formatter.write_str("geology revision space is exhausted during mining completion")
            }
            Self::MiningRevisionExhausted => {
                formatter.write_str("mining revision space is exhausted")
            }
            Self::ManualPowerEnergyRevisionExhausted => {
                formatter.write_str("manual power energy revision space is exhausted")
            }
            Self::ManualPowerEquipmentRevisionExhausted => {
                formatter.write_str("manual power equipment revision space is exhausted")
            }
            Self::GeologicalObservationIdExhausted => {
                formatter.write_str("geological observation identifier space is exhausted")
            }
            Self::GeologicalKnowledgeRevisionExhausted => {
                formatter.write_str("geological knowledge revision space is exhausted")
            }
            Self::ProductionResumeTickOverflow {
                job,
                current,
                remaining,
            } => write!(
                formatter,
                "production job {} cannot resume {} active ticks from simulation tick {}",
                job.value(),
                remaining.value(),
                current.value()
            ),
            Self::MaterialLotIdExhausted => {
                formatter.write_str("material lot identifier space is exhausted")
            }
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::ProductionRevisionExhausted => {
                formatter.write_str("production revision space is exhausted")
            }
            Self::EquipmentRevisionExhausted => {
                formatter.write_str("equipment revision space is exhausted")
            }
            Self::EnergyRevisionExhausted => {
                formatter.write_str("energy revision space is exhausted")
            }
            Self::DestinationMassOverflow { stockpile } => write!(
                formatter,
                "due production output mass overflows stockpile {}",
                stockpile.value()
            ),
            Self::ProductionStorageAgeOverflow { job } => write!(
                formatter,
                "production job {} material storage exposure overflows at completion",
                job.value()
            ),
            Self::StructuralLoad(error) => {
                write!(
                    formatter,
                    "due production stored-matter load failed: {error}"
                )
            }
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "tick completion plan expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleProductionRevision { expected, actual } => write!(
                formatter,
                "tick completion plan expected production revision {expected} but current revision is {actual}"
            ),
            Self::StaleEquipmentRevision { expected, actual } => write!(
                formatter,
                "tick completion plan expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::StaleEnergyRevision { expected, actual } => write!(
                formatter,
                "tick completion plan expected energy revision {expected} but current revision is {actual}"
            ),
            Self::StaleStructureRevision { expected, actual } => write!(
                formatter,
                "tick completion plan expected structural revision {expected} but current revision is {actual}"
            ),
            Self::StalePlayerWorkRevision { expected, actual } => write!(
                formatter,
                "tick completion plan expected player-work revision {expected} but current revision is {actual}"
            ),
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "tick completion plan expected survival revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => {
                write!(
                    formatter,
                    "tick stored-matter structural commit failed: {error}"
                )
            }
        }
    }
}

impl Error for TickError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StructuralLoad(error) => Some(error),
            Self::Structure(error) => Some(error),
            Self::ClockExhausted { .. }
            | Self::ProductionResumeTickOverflow { .. }
            | Self::DestinationMassOverflow { .. }
            | Self::ProductionStorageAgeOverflow { .. }
            | Self::StaleInventoryRevision { .. }
            | Self::StaleProductionRevision { .. }
            | Self::StaleEquipmentRevision { .. }
            | Self::StaleEnergyRevision { .. }
            | Self::StaleStructureRevision { .. }
            | Self::StalePlayerWorkRevision { .. }
            | Self::StaleSurvivalRevision { .. }
            | Self::MaterialLotIdExhausted
            | Self::InventoryRevisionExhausted
            | Self::ProductionRevisionExhausted
            | Self::EquipmentRevisionExhausted
            | Self::EnergyRevisionExhausted
            | Self::SurvivalRevisionExhausted
            | Self::SurvivalEnergyCostOverflow
            | Self::SurvivalHydrationCostOverflow
            | Self::PlayerWorkRevisionExhausted
            | Self::GeologyRevisionExhausted
            | Self::MiningRevisionExhausted
            | Self::ManualPowerEnergyRevisionExhausted
            | Self::ManualPowerEquipmentRevisionExhausted
            | Self::GeologicalObservationIdExhausted
            | Self::GeologicalKnowledgeRevisionExhausted => None,
        }
    }
}
