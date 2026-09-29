//! Deterministic actor-side material-lot selection for physically equivalent process feed.

use std::collections::BTreeMap;

use deep_hearth::core::quantity::{Mass, Temperature};
use deep_hearth::core::state::AppState;
use deep_hearth::inventory::{MaterialLotId, MaterialLotSelection, StockpileId};
use deep_hearth::material::{CommodityKey, MaterialComposition, ParticleSizeDistribution};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ObservableMaterialProfile {
    commodity: CommodityKey,
    temperature: Temperature,
    composition: MaterialComposition,
    particle_size: Option<ParticleSizeDistribution>,
}

impl ObservableMaterialProfile {
    pub(super) const fn commodity(&self) -> CommodityKey {
        self.commodity
    }

    #[allow(
        dead_code,
        reason = "only gameplay targets with material-ranking policy inspect composition directly"
    )]
    pub(super) fn composition(&self) -> &MaterialComposition {
        &self.composition
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ObservableMaterialCohort {
    profile: ObservableMaterialProfile,
    lots: Vec<MaterialLotId>,
    mass: Mass,
}

impl ObservableMaterialCohort {
    pub(super) fn profile(&self) -> &ObservableMaterialProfile {
        &self.profile
    }

    #[allow(
        dead_code,
        reason = "only gameplay targets with material-ranking policy inspect cohort mass directly"
    )]
    pub(super) const fn mass(&self) -> Mass {
        self.mass
    }

    pub(super) fn select_mass(
        &self,
        state: &AppState,
        mass: Mass,
        context: &'static str,
    ) -> Vec<MaterialLotSelection> {
        select_equivalent_lot_mass(state, self.lots.clone(), mass, context)
    }
}

pub(super) fn observable_material_cohorts(
    state: &AppState,
    stockpile: StockpileId,
) -> Vec<ObservableMaterialCohort> {
    let mut grouped = BTreeMap::<ObservableMaterialProfile, (Mass, Vec<MaterialLotId>)>::new();
    for lot_id in state.inventory().lot_ids(stockpile) {
        let lot = state
            .inventory()
            .get_lot(lot_id)
            .unwrap_or_else(|| panic!("gameplay harness material cohort lot disappeared"));
        let profile = ObservableMaterialProfile {
            commodity: lot.commodity(),
            temperature: lot.temperature(),
            composition: lot.composition().clone(),
            particle_size: lot.particle_size_distribution().cloned(),
        };
        let entry = grouped.entry(profile).or_insert((Mass::ZERO, Vec::new()));
        entry.0 = entry
            .0
            .checked_add(lot.mass())
            .unwrap_or_else(|| panic!("gameplay harness material cohort mass overflowed"));
        entry.1.push(lot_id);
    }
    grouped
        .into_iter()
        .map(|(profile, (mass, lots))| ObservableMaterialCohort {
            profile,
            lots,
            mass,
        })
        .collect()
}

fn select_equivalent_lot_mass(
    state: &AppState,
    lot_ids: Vec<deep_hearth::inventory::MaterialLotId>,
    mass: Mass,
    context: &'static str,
) -> Vec<MaterialLotSelection> {
    assert!(
        !mass.is_zero(),
        "gameplay harness {context} requires a positive material selection"
    );
    let reference = lot_ids
        .first()
        .and_then(|lot| state.inventory().get_lot(*lot))
        .unwrap_or_else(|| panic!("gameplay harness {context} has no matching source material"));
    for lot_id in lot_ids.iter().copied().skip(1) {
        let lot = state
            .inventory()
            .get_lot(lot_id)
            .unwrap_or_else(|| panic!("gameplay harness {context} source lot disappeared"));
        assert_eq!(
            lot.commodity(),
            reference.commodity(),
            "gameplay harness {context} source contains multiple commodities; choose one through an explicit actor policy"
        );
        assert_eq!(
            lot.temperature(),
            reference.temperature(),
            "gameplay harness {context} source contains multiple temperatures; choose one through an explicit actor policy"
        );
        assert_eq!(
            lot.composition(),
            reference.composition(),
            "gameplay harness {context} source contains multiple compositions; choose one through an explicit actor policy"
        );
        assert_eq!(
            lot.particle_size_distribution(),
            reference.particle_size_distribution(),
            "gameplay harness {context} source contains multiple particulate states; choose one through an explicit actor policy"
        );
    }

    let mut remaining = mass;
    let mut selections = Vec::new();
    for lot in lot_ids {
        if remaining.is_zero() {
            break;
        }
        let available = state
            .inventory()
            .get_lot(lot)
            .unwrap_or_else(|| panic!("gameplay harness {context} source lot disappeared"))
            .mass();
        let selected = Mass::from_milligrams(available.milligrams().min(remaining.milligrams()));
        if selected.is_zero() {
            continue;
        }
        selections.push(MaterialLotSelection::new(lot, selected));
        remaining = remaining
            .checked_sub(selected)
            .unwrap_or_else(|| unreachable!("selected output mass is bounded by remaining demand"));
    }
    assert!(
        remaining.is_zero(),
        "gameplay harness {context} is missing {}mg of the requested runtime material",
        remaining.milligrams()
    );
    selections
}

/// Selects an exact positive mass from one physically homogeneous stockpile.
///
/// Stable lot identity is not a legitimate player preference when temperature, composition, form,
/// or particulate state differ. This helper therefore uses lot-ID order only after proving that all
/// available lots are physically equivalent for process resolution. A heterogeneous source must be
/// handled by a probe-specific actor policy that explains which observable material it selected.
/// Production validators remain responsible for proving the resulting explicit request is legal.
pub(super) fn select_stockpile_mass(
    state: &AppState,
    stockpile: StockpileId,
    mass: Mass,
    context: &'static str,
) -> Vec<MaterialLotSelection> {
    let cohorts = observable_material_cohorts(state, stockpile);
    let [cohort] = cohorts.as_slice() else {
        panic!(
            "gameplay harness {context} source contains {} observable material cohorts; actor policy must choose one explicitly",
            cohorts.len()
        );
    };
    cohort.select_mass(state, mass, context)
}

/// Selects one commodity while refusing to hide physically distinct cohorts behind lot identity.
#[allow(
    dead_code,
    reason = "shared gameplay helper is used only by target shapes that need commodity-specific selection"
)]
pub(super) fn select_stockpile_commodity_mass(
    state: &AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
    context: &'static str,
) -> Vec<MaterialLotSelection> {
    let cohorts = observable_material_cohorts(state, stockpile)
        .into_iter()
        .filter(|cohort| cohort.profile().commodity() == commodity)
        .collect::<Vec<_>>();
    let [cohort] = cohorts.as_slice() else {
        panic!(
            "gameplay harness {context} source contains {} observable cohorts for commodity {}; actor policy must choose one explicitly",
            cohorts.len(),
            commodity.value()
        );
    };
    cohort.select_mass(state, mass, context)
}
