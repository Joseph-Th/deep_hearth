//! Visible-stack portion aggregation into one exact canonical eating transaction.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    MaterialLotId, MaterialLotSelection, MaterialStackProjection, MaterialStackProjectionError,
};
use crate::logistics::validate_player_stockpile_access;
use crate::registry::Registries;

use super::super::{EatError, ValidatedEat, validate_eat};

#[must_use]
#[derive(Clone, Copy, Debug)]
pub struct FoodStackPortion<'a> {
    stack: &'a MaterialStackProjection,
    mass: Mass,
}

impl<'a> FoodStackPortion<'a> {
    pub const fn new(stack: &'a MaterialStackProjection, mass: Mass) -> Self {
        Self { stack, mass }
    }

    pub const fn stack(self) -> &'a MaterialStackProjection {
        self.stack
    }

    #[must_use]
    pub const fn mass(self) -> Mass {
        self.mass
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EatStackPortionsError {
    EmptyMeal,
    MixedSources {
        expected: crate::inventory::StockpileId,
        actual: crate::inventory::StockpileId,
    },
    Stack {
        portion: usize,
        error: MaterialStackProjectionError,
    },
    Eat(EatError),
}

impl Display for EatStackPortionsError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyMeal => {
                formatter.write_str("visible-stack meal requires at least one portion")
            }
            Self::MixedSources { expected, actual } => write!(
                formatter,
                "visible-stack meal mixes stockpiles {} and {}",
                expected.value(),
                actual.value()
            ),
            Self::Stack { portion, error } => {
                write!(formatter, "meal portion {portion} became stale: {error}")
            }
            Self::Eat(error) => write!(formatter, "visible-stack meal failed: {error}"),
        }
    }
}

impl Error for EatStackPortionsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Stack { error, .. } => Some(error),
            Self::Eat(error) => Some(error),
            Self::EmptyMeal | Self::MixedSources { .. } => None,
        }
    }
}

/// Validates one deliberate meal composed from visible physical stack portions.
///
/// Exact lot membership remains private to inventory. Every portion must come from the same
/// stockpile; canonical eating still owns freshness, intake limits, nutrition, timing, attention,
/// custody, and conservation.
pub fn validate_eat_stack_portions(
    registries: &Registries,
    state: &AppState,
    portions: &[FoodStackPortion<'_>],
) -> Result<ValidatedEat, EatStackPortionsError> {
    let source = portions
        .first()
        .map(|portion| portion.stack().stockpile())
        .ok_or(EatStackPortionsError::EmptyMeal)?;
    // A displayed stack is not proof of current player access. Reject remote
    // custody before inspecting sizes, freshness cohorts, or lot membership.
    validate_player_stockpile_access(state, source)
        .map_err(|error| EatStackPortionsError::Eat(EatError::Access(error)))?;
    // Multiple requested portions of the same projected stack must share one
    // allocation across its underlying lots. Resolving each portion separately
    // starts at the first lot every time and can overdraw it even when the
    // complete visible stack contains sufficient food.
    let mut grouped = Vec::<(usize, &MaterialStackProjection, Mass)>::new();
    for (index, portion) in portions.iter().copied().enumerate() {
        if portion.stack().stockpile() != source {
            return Err(EatStackPortionsError::MixedSources {
                expected: source,
                actual: portion.stack().stockpile(),
            });
        }
        // Validate every requested portion independently, including zero-mass and
        // stale projections, before adding it to an existing stack group.
        portion
            .stack()
            .validate_selection_mass(state, source, portion.mass())
            .map_err(|error| EatStackPortionsError::Stack {
                portion: index,
                error,
            })?;
        if let Some((last_index, _, requested)) = grouped
            .iter_mut()
            .find(|(_, stack, _)| *stack == portion.stack())
        {
            *requested =
                requested
                    .checked_add(portion.mass())
                    .ok_or(EatStackPortionsError::Eat(
                        EatError::InventoryMassOverflow { stockpile: source },
                    ))?;
            *last_index = index;
        } else {
            grouped.push((index, portion.stack(), portion.mass()));
        }
    }
    let mut mass_by_lot = BTreeMap::<MaterialLotId, u128>::new();
    for (index, stack, requested) in grouped {
        for selection in stack
            .selections_for_mass(state, source, requested)
            .map_err(|error| EatStackPortionsError::Stack {
                portion: index,
                error,
            })?
        {
            let total = mass_by_lot.entry(selection.lot()).or_default();
            *total = total
                .checked_add(u128::from(selection.mass().milligrams()))
                .ok_or(EatStackPortionsError::Eat(
                    EatError::InventoryMassOverflow { stockpile: source },
                ))?;
        }
    }
    let selections = mass_by_lot
        .into_iter()
        .map(|(lot, total)| {
            let mass = u64::try_from(total).map_err(|_| {
                EatStackPortionsError::Eat(EatError::InventoryMassOverflow { stockpile: source })
            })?;
            Ok(MaterialLotSelection::new(lot, Mass::from_milligrams(mass)))
        })
        .collect::<Result<Vec<_>, EatStackPortionsError>>()?;
    validate_eat(registries, state, source, &selections).map_err(EatStackPortionsError::Eat)
}
