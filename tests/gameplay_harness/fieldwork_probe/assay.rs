//! A lived alternative to committing to a heavy pick on a marginal reserve estimate.
//!
//! Replay the same actor-visible snapshot with the *already-owned* hand hammer. The
//! original expedition stays unchanged; this matched branch is evidence of an actual
//! newly available choice, not a prediction drawn from hidden geological truth.

use deep_hearth::content::MATERIAL_COPPER;
use deep_hearth::core::state::AppState;
use deep_hearth::equipment::EquipmentId;
use deep_hearth::geology::{
    FieldProspectingRequest, GeologicalEvidenceKind, ResourceMassEstimate,
    player_prospecting_options, validate_start_field_prospecting,
};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::spatial::VoxelBounds;

use super::super::prospecting_timing::complete_prospecting_work;

pub(super) fn observe_extended_hand_assay(
    registries: &Registries,
    state: &AppState,
    hammer: EquipmentId,
    site: VoxelBounds,
    coarse: ResourceMassEstimate,
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
    reviewln!(
        "FIELDWORK OPTIONAL ASSAY seed=0x{seed:016X} basis=matched-actor-state evidence=executed-ordinary-system-spatial-proxy instrument=existing-stone-hammer choice=additional-hand-sampling extra-attention={}t visible-before={}..{}mg visible-after={}..{}mg suggested-next-action={} followup=not-executed tool-preparation=not-started matter=conserved original-expedition=unchanged",
        elapsed.value(),
        coarse.lower().milligrams(),
        coarse.upper().milligrams(),
        observed.lower().milligrams(),
        observed.upper().milligrams(),
        if poor_site {
            "seek-other-seam-before-investment"
        } else {
            "reassess-local-extraction"
        }
    );
}
