//! Player-facing evidence that specialist geological information changes a real follow-up choice.

use deep_hearth::content::gameplay_fixture::{
    GeologicalDepositSeed, seed_geological_deposit, seed_lot,
};
use deep_hearth::content::{
    EQUIPMENT_STONE_GEOLOGICAL_HAMMER, EQUIPMENT_TIMBER_CHANNEL_SAMPLING_FRAME,
    EQUIPMENT_TIMBER_TRIPOD_CORE_DRILL, FORM_ORE, MATERIAL_COPPER,
    PROSPECTING_CHANNEL_COMPOSITE_SURVEY, PROSPECTING_DETAILED_FIELD_SURVEY,
    PROSPECTING_REGIONAL_RECONNAISSANCE, PROSPECTING_SHALLOW_CORE_SURVEY,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::equipment::validate_assemble_equipment;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::mining::{MiningTargetRequest, resolve_mining_target};
use deep_hearth::registry::Registries;
use deep_hearth::survival::initialize_player_survival;

use super::super::environment::ROOM_TEMPERATURE;
use super::super::inventory_support::add_solid_stockpile;
use super::super::ore_fixture::copper_ore_composition;
use super::extraction::{FieldworkExtractionOrder, FieldworkStop, execute_fieldwork_extraction};
use super::planning::{
    choose_fieldwork_tool_quiet, fieldwork_bulk_crossover, fieldwork_mining_limits,
    fieldwork_raw_opportunity, fieldwork_tool_label, raw_opportunity_for_equipment_components,
};
use super::preparation::{assemble_fieldwork_tool, craft_equipment_components};
use super::survey::{FieldworkSurveyStrategy, horizontal_region, run_survey, search_target};

const PROSPECT_WIDTH: i64 = 8;
const FIRST_PROSPECT_X: i64 = 1_000;
const SECOND_PROSPECT_X: i64 = 1_100;

fn finding_bounds(
    state: &AppState,
    outcome: deep_hearth::geology::FieldProspectingOutcome,
) -> (u32, u32) {
    let record = state
        .geological_knowledge()
        .get_observation(outcome.observation())
        .unwrap_or_else(|| panic!("fieldwork knowledge observation disappeared"));
    let finding = record
        .finding(MATERIAL_COPPER)
        .unwrap_or_else(|| panic!("fieldwork knowledge copper finding disappeared"));
    (finding.lower_ppm(), finding.upper_ppm())
}

pub(super) fn run_fieldwork_knowledge_experience(registries: &Registries, seed: u64) {
    let regional = registries
        .labor()
        .get_prospecting(PROSPECTING_REGIONAL_RECONNAISSANCE)
        .copied()
        .unwrap_or_else(|| panic!("fieldwork regional reconnaissance disappeared"));
    let channel = registries
        .labor()
        .get_prospecting(PROSPECTING_CHANNEL_COMPOSITE_SURVEY)
        .copied()
        .unwrap_or_else(|| panic!("fieldwork channel-composite survey disappeared"));
    let detailed = registries
        .labor()
        .get_prospecting(PROSPECTING_DETAILED_FIELD_SURVEY)
        .copied()
        .unwrap_or_else(|| panic!("fieldwork detailed survey disappeared"));
    let core = registries
        .labor()
        .get_prospecting(PROSPECTING_SHALLOW_CORE_SURVEY)
        .copied()
        .unwrap_or_else(|| panic!("fieldwork shallow-core survey disappeared"));
    let broad_uncertainty = regional.abundance_uncertainty_ppm();
    let precise_uncertainty = channel.abundance_uncertainty_ppm();
    assert!(
        broad_uncertainty > precise_uncertainty.saturating_mul(5),
        "channel sampling must retain a materially tighter abundance product than reconnaissance"
    );
    assert!(
        regional.maximum_region_voxels() >= PROSPECT_WIDTH as u128
            && channel.maximum_region_voxels() >= PROSPECT_WIDTH as u128,
        "fieldwork knowledge witness requires both methods to cover one bounded prospect"
    );
    let detailed_hardness_resolution = detailed
        .excavation_hardness_resolution()
        .unwrap_or_else(|| panic!("fieldwork detailed survey lost hardness evidence"));
    let core_hardness_resolution = core
        .excavation_hardness_resolution()
        .unwrap_or_else(|| panic!("fieldwork core survey lost hardness evidence"));
    let detailed_mass_resolution = detailed
        .resource_mass_resolution()
        .unwrap_or_else(|| panic!("fieldwork detailed survey lost reserve evidence"));
    let core_mass_resolution = core
        .resource_mass_resolution()
        .unwrap_or_else(|| panic!("fieldwork core survey lost reserve evidence"));
    assert!(
        detailed_hardness_resolution > core_hardness_resolution,
        "fieldwork core survey must retain tighter hardness evidence than the hand sample"
    );
    assert!(
        detailed_mass_resolution > core_mass_resolution,
        "fieldwork core survey must retain tighter reserve evidence than the hand sample"
    );
    let mining_limits = fieldwork_mining_limits(registries);
    // Put the reserve witness exactly on the ordinary quarry hardness boundary. Both the hammer
    // and core methods therefore report the same usable upper hardness, isolating the capital
    // decision change to reserve knowledge rather than giving the core drill two simultaneous
    // advantages.
    let prospect_hardness = mining_limits.base_quarry_hardness;

    // Derive two close opportunities from the live information hierarchy. Broad reconnaissance
    // leaves overlapping grade bands, while the specialist frame narrows them until the second
    // prospect's lower bound clears the first prospect's upper bound. The actor never receives
    // these hidden grades; they only author the controlled world.
    let grade_gap = precise_uncertainty
        .checked_mul(4)
        .unwrap_or_else(|| panic!("fieldwork knowledge grade gap overflowed"));
    let first_grade = broad_uncertainty
        .checked_add(grade_gap)
        .unwrap_or_else(|| panic!("fieldwork knowledge first grade overflowed"));
    let second_grade = first_grade
        .checked_add(grade_gap)
        .unwrap_or_else(|| panic!("fieldwork knowledge second grade overflowed"));
    assert!(
        second_grade.saturating_add(broad_uncertainty) < 1_000_000,
        "fieldwork knowledge witness lost the unsaturated reconnaissance comparison band"
    );

    let first_region = horizontal_region(FIRST_PROSPECT_X, PROSPECT_WIDTH);
    let second_region = horizontal_region(SECOND_PROSPECT_X, PROSPECT_WIDTH);

    let (mut raw_opportunity, mut parts_capacity) = fieldwork_raw_opportunity(registries);
    let (specialist_raw, specialist_parts_capacity) = raw_opportunity_for_equipment_components(
        registries,
        &[
            EQUIPMENT_TIMBER_CHANNEL_SAMPLING_FRAME,
            EQUIPMENT_TIMBER_TRIPOD_CORE_DRILL,
        ],
        "knowledge-instrument raw opportunity",
    );
    for (commodity, mass) in specialist_raw {
        let entry = raw_opportunity.entry(commodity).or_insert(Mass::ZERO);
        *entry = entry
            .checked_add(mass)
            .unwrap_or_else(|| panic!("fieldwork specialist raw opportunity overflowed"));
    }
    parts_capacity = parts_capacity
        .checked_add(specialist_parts_capacity)
        .unwrap_or_else(|| panic!("fieldwork specialist parts capacity overflowed"));
    let raw_capacity = raw_opportunity
        .values()
        .copied()
        .try_fold(Mass::ZERO, Mass::checked_add)
        .unwrap_or_else(|| panic!("fieldwork knowledge raw capacity overflowed"));
    // The tool-market projection itself requires an admitted survival owner. Derive the live
    // crossover in a separate actor-valid planning state so the played world can still finish all
    // geological fixture construction before its actor is admitted.
    let mut planning_state = AppState::new();
    let planning_raw = add_solid_stockpile(&mut planning_state, raw_capacity);
    for (&commodity, &mass) in &raw_opportunity {
        let _ = seed_lot(
            registries,
            &mut planning_state,
            planning_raw,
            commodity,
            mass,
            ROOM_TEMPERATURE,
        );
    }
    let planning_parts = add_solid_stockpile(&mut planning_state, parts_capacity);
    initialize_player_survival(registries, &mut planning_state)
        .unwrap_or_else(|error| panic!("fieldwork knowledge planning survival failed: {error}"));
    let sampled_crossover = fieldwork_bulk_crossover(
        registries,
        &planning_state,
        planning_raw,
        planning_parts,
        mining_limits.base_quarry_hardness,
        mining_limits.base_quarry_batch,
    )
    .unwrap_or_else(|| panic!("fieldwork knowledge witness lost its soft-rock capital crossover"));
    let sampled_heavy = choose_fieldwork_tool_quiet(
        registries,
        &planning_state,
        planning_raw,
        planning_parts,
        mining_limits.base_quarry_hardness,
        sampled_crossover.order,
    )
    .unwrap_or_else(|| panic!("fieldwork sampled crossover lost its selected tool"));
    let mut exact_crossover = None;
    let exact_steps = sampled_crossover.order.milligrams() / core_mass_resolution.milligrams();
    for steps in 1..=exact_steps {
        let order = Mass::from_milligrams(
            core_mass_resolution
                .milligrams()
                .checked_mul(steps)
                .unwrap_or_else(|| panic!("fieldwork exact crossover mass overflowed")),
        );
        let Some(choice) = choose_fieldwork_tool_quiet(
            registries,
            &planning_state,
            planning_raw,
            planning_parts,
            mining_limits.base_quarry_hardness,
            order,
        ) else {
            continue;
        };
        if choice.tool.target == sampled_heavy.tool.target {
            exact_crossover = Some((order, choice));
            break;
        }
    }
    let (capital_crossover_order, capital_crossover_choice) = exact_crossover
        .unwrap_or_else(|| panic!("fieldwork sampled heavy-tool crossover had no exact boundary"));
    assert!(
        capital_crossover_order > detailed_mass_resolution,
        "fieldwork capital crossover must exceed one hand-sample reserve bucket"
    );
    let detailed_resolution_mg = detailed_mass_resolution.milligrams();
    let core_resolution_mg = core_mass_resolution.milligrams();
    assert!(
        detailed_resolution_mg > core_resolution_mg
            && detailed_resolution_mg.is_multiple_of(core_resolution_mg),
        "fieldwork knowledge witness requires nested reserve-resolution buckets"
    );
    let crossover_mg = capital_crossover_order.milligrams();
    let body_mg = (1..=detailed_resolution_mg / core_resolution_mg + 1)
        .filter_map(|steps| crossover_mg.checked_sub(steps * core_resolution_mg))
        .filter(|&actual_mg| {
            let hammer_upper = (actual_mg / detailed_resolution_mg + 1)
                .saturating_mul(detailed_resolution_mg);
            let core_upper = (actual_mg / core_resolution_mg + 1)
                .saturating_mul(core_resolution_mg);
            hammer_upper >= crossover_mg && core_upper < crossover_mg
        })
        .min()
        .unwrap_or_else(|| {
            panic!(
                "fieldwork reserve resolutions cannot straddle the live capital crossover: crossover={crossover_mg}mg hammer-resolution={detailed_resolution_mg}mg core-resolution={core_resolution_mg}mg"
            )
        });
    let body_mass = Mass::from_milligrams(body_mg);

    let mut state = AppState::new();
    let raw = add_solid_stockpile(&mut state, raw_capacity);
    for (commodity, mass) in raw_opportunity {
        let _ = seed_lot(
            registries,
            &mut state,
            raw,
            commodity,
            mass,
            ROOM_TEMPERATURE,
        );
    }
    let parts = add_solid_stockpile(&mut state, parts_capacity);
    for (start_x, grade) in [
        (FIRST_PROSPECT_X, first_grade),
        (SECOND_PROSPECT_X, second_grade),
    ] {
        // Model each prospect as a contiguous set of localized bodies. Aggregate surveys see
        // complete uniform coverage, while a later one-voxel excavation sample can still resolve
        // the mass of the specific body the player chose to inspect.
        for offset in 0..PROSPECT_WIDTH {
            seed_geological_deposit(
                registries,
                &mut state,
                GeologicalDepositSeed::new(
                    horizontal_region(start_x + offset, 1),
                    CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
                    body_mass,
                    ROOM_TEMPERATURE,
                    prospect_hardness,
                    copper_ore_composition(grade, 50_000),
                ),
            );
        }
    }
    let destination = add_solid_stockpile(&mut state, capital_crossover_order);
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("fieldwork knowledge matter setup failed: {error}"))
        .total();
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("fieldwork knowledge survival setup failed: {error}"));

    let first_coarse_outcome = run_survey(
        registries,
        &mut state,
        PROSPECTING_REGIONAL_RECONNAISSANCE,
        first_region,
        None,
        "knowledge first reconnaissance",
    );
    let first_coarse = finding_bounds(&state, first_coarse_outcome);
    let second_coarse_outcome = run_survey(
        registries,
        &mut state,
        PROSPECTING_REGIONAL_RECONNAISSANCE,
        second_region,
        None,
        "knowledge second reconnaissance",
    );
    let second_coarse = finding_bounds(&state, second_coarse_outcome);
    assert!(
        first_coarse.1 >= second_coarse.0,
        "broad reconnaissance must leave the two prospect grade bands overlapping"
    );

    let frame_setup_ticks = craft_equipment_components(
        registries,
        &mut state,
        raw,
        parts,
        &[EQUIPMENT_TIMBER_CHANNEL_SAMPLING_FRAME],
        "fieldwork channel-frame components",
    );
    let frame = validate_assemble_equipment(
        registries,
        &state,
        EQUIPMENT_TIMBER_CHANNEL_SAMPLING_FRAME,
        parts,
    )
    .unwrap_or_else(|error| panic!("fieldwork channel-frame assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fieldwork channel-frame commit failed: {error}"));
    let first_refined_outcome = run_survey(
        registries,
        &mut state,
        PROSPECTING_CHANNEL_COMPOSITE_SURVEY,
        first_region,
        Some(frame),
        "knowledge first channel composite",
    );
    let first_refined = finding_bounds(&state, first_refined_outcome);
    let second_refined_outcome = run_survey(
        registries,
        &mut state,
        PROSPECTING_CHANNEL_COMPOSITE_SURVEY,
        second_region,
        Some(frame),
        "knowledge second channel composite",
    );
    let second_refined = finding_bounds(&state, second_refined_outcome);
    assert!(
        first_refined.1 < second_refined.0,
        "specialist channel evidence must establish a non-overlapping richer second prospect using actor-visible bounds"
    );

    // Overlapping coarse bands do not justify spending detailed follow-up effort on either site.
    // The specialist evidence resolves that uncertainty and unlocks the richer second prospect.
    let coarse_selection = (first_coarse.1 < second_coarse.0).then_some(SECOND_PROSPECT_X);
    assert_eq!(coarse_selection, None);
    let selected_start = if first_refined.1 < second_refined.0 {
        SECOND_PROSPECT_X
    } else {
        FIRST_PROSPECT_X
    };
    assert_eq!(selected_start, SECOND_PROSPECT_X);
    let hammer_setup_ticks = craft_equipment_components(
        registries,
        &mut state,
        raw,
        parts,
        &[EQUIPMENT_STONE_GEOLOGICAL_HAMMER],
        "fieldwork detailed-sampling hammer components",
    );
    let hammer =
        validate_assemble_equipment(registries, &state, EQUIPMENT_STONE_GEOLOGICAL_HAMMER, parts)
            .unwrap_or_else(|error| panic!("fieldwork knowledge hammer assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("fieldwork knowledge hammer commit failed: {error}"));
    let localization = search_target(
        registries,
        &mut state,
        hammer,
        PROSPECT_WIDTH / 2,
        selected_start,
        FieldworkSurveyStrategy::PointSearch,
    )
    .unwrap_or_else(|| panic!("specialist-selected fieldwork prospect failed detailed follow-up"));
    assert_eq!(
        localization.target.region(),
        horizontal_region(SECOND_PROSPECT_X, 1),
        "better information must direct the expensive local follow-up to the richer prospect"
    );
    assert!(
        localization.resource_mass.lower() < capital_crossover_order
            && localization.resource_mass.upper() >= capital_crossover_order,
        "ordinary hammer reserve evidence must straddle the live heavy-tool crossover"
    );
    let hammer_plan = capital_crossover_order.min(localization.resource_mass.upper());
    let hammer_choice = choose_fieldwork_tool_quiet(
        registries,
        &state,
        raw,
        parts,
        localization.hardness.upper(),
        hammer_plan,
    )
    .unwrap_or_else(|| panic!("fieldwork hammer evidence lost every extraction route"));
    assert_eq!(
        hammer_choice.tool.target, capital_crossover_choice.tool.target,
        "coarse reserve evidence must make the live heavy-tool crossover look actionable"
    );

    let core_setup_ticks = craft_equipment_components(
        registries,
        &mut state,
        raw,
        parts,
        &[EQUIPMENT_TIMBER_TRIPOD_CORE_DRILL],
        "fieldwork core-drill components",
    );
    let core_drill = validate_assemble_equipment(
        registries,
        &state,
        EQUIPMENT_TIMBER_TRIPOD_CORE_DRILL,
        parts,
    )
    .unwrap_or_else(|error| panic!("fieldwork core-drill assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fieldwork core-drill commit failed: {error}"));
    let core_outcome = run_survey(
        registries,
        &mut state,
        PROSPECTING_SHALLOW_CORE_SURVEY,
        localization.target.region(),
        Some(core_drill),
        "knowledge selected-prospect core sample",
    );
    let core_record = state
        .geological_knowledge()
        .get_observation(core_outcome.observation())
        .unwrap_or_else(|| panic!("fieldwork core observation disappeared"));
    let core_hardness = core_record
        .excavation_hardness()
        .unwrap_or_else(|| panic!("fieldwork core sample lost hardness evidence"));
    let core_resource_mass = core_record
        .resource_mass()
        .unwrap_or_else(|| panic!("fieldwork core sample lost resource-mass evidence"));
    assert_eq!(
        core_hardness.upper(),
        localization.hardness.upper(),
        "reserve witness must isolate core-drill capital effects from hardness refinement"
    );
    assert!(
        core_resource_mass.upper() < capital_crossover_order,
        "core evidence must resolve the local reserve below the heavy-tool crossover"
    );
    let core_plan = capital_crossover_order.min(core_resource_mass.upper());
    let core_choice = choose_fieldwork_tool_quiet(
        registries,
        &state,
        raw,
        parts,
        core_hardness.upper(),
        core_plan,
    )
    .unwrap_or_else(|| panic!("fieldwork core evidence lost every extraction route"));
    assert_ne!(
        core_choice.tool.target, hammer_choice.tool.target,
        "tighter reserve evidence must change the capital choice near the crossover"
    );
    assert!(
        core_choice.total_ticks() < hammer_choice.total_ticks(),
        "core-informed smaller local workload must prefer the lower-attention extraction plan"
    );
    let projected_attention_saved = hammer_choice.total_ticks() - core_choice.total_ticks();
    assert!(
        projected_attention_saved > core.duration().value(),
        "specialist core evidence must repay its own use attention when it changes the live capital decision"
    );
    let net_information_ticks = projected_attention_saved - core.duration().value();
    let setup_payback_uses = core_setup_ticks.div_ceil(net_information_ticks);

    let (mining_tool, tool_setup_ticks) =
        assemble_fieldwork_tool(registries, &mut state, raw, parts, core_choice.tool);
    assert_eq!(
        tool_setup_ticks, core_choice.preparation_ticks,
        "core-informed extraction setup must match its pre-action projection"
    );
    let refined_target = resolve_mining_target(
        &state,
        MiningTargetRequest::new(localization.target.region(), MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("fieldwork core-informed target refresh failed: {error}"));
    let extraction = execute_fieldwork_extraction(
        registries,
        &mut state,
        FieldworkExtractionOrder {
            target: refined_target,
            destination,
            equipment: mining_tool,
            requested: core_plan,
            batch_limit: core_choice.batch,
        },
    );
    assert_eq!(extraction.stop, FieldworkStop::ShortClaim);
    assert_eq!(extraction.extracted, body_mass);
    assert!(
        core_plan.checked_sub(body_mass).unwrap_or(Mass::ZERO)
            < hammer_plan.checked_sub(body_mass).unwrap_or(Mass::ZERO),
        "core evidence must reduce the actor's inevitable upper-bound overcommit"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("fieldwork knowledge matter audit failed: {error}"))
            .total(),
        matter_before,
        "fieldwork knowledge investment must conserve matter"
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("fieldwork knowledge final state invalid: {error}"));
    let frame_condition = state
        .equipment()
        .get_equipment(frame)
        .map(|record| record.condition().parts_per_million())
        .unwrap_or_else(|| panic!("fieldwork channel frame disappeared"));

    reviewln!(
        "FIELDWORK KNOWLEDGE EXPERIENCE seed=0x{seed:016X} evidence-scope=ordinary-system-spatial-proxy movement=abstracted locality-claim=false basis=actor-evidence coarse=[first:{}..{}ppm second:{}..{}ppm overlapping:true selection:defer] specialist=channel-frame refined=[first:{}..{}ppm second:{}..{}ppm separation:{}ppm] selection=[refined:second changed:true] investment=[frame-components:{}t raw-roots:finite-disclosed-starting-state] followup=[hammer-components:{}t strategy:point-search transects:{} inspections:{} detailed:{} target:second] core=[setup:{}t survey:{}t hammer-reserve:{}..{}mg reserve:{}..{}mg crossover:{}mg hammer-tool:{} hammer-plan:{}t core-tool:{} core-plan:{}t plan:{}mg extracted:{}mg stop:{} attention-saved:{}t net:{}t payback:{}uses] instrument=[condition:{}ppm] matter=conserved",
        first_coarse.0,
        first_coarse.1,
        second_coarse.0,
        second_coarse.1,
        first_refined.0,
        first_refined.1,
        second_refined.0,
        second_refined.1,
        second_refined.0.saturating_sub(first_refined.1),
        frame_setup_ticks,
        hammer_setup_ticks,
        localization.transects,
        localization.field_inspections,
        localization.detailed_surveys,
        core_setup_ticks,
        core.duration().value(),
        localization.resource_mass.lower().milligrams(),
        localization.resource_mass.upper().milligrams(),
        core_resource_mass.lower().milligrams(),
        core_resource_mass.upper().milligrams(),
        capital_crossover_order.milligrams(),
        fieldwork_tool_label(hammer_choice.tool),
        hammer_choice.total_ticks(),
        fieldwork_tool_label(core_choice.tool),
        core_choice.total_ticks(),
        core_plan.milligrams(),
        extraction.extracted.milligrams(),
        extraction.stop.label(),
        projected_attention_saved,
        net_information_ticks,
        setup_payback_uses,
        frame_condition,
    );
}
