//! A lived alternative to committing to a heavy pick on a marginal reserve estimate.
//!
//! Replay the same actor-visible snapshot with the *already-owned* hand hammer. The
//! original expedition stays unchanged; this matched branch is evidence of an actual
//! newly available choice, not a prediction drawn from hidden geological truth.

use deep_hearth::content::MATERIAL_COPPER;
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::equipment::EquipmentId;
use deep_hearth::geology::{
    FieldProspectingRequest, GeologicalEvidenceKind, ResourceMassEstimate,
    player_prospecting_options, validate_start_field_prospecting,
};
use deep_hearth::inventory::StockpileId;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::spatial::VoxelBounds;

use super::super::prospecting_timing::complete_prospecting_work;
use super::extraction::{FieldworkExtractionOrder, execute_fieldwork_extraction};
use super::preparation::assemble_fieldwork_tool;
use super::survey::{FieldworkSurveyStrategy, SECONDARY_CHANNEL_START_X, search_target};
use super::{choose_fieldwork_tool_with_market_phase, fieldwork_tool_label};

pub(super) fn observe_extended_hand_assay(
    registries: &Registries,
    state: &AppState,
    hammer: EquipmentId,
    site: VoxelBounds,
    coarse: ResourceMassEstimate,
    channel_voxels: i64,
    raw: StockpileId,
    parts: StockpileId,
    destination: StockpileId,
    requested: Mass,
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
        let observed_site = localized.map_or_else(
            || "barren".to_owned(),
            |site| {
                let reserve = format!(
                    "located:{}..{}mg",
                    site.resource_mass.lower().milligrams(),
                    site.resource_mass.upper().milligrams()
                );
                let planned = requested.min(site.resource_mass.upper());
                if planned.is_zero() {
                    return format!("{reserve}:no-promising-work");
                }
                let candidate = choose_fieldwork_tool_with_market_phase(
                    registries,
                    &branch,
                    raw,
                    parts,
                    site.hardness.upper(),
                    planned,
                    "matched-assay-alternate-visible-reserve",
                )
                .unwrap_or_else(|| panic!("alternate surveyed seam has no available mining tool"));
                let (tool, preparation_ticks) =
                    assemble_fieldwork_tool(registries, &mut branch, raw, parts, candidate.tool);
                assert_eq!(preparation_ticks, candidate.preparation_ticks);
                let extraction = execute_fieldwork_extraction(
                    registries,
                    &mut branch,
                    FieldworkExtractionOrder {
                        target: site.target,
                        destination,
                        equipment: tool,
                        requested,
                        batch_limit: candidate.batch,
                    },
                );
                let first_ore_since_assay = elapsed.value()
                    + spent.value()
                    + preparation_ticks
                    + extraction.first_ore_ticks;
                format!(
                    "{reserve}:prepared:{}t:{}:first-ore-after-assay-decision:{}t:recovered:{}mg:extraction:{}t:stop:{}",
                    preparation_ticks,
                    fieldwork_tool_label(candidate.tool),
                    first_ore_since_assay,
                    extraction.extracted.milligrams(),
                    extraction.ticks,
                    extraction.stop.label(),
                )
            },
        );
        format!("searched:{}t:{observed_site}", spent.value())
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
        "FIELDWORK OPTIONAL ASSAY seed=0x{seed:016X} basis=matched-actor-state evidence=executed-ordinary-system-spatial-proxy instrument=existing-stone-hammer choice=additional-hand-sampling extra-attention={}t visible-before={}..{}mg visible-after={}..{}mg selected-next-action={} followup={} followup-scope=one-surveyed-seam-with-executed-tool-and-extraction matter=conserved original-expedition=unchanged",
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
