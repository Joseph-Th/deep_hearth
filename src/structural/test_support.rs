//! Unit-test conveniences layered over canonical structural construction and geometry boundaries.

use crate::core::quantity::{Area, Length, Temperature};
use crate::core::state::AppState;
use crate::inventory::{MaterialLotSelection, add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::material::{CommodityKey, FormId};
use crate::registry::Registries;
use crate::spatial::VoxelBounds;

use super::construction_execution::{
    bind_structural_construction_selection, resolve_structural_material_requirement,
    validate_structural_construction,
};
use super::{StructuralElementGeometry, StructuralElementId};

pub(crate) fn make_test_structural_geometry(
    bounds: VoxelBounds,
    length: Length,
    cross_section: Area,
) -> StructuralElementGeometry {
    match StructuralElementGeometry::new(bounds, length, cross_section) {
        Ok(geometry) => geometry,
        Err(error) => panic!("structural test geometry is invalid: {error}"),
    }
}

pub(crate) fn materialize_structural_element_for_test(
    registries: &Registries,
    state: &mut AppState,
    element: StructuralElementId,
    form: FormId,
) {
    let requirement = match resolve_structural_material_requirement(registries, state, element) {
        Ok(requirement) => requirement,
        Err(error) => panic!("construction test material requirement failed: {error}"),
    };
    let material = requirement.material();
    let mass = requirement.required_mass();
    let source = match add_solid_stockpile_for_test(state, mass) {
        Ok(source) => source,
        Err(error) => panic!("construction test stockpile failed: {error}"),
    };
    let lot = match deposit_lot_for_test(
        registries,
        state,
        source,
        CommodityKey::new(material, form),
        mass,
        Temperature::from_millikelvin(293_150),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("construction test material deposit failed: {error}"),
    };
    let resolution = match bind_structural_construction_selection(
        state,
        element,
        source,
        &[MaterialLotSelection::new(lot, mass)],
    ) {
        Ok(resolution) => resolution,
        Err(error) => panic!("construction test material binding failed: {error:?}"),
    };
    let token = match validate_structural_construction(registries, state, resolution) {
        Ok(token) => token,
        Err(error) => panic!("construction test validation failed: {error}"),
    };
    if let Err(error) = token.commit(state) {
        panic!("construction test commit failed: {error}");
    }
}
