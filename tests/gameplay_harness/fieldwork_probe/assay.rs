//! A lived alternative to committing to a heavy pick on a marginal reserve estimate.
//!
//! Replay the same actor-visible snapshot with the *already-owned* hand hammer. The
//! original expedition stays unchanged; this matched branch is evidence of an actual
//! newly available choice, not a prediction drawn from hidden geological truth.

use deep_hearth::content::MATERIAL_COPPER;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::equipment::EquipmentId;
use deep_hearth::geology::{
    FieldProspectingRequest, GeologicalEvidenceKind, ResourceMassEstimate,
    player_prospecting_options, validate_start_field_prospecting,
};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::spatial::VoxelBounds;

use super::super::prospecting_timing::complete_prospecting_work;
use super::survey::{FieldworkSurveyStrategy, SECONDARY_CHANNEL_START_X, search_target};

pub(super) fn observe_extended_hand_assay(
    registries: &Registries,
    state: &AppState,
    hammer: EquipmentId,
    site: VoxelBounds,
    coarse: ResourceMassEstimate,
    channel_voxels: i64,
    seed: u64,
) {
    let hammer_definition = state
        .equipment()
        .get_equipment(hammer)
        .unwrap_or_else(|| panic!("matched assay lost actor's sampling hammer"))
        .definition();
    let options = player_prospecting_options(registries, state, site)
        .unwrap_or_else(|error| panic!("matched assay catalog failed: {error}"));
    let assay = options
        .into_iter()
        .filter(|option| option.evidence() == GeologicalEvidenceKind::ExcavationSample)
        .filter(|option| {
            option
                .accepted_instrument_definitions()
                .contains(&hammer_definition)
        })
        .filter(|option| {
            option
                .resource_mass_resolution()
                .is_some_and(|resolution| resolution < coarse.width())
        })
        .min_by_key(|option| option.duration().value())
        .unwrap_or_else(|| panic!("marginal-seam actor has no deeper hand-assay option"));

    let initial_matter = calculate_matter_accounting(state)
        .unwrap_or_else(|error| panic!("matched assay initial matter audit failed: {error}"))
        .total();
    let mut branch = state.clone();
    let started_at = branch.tick();
    let start = validate_start_field_prospecting(
        registries,
        &branch,
        FieldProspectingRequest::new_with_equipment(assay.method(), site, MATERIAL_COPPER, hammer),
    )
    .unwrap_or_else(|error| panic!("matched hand assay was not admissible: {error}"));
    let work = start.work();
    start
        .commit(&mut branch)
        .unwrap_or_else(|error| panic!("matched hand assay start failed: {error}"));
    let outcome =
        complete_prospecting_work(registries, &mut branch, work, "matched extended assay");
    let observed = branch
        .geological_knowledge()
        .get_observation(outcome.observation())
        .and_then(|record| record.resource_mass())
        .unwrap_or_else(|| panic!("deeper hand assay did not produce local reserve evidence"));
    assert!(
        observed.width() < coarse.width(),
        "extended sampling must produce genuinely more informative local reserve evidence"
    );
    let elapsed = branch
        .tick()
        .checked_duration_since(started_at)
        .unwrap_or_else(|| panic!("matched assay ended before it began"));
    assert_eq!(elapsed, assay.duration());
    assert_eq!(
        calculate_matter_accounting(&branch)
            .unwrap_or_else(|error| panic!("matched assay final matter audit failed: {error}"))
            .total(),
        initial_matter,
        "information-gathering must not fabricate ore or consume hidden resource mass"
    );
    assert_eq!(
        calculate_matter_accounting(state)
            .unwrap_or_else(|error| panic!(
                "original field expedition matter audit failed: {error}"
            ))
            .total(),
        initial_matter,
        "the matched investigation must not mutate the chosen expedition"
    );

    // Upper bands, not omniscient ground truth, let a player defer committing mining capital.
    let poor_site =
        u128::from(observed.upper().milligrams()) * 5 <= u128::from(coarse.upper().milligrams());
    // The alternate actor executes the first move justified by the new evidence. A nearby
    // search can be barren; it must not silently guarantee the player a replacement deposit.
    let next_site = if poor_site {
        let started = branch.tick();
        let localized = search_target(
            registries,
            &mut branch,
            hammer,
            channel_voxels,
            SECONDARY_CHANNEL_START_X,
            FieldworkSurveyStrategy::PointSearch,
        );
        let spent = branch
            .tick()
            .checked_duration_since(started)
            .unwrap_or_else(|| panic!("assay followup search ended before it began"));
        let site = localized.map_or_else(
            || "barren".to_owned(),
            |site| {
                format!(
                    "located:{}..{}mg",
                    site.resource_mass.lower().milligrams(),
                    site.resource_mass.upper().milligrams()
                )
            },
        );
        format!("searched:{}t:{site}", spent.value())
    } else {
        "not-executed".to_owned()
    };
    assert_eq!(
        calculate_matter_accounting(&branch)
            .unwrap_or_else(|error| panic!("extended assay followup matter audit failed: {error}"))
            .total(),
        initial_matter
    );
    validate_loaded_state(registries, &branch)
        .unwrap_or_else(|error| panic!("extended assay followup state invalid: {error}"));
    reviewln!(
        "FIELDWORK OPTIONAL ASSAY seed=0x{seed:016X} basis=matched-actor-state evidence=executed-ordinary-system-spatial-proxy instrument=existing-stone-hammer choice=additional-hand-sampling extra-attention={}t visible-before={}..{}mg visible-after={}..{}mg selected-next-action={} followup={} tool-preparation=not-started matter=conserved original-expedition=unchanged",
        elapsed.value(),
        coarse.lower().milligrams(),
        coarse.upper().milligrams(),
        observed.lower().milligrams(),
        observed.upper().milligrams(),
        if poor_site {
            "seek-other-seam-before-investment"
        } else {
            "reassess-local-extraction"
        },
        next_site,
    );
}
