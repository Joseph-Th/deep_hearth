//! A patrol may be funded by several real fresh parcels, without minting or losing food.

use crate::content::{FORM_FOOD, MATERIAL_BERRIES, MATERIAL_WILD_NUTS};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::validate_loaded_state;
use crate::energy::EnergyCarrier;
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};

use super::{advance_to, scenario};

fn add_fragments(
    registries: &crate::Registries,
    state: &mut crate::core::state::AppState,
    den: VoxelCoord,
    amounts: [u64; 3],
) -> Vec<crate::surface::SurfaceResourceId> {
    let specs = amounts.into_iter().enumerate().map(|(index, amount)| {
        let material = if index == 1 {
            MATERIAL_BERRIES
        } else {
            MATERIAL_WILD_NUTS
        };
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            den,
            CommodityKey::new(material, FORM_FOOD),
            Mass::from_milligrams(amount),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(material),
        )
        .unwrap()
    });
    let plan = validate_create_surface_resources(registries, state, specs).unwrap();
    let ids = plan.resource_ids().collect();
    plan.commit(state).unwrap();
    ids
}

#[test]
fn fresh_mixed_fragments_fund_exact_watch_ration_but_insufficient_fragments_do_not() {
    let (registries, mut sufficient, route) =
        scenario(EnergyCarrier::Mechanical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut sufficient, 195);
    assert!(
        sufficient
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .mobilized_at
            .is_some()
    );
    let mut insufficient = sufficient.clone();
    let ids = add_fragments(
        &registries,
        &mut sufficient,
        route[0],
        [8_000, 7_000, 5_000],
    );
    let short_ids = add_fragments(
        &registries,
        &mut insufficient,
        route[0],
        [8_000, 7_000, 4_000],
    );
    let conserved = calculate_matter_accounting(&sufficient).unwrap().total();
    let short_total = calculate_matter_accounting(&insufficient).unwrap().total();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &sufficient)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();

    advance_to(&registries, &mut sufficient, 200);
    advance_to(&registries, &mut insufficient, 200);
    advance_to(&registries, &mut replay, 200);
    assert_eq!(sufficient, replay);
    let watch = sufficient.kobold_state().colony.as_ref().unwrap();
    assert_eq!(watch.watch_position(), route[1]);
    assert!(watch.watch_deployed_at.is_some());
    assert_eq!(watch.watch_tracks.len(), 1);
    for id in ids {
        assert_eq!(
            sufficient.surface().get(id).unwrap().remaining_mass(),
            Mass::ZERO
        );
    }
    assert_eq!(
        sufficient
            .kobold_state()
            .consumed_matter
            .get(&MATERIAL_WILD_NUTS)
            .copied(),
        Some(crate::core::quantity::AggregateMass::from_milligrams(
            13_000
        ))
    );
    assert_eq!(
        sufficient
            .kobold_state()
            .consumed_matter
            .get(&MATERIAL_BERRIES)
            .copied(),
        Some(crate::core::quantity::AggregateMass::from_milligrams(7_000))
    );
    assert_eq!(
        calculate_matter_accounting(&sufficient).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &sufficient), Ok(()));

    let home = insufficient.kobold_state().colony.as_ref().unwrap();
    assert_eq!(home.watch_position(), route[0]);
    assert!(home.watch_deployed_at.is_none());
    for (id, mass) in short_ids.into_iter().zip([8_000, 7_000, 4_000]) {
        assert_eq!(
            insufficient.surface().get(id).unwrap().remaining_mass(),
            Mass::from_milligrams(mass)
        );
    }
    assert!(insufficient.kobold_state().consumed_matter.is_empty());
    assert_eq!(
        calculate_matter_accounting(&insufficient).unwrap().total(),
        short_total
    );
    assert_eq!(validate_loaded_state(&registries, &insufficient), Ok(()));

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &sufficient)).unwrap();
    replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut sufficient, 220);
    advance_to(&registries, &mut replay, 220);
    assert_eq!(sufficient, replay);
    assert_eq!(
        calculate_matter_accounting(&sufficient).unwrap().total(),
        conserved
    );
}
