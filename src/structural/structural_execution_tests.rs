//! Tests for the sibling structural execution module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{
    FORM_LOG, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Length};
use crate::core::time::WorldSeed;
use crate::inventory::add_solid_stockpile_for_test;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralFailureCause, StructuralStage, ValidatedStructuralDeconstruction,
    make_test_deconstruction_resolution, materialize_structural_element_for_test,
    validate_structural_deconstruction,
};

const MEMBER_AREA: Area = Area::from_square_millimeters(1_000);
const WOOD_COMPRESSION_CAPACITY_MN: u128 = 40_000_000;

fn make_test_bounds(x: i64, y: i64) -> VoxelBounds {
    match VoxelBounds::new(VoxelCoord::new(x, y, 0), VoxelCoord::new(x + 1, y + 1, 1)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("structural bounds fixture failed: {error}"),
    }
}

#[test]
fn structural_geometry_rejects_zero_length_before_allocation() {
    assert_eq!(
        StructuralElementGeometry::new(make_test_bounds(0, 0), Length::ZERO, MEMBER_AREA),
        Err(crate::structural::StructuralGeometryError::ZeroLength)
    );
    assert_eq!(
        StructuralElementGeometry::new(
            make_test_bounds(0, 0),
            Length::from_micrometers(1),
            Area::ZERO,
        ),
        Err(crate::structural::StructuralGeometryError::ZeroCrossSection)
    );

    let valid = crate::structural::make_test_structural_geometry(
        make_test_bounds(0, 0),
        Length::from_micrometers(1),
        MEMBER_AREA,
    );
    let mut encoded = match serde_json::to_value(valid) {
        Ok(encoded) => encoded,
        Err(error) => panic!("structural geometry serialization failed: {error}"),
    };
    encoded["length"] = serde_json::json!(0_u64);
    assert!(serde_json::from_value::<StructuralElementGeometry>(encoded).is_err());
}

#[test]
fn allocation_revalidates_geometry_before_mutating_state() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5700_1002));
    let invalid = StructuralElementGeometry {
        bounds: make_test_bounds(0, 0),
        length: Length::ZERO,
        cross_section: MEMBER_AREA,
    };
    let before = state.clone();

    assert_eq!(
        add_structural_element(
            &registries,
            &mut state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            invalid,
            true,
        ),
        Err(AddStructuralElementError::Geometry(
            crate::structural::StructuralGeometryError::ZeroLength
        ))
    );
    assert_eq!(state, before);
}

fn validate_test_deconstruction(
    registries: &Registries,
    state: &mut AppState,
    element: StructuralElementId,
) -> ValidatedStructuralDeconstruction {
    let mass = match state.structures().get_element(element) {
        Some(record) => record.embodied_mass(),
        None => panic!("deconstruction fixture references missing structural element"),
    };
    let destination = match add_solid_stockpile_for_test(state, mass) {
        Ok(destination) => destination,
        Err(error) => panic!("deconstruction fixture stockpile failed: {error}"),
    };
    match validate_structural_deconstruction(
        registries,
        state,
        make_test_deconstruction_resolution(element, destination),
    ) {
        Ok(token) => token,
        Err(error) => panic!("deconstruction fixture validation failed: {error}"),
    }
}

fn commit_test_deconstruction(token: ValidatedStructuralDeconstruction, state: &mut AppState) {
    if let Err(error) = token.commit(state) {
        panic!("deconstruction fixture commit failed: {error}");
    }
}

fn make_test_element(
    registries: &Registries,
    state: &mut AppState,
    x: i64,
    y: i64,
    is_grounded: bool,
) -> StructuralElementId {
    let element = match add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            make_test_bounds(x, y),
            Length::from_micrometers(1),
            MEMBER_AREA,
        ),
        is_grounded,
    ) {
        Ok(element) => element,
        Err(error) => panic!("structural element fixture failed: {error}"),
    };
    materialize_structural_element_for_test(registries, state, element, FORM_LOG);
    element
}

fn commit_test_mutation(
    token: ValidatedStructuralMutation,
    state: &mut AppState,
) -> StructuralMutationOutcome {
    match token.commit(state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("structural mutation fixture commit failed: {error}"),
    }
}

fn activate_test_element(
    registries: &Registries,
    state: &mut AppState,
    element: StructuralElementId,
) {
    let token = match validate_activate_structural_element(registries, state, element) {
        Ok(token) => token,
        Err(error) => panic!("structural activation fixture failed: {error}"),
    };
    commit_test_mutation(token, state);
}

fn link_test_support(
    registries: &Registries,
    state: &mut AppState,
    element: StructuralElementId,
    support: StructuralElementId,
) {
    let token = match validate_link_support(registries, state, element, support) {
        Ok(token) => token,
        Err(error) => panic!("structural support fixture failed: {error}"),
    };
    commit_test_mutation(token, state);
}

fn find_assessment(
    outcome: &StructuralMutationOutcome,
    element: StructuralElementId,
) -> super::super::analysis::StructuralAssessment {
    match outcome
        .analysis()
        .assessments()
        .iter()
        .copied()
        .find(|assessment| assessment.element() == element)
    {
        Some(assessment) => assessment,
        None => panic!(
            "structural assessment fixture missing element {}",
            element.value()
        ),
    }
}

#[test]
fn load_distribution_preserves_force_and_uses_stable_support_order() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5100_0001));
    let left = make_test_element(&registries, &mut state, 0, 0, true);
    let right = make_test_element(&registries, &mut state, 2, 0, true);
    let deck = make_test_element(&registries, &mut state, 1, 1, false);
    activate_test_element(&registries, &mut state, left);
    activate_test_element(&registries, &mut state, right);
    link_test_support(&registries, &mut state, deck, left);
    link_test_support(&registries, &mut state, deck, right);
    activate_test_element(&registries, &mut state, deck);

    let token = match validate_set_structural_load(
        &registries,
        &state,
        deck,
        StructuralLoadKind::Occupancy,
        Force::from_millinewtons(30_000_001),
    ) {
        Ok(token) => token,
        Err(error) => panic!("structural load validation failed: {error}"),
    };
    let outcome = commit_test_mutation(token, &mut state);
    let deck_assessment = find_assessment(&outcome, deck);
    let left_assessment = find_assessment(&outcome, left);
    let right_assessment = find_assessment(&outcome, right);

    assert_eq!(
        deck_assessment.pristine_capacity(),
        Force::from_millinewtons(WOOD_COMPRESSION_CAPACITY_MN)
    );
    assert_eq!(deck_assessment.stage(), StructuralStage::Strained);
    assert_eq!(deck_assessment.carried_load().millinewtons(), 30_000_002);
    assert_eq!(left_assessment.carried_load().millinewtons(), 15_000_002);
    assert_eq!(right_assessment.carried_load().millinewtons(), 15_000_002);
    assert_eq!(
        left_assessment.carried_load().millinewtons() - 1
            + right_assessment.carried_load().millinewtons()
            - 1,
        deck_assessment.carried_load().millinewtons()
    );
    assert_eq!(left_assessment.stage(), StructuralStage::Stable);
    assert_eq!(right_assessment.stage(), StructuralStage::Stable);
}

#[test]
fn independent_load_sources_accumulate_without_overwriting_and_zero_removes_source() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5100_0008));
    let column = make_test_element(&registries, &mut state, 0, 0, true);
    activate_test_element(&registries, &mut state, column);

    let permanent = match validate_set_structural_load(
        &registries,
        &state,
        column,
        StructuralLoadKind::Permanent,
        Force::from_millinewtons(10_000_000),
    ) {
        Ok(token) => token,
        Err(error) => panic!("permanent load validation failed: {error}"),
    };
    commit_test_mutation(permanent, &mut state);

    let snow = match validate_set_structural_load(
        &registries,
        &state,
        column,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(20_000_000),
    ) {
        Ok(token) => token,
        Err(error) => panic!("snow load validation failed: {error}"),
    };
    let combined = commit_test_mutation(snow, &mut state);
    assert_eq!(
        find_assessment(&combined, column)
            .carried_load()
            .millinewtons(),
        30_000_001
    );
    assert_eq!(
        find_assessment(&combined, column).stage(),
        StructuralStage::Strained
    );
    let record = match state.structures().get_element(column) {
        Some(record) => record,
        None => panic!("column disappeared after independent load updates"),
    };
    assert_eq!(
        record.load(StructuralLoadKind::Permanent),
        Force::from_millinewtons(10_000_000)
    );
    assert_eq!(
        record.load(StructuralLoadKind::Snow),
        Force::from_millinewtons(20_000_000)
    );

    let clear_snow = match validate_set_structural_load(
        &registries,
        &state,
        column,
        StructuralLoadKind::Snow,
        Force::ZERO,
    ) {
        Ok(token) => token,
        Err(error) => panic!("snow load removal validation failed: {error}"),
    };
    let cleared = commit_test_mutation(clear_snow, &mut state);
    assert_eq!(
        find_assessment(&cleared, column)
            .carried_load()
            .millinewtons(),
        10_000_001
    );
    let record = match state.structures().get_element(column) {
        Some(record) => record,
        None => panic!("column disappeared after clearing snow load"),
    };
    assert_eq!(record.load(StructuralLoadKind::Snow), Force::ZERO);
    assert_eq!(record.loads().count(), 2);
}

#[test]
fn self_weight_load_channel_rejects_generic_writes() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5100_0013));
    let member = make_test_element(&registries, &mut state, 0, 0, true);
    let before = state.clone();

    assert_eq!(
        validate_set_structural_load(
            &registries,
            &state,
            member,
            StructuralLoadKind::SelfWeight,
            Force::from_millinewtons(999),
        ),
        Err(StructuralMutationError::LoadOwnedBySubsystem {
            kind: StructuralLoadKind::SelfWeight,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn mutation_analysis_is_scoped_to_connected_structure_components() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5100_0012));
    let support = make_test_element(&registries, &mut state, 0, 0, true);
    let deck = make_test_element(&registries, &mut state, 0, 1, false);
    activate_test_element(&registries, &mut state, support);
    link_test_support(&registries, &mut state, deck, support);
    activate_test_element(&registries, &mut state, deck);

    let mut unrelated_elements = Vec::with_capacity(256);
    for index in 0_i64..256 {
        let unrelated = make_test_element(&registries, &mut state, 10 + index, 0, true);
        activate_test_element(&registries, &mut state, unrelated);
        unrelated_elements.push(unrelated);
    }

    let token = match validate_set_structural_load(
        &registries,
        &state,
        deck,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(20_000_000),
    ) {
        Ok(token) => token,
        Err(error) => panic!("component-scoped load validation failed: {error}"),
    };
    let assessed: Vec<_> = token
        .analysis()
        .assessments()
        .iter()
        .map(|assessment| assessment.element())
        .collect();

    assert_eq!(assessed, vec![support, deck]);
    assert!(token.analysis().damage_events().is_empty());
    commit_test_mutation(token, &mut state);
    assert!(unrelated_elements.into_iter().all(|element| {
        state
            .structures()
            .get_element(element)
            .is_some_and(|record| record.lifecycle() == StructuralLifecycle::Active)
    }));
}

#[test]
fn planned_load_contribution_overflow_is_rejected_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5100_0009));
    let member = make_test_element(&registries, &mut state, 0, 0, true);
    let maximum = match validate_set_structural_load(
        &registries,
        &state,
        member,
        StructuralLoadKind::Permanent,
        Force::from_millinewtons(u128::MAX - 1),
    ) {
        Ok(token) => token,
        Err(error) => panic!("maximum planned load validation failed: {error}"),
    };
    commit_test_mutation(maximum, &mut state);
    let before = state.clone();

    assert_eq!(
        validate_set_structural_load(
            &registries,
            &state,
            member,
            StructuralLoadKind::Snow,
            Force::from_millinewtons(1),
        ),
        Err(StructuralMutationError::Analysis(
            StructuralAnalysisError::AppliedLoadOverflow { element: member }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn crack_damage_persists_after_unloading_and_reduces_later_failure_capacity() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5100_0002));
    let column = make_test_element(&registries, &mut state, 0, 0, true);
    activate_test_element(&registries, &mut state, column);

    let crack_token = match validate_set_structural_load(
        &registries,
        &state,
        column,
        StructuralLoadKind::Permanent,
        Force::from_millinewtons(35_000_000),
    ) {
        Ok(token) => token,
        Err(error) => panic!("cracking load validation failed: {error}"),
    };
    assert_eq!(crack_token.analysis().damage_events().len(), 1);
    assert!(matches!(
        crack_token.analysis().damage_events()[0],
        StructuralDamageEvent::Cracked { element, .. } if element == column
    ));
    let cracked_outcome = commit_test_mutation(crack_token, &mut state);
    assert_eq!(
        find_assessment(&cracked_outcome, column).stage(),
        StructuralStage::Cracking
    );
    assert!(
        state
            .structures()
            .get_element(column)
            .is_some_and(|record| record.is_cracked())
    );

    let unload_token = match validate_set_structural_load(
        &registries,
        &state,
        column,
        StructuralLoadKind::Permanent,
        Force::from_millinewtons(10_000_000),
    ) {
        Ok(token) => token,
        Err(error) => panic!("unload validation failed: {error}"),
    };
    let unload_outcome = commit_test_mutation(unload_token, &mut state);
    assert!(unload_outcome.analysis().damage_events().is_empty());
    assert_eq!(
        find_assessment(&unload_outcome, column).stage(),
        StructuralStage::Cracking
    );

    let failure_token = match validate_set_structural_load(
        &registries,
        &state,
        column,
        StructuralLoadKind::Permanent,
        Force::from_millinewtons(37_000_000),
    ) {
        Ok(token) => token,
        Err(error) => panic!("post-crack overload validation failed: {error}"),
    };
    assert!(matches!(
        failure_token.analysis().damage_events(),
        [StructuralDamageEvent::Failed {
            element,
            cause: StructuralFailureCause::Overloaded {
                effective_capacity,
                ..
            }
        }] if *element == column && effective_capacity.millinewtons() == 36_000_000
    ));
    let failure_outcome = commit_test_mutation(failure_token, &mut state);
    assert_eq!(
        find_assessment(&failure_outcome, column).stage(),
        StructuralStage::Failed
    );
    assert_eq!(
        state
            .structures()
            .get_element(column)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Failed)
    );
}

#[test]
fn unchanged_public_load_is_rejected_without_revision_churn() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5100_0010));
    let member = make_test_element(&registries, &mut state, 0, 0, true);
    activate_test_element(&registries, &mut state, member);
    let load = Force::from_millinewtons(1_000_000);
    let initial = validate_set_structural_load(
        &registries,
        &state,
        member,
        StructuralLoadKind::Permanent,
        load,
    )
    .unwrap_or_else(|error| panic!("initial public load validation failed: {error}"));
    commit_test_mutation(initial, &mut state);
    let before = state.clone();

    assert_eq!(
        validate_set_structural_load(
            &registries,
            &state,
            member,
            StructuralLoadKind::Permanent,
            load,
        ),
        Err(StructuralMutationError::LoadUnchanged {
            element: member,
            kind: StructuralLoadKind::Permanent,
            load,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn removing_one_load_path_cascades_failure_through_dependents_atomically() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5100_0003));
    let foundation = make_test_element(&registries, &mut state, 0, 0, true);
    let middle = make_test_element(&registries, &mut state, 0, 1, false);
    let top = make_test_element(&registries, &mut state, 0, 2, false);
    activate_test_element(&registries, &mut state, foundation);
    link_test_support(&registries, &mut state, middle, foundation);
    activate_test_element(&registries, &mut state, middle);
    link_test_support(&registries, &mut state, top, middle);
    activate_test_element(&registries, &mut state, top);

    let token = match validate_remove_support(&registries, &state, middle, foundation) {
        Ok(token) => token,
        Err(error) => panic!("support removal validation failed: {error}"),
    };
    assert_eq!(token.analysis().damage_events().len(), 2);
    assert!(
        token
            .analysis()
            .damage_events()
            .iter()
            .all(|event| matches!(
                event,
                StructuralDamageEvent::Failed {
                    cause: StructuralFailureCause::Unsupported,
                    ..
                }
            ))
    );
    let before_revision = state.structures().revision();
    commit_test_mutation(token, &mut state);

    assert_eq!(state.structures().revision(), before_revision + 1);
    assert_eq!(
        state
            .structures()
            .get_element(foundation)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Active)
    );
    assert_eq!(
        state
            .structures()
            .get_element(middle)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Failed)
    );
    assert_eq!(
        state
            .structures()
            .get_element(top)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Failed)
    );
}

#[test]
fn removing_member_redistributes_to_surviving_support_and_cleans_indexes() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5100_0010));
    let left = make_test_element(&registries, &mut state, 0, 0, true);
    let right = make_test_element(&registries, &mut state, 2, 0, true);
    let deck = make_test_element(&registries, &mut state, 1, 1, false);
    activate_test_element(&registries, &mut state, left);
    activate_test_element(&registries, &mut state, right);
    link_test_support(&registries, &mut state, deck, left);
    link_test_support(&registries, &mut state, deck, right);
    activate_test_element(&registries, &mut state, deck);
    let load = match validate_set_structural_load(
        &registries,
        &state,
        deck,
        StructuralLoadKind::Occupancy,
        Force::from_millinewtons(30_000_000),
    ) {
        Ok(token) => token,
        Err(error) => panic!("member removal load fixture failed: {error}"),
    };
    commit_test_mutation(load, &mut state);

    let removal = validate_test_deconstruction(&registries, &mut state, left);
    assert!(removal.structural_analysis().damage_events().is_empty());
    let right_assessment = match removal
        .structural_analysis()
        .assessments()
        .iter()
        .copied()
        .find(|assessment| assessment.element() == right)
    {
        Some(assessment) => assessment,
        None => panic!("surviving support assessment disappeared during removal planning"),
    };
    assert_eq!(right_assessment.carried_load().millinewtons(), 30_000_002);
    assert_eq!(right_assessment.stage(), StructuralStage::Strained);
    commit_test_deconstruction(removal, &mut state);

    assert!(state.structures().get_element(left).is_none());
    assert!(state.structures().supports(left).is_none());
    assert!(state.structures().dependents(left).is_none());
    let deck_supports: Vec<_> = match state.structures().supports(deck) {
        Some(supports) => supports.collect(),
        None => panic!("deck support index disappeared after member removal"),
    };
    assert_eq!(deck_supports, vec![right]);
    let right_dependents: Vec<_> = match state.structures().dependents(right) {
        Some(dependents) => dependents.collect(),
        None => panic!("surviving support reverse index disappeared"),
    };
    assert_eq!(right_dependents, vec![deck]);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn failed_debris_can_be_removed_and_rebuilt_without_reusing_identity() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5100_0011));
    let foundation = make_test_element(&registries, &mut state, 0, 0, true);
    let middle = make_test_element(&registries, &mut state, 0, 1, false);
    let top = make_test_element(&registries, &mut state, 0, 2, false);
    activate_test_element(&registries, &mut state, foundation);
    link_test_support(&registries, &mut state, middle, foundation);
    activate_test_element(&registries, &mut state, middle);
    link_test_support(&registries, &mut state, top, middle);
    activate_test_element(&registries, &mut state, top);

    let remove_foundation = validate_test_deconstruction(&registries, &mut state, foundation);
    assert_eq!(
        remove_foundation
            .structural_analysis()
            .damage_events()
            .len(),
        2
    );
    commit_test_deconstruction(remove_foundation, &mut state);
    assert_eq!(
        state
            .structures()
            .get_element(middle)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Failed)
    );
    assert_eq!(
        state
            .structures()
            .get_element(top)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Failed)
    );

    for debris in [top, middle] {
        let token = validate_test_deconstruction(&registries, &mut state, debris);
        commit_test_deconstruction(token, &mut state);
        assert!(state.structures().get_element(debris).is_none());
    }
    assert_eq!(state.structures().elements().count(), 0);

    let replacement_foundation = make_test_element(&registries, &mut state, 0, 0, true);
    let replacement_upper = make_test_element(&registries, &mut state, 0, 1, false);
    assert!(replacement_foundation > top);
    assert!(replacement_upper > replacement_foundation);
    activate_test_element(&registries, &mut state, replacement_foundation);
    link_test_support(
        &registries,
        &mut state,
        replacement_upper,
        replacement_foundation,
    );
    activate_test_element(&registries, &mut state, replacement_upper);

    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    assert_eq!(
        state
            .structures()
            .get_element(replacement_upper)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Active)
    );
}

#[test]
fn support_cycle_is_rejected_before_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5100_0004));
    let first = make_test_element(&registries, &mut state, 0, 0, false);
    let second = make_test_element(&registries, &mut state, 1, 0, false);
    link_test_support(&registries, &mut state, first, second);
    let before = state.clone();

    assert_eq!(
        validate_link_support(&registries, &state, second, first),
        Err(StructuralMutationError::SupportCycle {
            element: second,
            support: first,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn unsupported_planned_member_cannot_activate() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5100_0005));
    let member = make_test_element(&registries, &mut state, 0, 0, false);
    let before = state.clone();

    assert_eq!(
        validate_activate_structural_element(&registries, &state, member),
        Err(StructuralMutationError::ActivationUnsupported { element: member })
    );
    assert_eq!(state, before);
}

#[test]
fn stale_structural_token_cannot_overwrite_later_structural_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5100_0006));
    let member = make_test_element(&registries, &mut state, 0, 0, true);
    activate_test_element(&registries, &mut state, member);
    let expected_revision = state.structures().revision();
    let stale = match validate_set_structural_load(
        &registries,
        &state,
        member,
        StructuralLoadKind::Permanent,
        Force::from_millinewtons(1_000_000),
    ) {
        Ok(token) => token,
        Err(error) => panic!("stale token fixture failed: {error}"),
    };
    make_test_element(&registries, &mut state, 4, 0, true);
    let before_commit = state.clone();

    assert_eq!(
        stale.commit(&mut state),
        Err(StructuralCommitError::StaleRevision {
            expected: expected_revision,
            actual: expected_revision + 2,
        })
    );
    assert_eq!(state, before_commit);
}

#[test]
fn long_support_chain_collapse_is_complete_and_deterministically_ordered() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5100_0007));
    let foundation = make_test_element(&registries, &mut state, 0, 0, true);
    activate_test_element(&registries, &mut state, foundation);
    let mut support = foundation;
    let mut chain = Vec::new();

    for index in 0_i64..128 {
        let element = make_test_element(&registries, &mut state, 0, index + 1, false);
        link_test_support(&registries, &mut state, element, support);
        activate_test_element(&registries, &mut state, element);
        chain.push(element);
        support = element;
    }

    let first = chain[0];
    let token = match validate_remove_support(&registries, &state, first, foundation) {
        Ok(token) => token,
        Err(error) => panic!("long-chain collapse validation failed: {error}"),
    };
    assert_eq!(token.analysis().damage_events().len(), chain.len());
    let event_ids: Vec<_> = token
        .analysis()
        .damage_events()
        .iter()
        .map(|event| event.element())
        .collect();
    assert_eq!(event_ids, chain);
    commit_test_mutation(token, &mut state);

    assert!(chain.iter().all(|element| {
        state
            .structures()
            .get_element(*element)
            .is_some_and(|record| {
                record.lifecycle() == StructuralLifecycle::Failed && record.is_cracked()
            })
    }));
    assert_eq!(
        state
            .structures()
            .get_element(foundation)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Active)
    );
}
