//! One generated-world engineer builds a mechanical worksite and faces an informed raid.
//! Only ordinary gathering, crafting, construction, storage and walking create the scenario.

use super::*;
use deep_hearth::content::{ENERGY_STONE_FLYWHEEL_DRIVE, FORM_FLYWHEEL, FORM_ROUGH_BOX_BODY};
use deep_hearth::energy::validate_assemble_energy_store;
use deep_hearth::inventory::project_material_stacks;
use deep_hearth::kobolds::{KoboldRole, available_nearby_kobold_scouts};
use deep_hearth::logistics::validate_allocate_player_ground_stockpile;
use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use deep_hearth::surface::available_adjacent_surface_resources;

const EXPOSED_STOCK: Mass = Mass::from_milligrams(600_000);

fn put_chips_at_worksite(
    registries: &Registries,
    state: &mut AppState,
    destination: StockpileId,
    carried: StockpileId,
) {
    let chips = CommodityKey::new(MATERIAL_WOOD, FORM_CHIP);
    let stack = project_material_stacks(registries, state, carried)
        .unwrap()
        .into_iter()
        .find(|stack| stack.commodity() == chips)
        .expect("shaping a real handle and boards must have left recoverable timber chips");
    assert!(stack.mass() >= EXPOSED_STOCK);
    validate_drop_stack_to_ground(registries, state, destination, &stack, EXPOSED_STOCK)
        .unwrap()
        .commit(state)
        .unwrap();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .unwrap()
            .get_mass(chips),
        EXPOSED_STOCK
    );
}

fn return_to_camp(
    registries: &Registries,
    state: &mut AppState,
    junction: VoxelCoord,
    fork: VoxelCoord,
    spawn: VoxelCoord,
) {
    assert_eq!(state.logistics().player().unwrap().position(), junction);
    walk_continuous_body_adjacent_and_settle(registries, state, fork, "leave mechanical worksite");
    walk_continuous_body_adjacent_and_settle(
        registries,
        state,
        spawn,
        "leave worksite patrol range",
    );
}

/// Proof of economic escalation from a player-made machine, not a machine fixture.
/// The actor's two alternatives protect exactly the same recoverable wood chips:
/// guarding one approach occupies the player, while a box costs real joinery.
pub(super) fn exercise_player_built_worksite_raid(print: bool) {
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WILDERNESS_WORLD_SEED;
    let spawn = generated_wilderness_surface_spawn(&registries, &mut state, seed);
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let carried = validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let conserved = calculate_matter_accounting(&state).unwrap().total();

    let fork = adjacent_surface_resource(&state, CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD))
        .position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        fork,
        "inspect workshop fork",
    );
    let junction =
        adjacent_surface_resource(&state, CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD))
            .position();
    walk_continuous_body_adjacent_and_settle(&registries, &mut state, junction, "choose worksite");

    for (commodity, amount, label) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(1_000_000),
            "gather stone for physical flywheel",
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(4_000_000),
            "gather timber for drive and enclosure",
        ),
    ] {
        let source = adjacent_surface_resource(&state, commodity);
        walk_continuous_body_adjacent_and_settle(&registries, &mut state, source.position(), label);
        gather_in_batches(&registries, &mut state, source.resource(), amount, label);
        walk_continuous_body_adjacent_and_settle(
            &registries,
            &mut state,
            junction,
            "return material to workshop",
        );
    }

    craft_in_place(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
        1,
        "hand-shape stone flywheel",
    );
    craft_in_place(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        1,
        "hand-shape drive handle",
    );
    // A matched actor leaves exactly the same wood-chip stock exposed but
    // does not assemble the physical machine. A loose ground cache alone
    // cannot teach an otherwise unalerted colony to extract inventory lots.
    let mut unwitnessed = state.clone();
    let unalerted_stock = validate_allocate_player_ground_stockpile(&unwitnessed, EXPOSED_STOCK)
        .unwrap()
        .commit(&mut unwitnessed)
        .unwrap();
    put_chips_at_worksite(&registries, &mut unwitnessed, unalerted_stock, carried);
    return_to_camp(&registries, &mut unwitnessed, junction, fork, spawn);
    let drive =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, carried)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    assert_eq!(
        state.logistics().energy_store_position(drive),
        Some(junction)
    );
    let constructed_at = state.tick();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    // Player-visible, local scout sightings reveal mobilization; the scenario
    // never reads the hidden colony's knowledge or den location to make decisions.
    return_to_camp(&registries, &mut state, junction, fork, spawn);
    let start = state.tick().value();
    let mut saw_mobilized = false;
    for _ in 0..65 {
        let next = state.tick().value() + 5;
        advance_to(
            &registries,
            &mut state,
            SimulationTick::new(next),
            TickEventAllowance::default(),
            "wait for physically reported machinery",
        );
        saw_mobilized |= available_nearby_kobold_scouts(&registries, &state)
            .any(|scout| scout.role() == KoboldRole::Scout && scout.is_mobilized());
        if saw_mobilized {
            break;
        }
    }
    assert!(
        saw_mobilized,
        "an actual scout must report the observed player-made drive"
    );
    assert!(state.tick().value() > constructed_at.value());
    let witnessed_at = state.tick();

    // Move back to an empty physical worksite before selecting a protection
    // strategy. A slow patrol step, not omniscient colony inspection, determines
    // when the visible scout has left the junction.
    for _ in 0..15 {
        if !available_nearby_kobold_scouts(&registries, &state)
            .any(|scout| scout.position() == junction)
        {
            break;
        }
        let next = state.tick().value() + 5;
        advance_to(
            &registries,
            &mut state,
            SimulationTick::new(next),
            TickEventAllowance::default(),
            "wait for worksite approach to clear",
        );
    }
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        fork,
        "return to worksite trail",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "inspect material storage",
    );

    let mut exposed = state.clone();
    let open = validate_allocate_player_ground_stockpile(&exposed, EXPOSED_STOCK)
        .unwrap()
        .commit(&mut exposed)
        .unwrap();
    put_chips_at_worksite(&registries, &mut exposed, open, carried);
    let mut guarded = exposed.clone();

    let mut enclosed = state;
    // A field box costs extra *player time* as well as its consumed timber.
    // The unprotected and actively guarded branches remain at the worksite
    // during the matched interval, so the later comparison uses equal clocks.
    let box_started_at = enclosed.tick();
    craft_in_place(
        &registries,
        &mut enclosed,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        3,
        "shape field-box boards",
    );
    craft_in_place(
        &registries,
        &mut enclosed,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY),
        1,
        "assemble field-box body",
    );
    let box_work_ticks = enclosed.tick().value() - box_started_at.value();
    assert!(
        box_work_ticks > 0,
        "protection must require actual construction attention"
    );
    let closed =
        validate_place_player_storage(&registries, &enclosed, STORAGE_ROUGH_TIMBER_FIELD_BOX)
            .unwrap()
            .commit(&mut enclosed)
            .unwrap();
    put_chips_at_worksite(&registries, &mut enclosed, closed, carried);
    assert!(
        enclosed
            .inventory()
            .get_stockpile(closed)
            .unwrap()
            .enclosure()
            .is_some()
    );
    assert_eq!(
        calculate_matter_accounting(&exposed).unwrap().total(),
        conserved
    );
    assert_eq!(
        calculate_matter_accounting(&enclosed).unwrap().total(),
        conserved
    );
    assert_eq!(
        calculate_matter_accounting(&guarded).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &exposed), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &enclosed), Ok(()));

    for (candidate, name) in [
        (
            &mut exposed,
            "unsecured camp matching enclosure construction time",
        ),
        (
            &mut guarded,
            "guarded camp matching enclosure construction time",
        ),
    ] {
        advance_to(
            &registries,
            candidate,
            enclosed.tick(),
            TickEventAllowance::default(),
            name,
        );
    }
    return_to_camp(&registries, &mut exposed, junction, fork, spawn);
    return_to_camp(&registries, &mut enclosed, junction, fork, spawn);
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut guarded,
        fork,
        "guard open worksite",
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &exposed)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut replay = loaded.into_state(&registries).unwrap();
    assert_eq!(exposed, replay);

    let decision_at = exposed.tick().value();
    let until = SimulationTick::new(decision_at + 150);
    for (candidate, name) in [
        (&mut exposed, "unattended exposed industrial stock"),
        (&mut enclosed, "protected industrial container"),
        (&mut guarded, "guarded open industrial worksite"),
        (
            &mut unwitnessed,
            "unreported technology leaves stockpile unraided",
        ),
        (&mut replay, "replayed industrial stock raid"),
    ] {
        advance_to(
            &registries,
            candidate,
            until,
            TickEventAllowance::default(),
            name,
        );
    }
    assert_eq!(
        exposed, replay,
        "the actual industrial raid must replay exactly after save/load"
    );
    let wood_chips = CommodityKey::new(MATERIAL_WOOD, FORM_CHIP);
    let exposed_left = exposed
        .inventory()
        .get_stockpile(open)
        .unwrap()
        .get_mass(wood_chips);
    let stolen = EXPOSED_STOCK.checked_sub(exposed_left).unwrap();
    assert!(
        stolen > Mass::ZERO,
        "informed scout must reach a vulnerable player-built worksite"
    );
    assert_eq!(
        guarded
            .inventory()
            .get_stockpile(open)
            .unwrap()
            .get_mass(wood_chips),
        EXPOSED_STOCK,
        "standing guard must save the stock without collecting it",
    );
    assert_eq!(
        unwitnessed
            .inventory()
            .get_stockpile(unalerted_stock)
            .unwrap()
            .get_mass(wood_chips),
        EXPOSED_STOCK,
        "without an actual machine report the same exposed inventory must not be raided",
    );
    assert_eq!(
        enclosed
            .inventory()
            .get_stockpile(closed)
            .unwrap()
            .get_mass(wood_chips),
        EXPOSED_STOCK,
        "the real field-box enclosure must block stockpile theft",
    );
    for candidate in [&exposed, &enclosed, &guarded, &unwitnessed] {
        assert_eq!(
            calculate_matter_accounting(candidate).unwrap().total(),
            conserved
        );
        assert_eq!(validate_loaded_state(&registries, candidate), Ok(()));
    }

    // The engineer now returns through the route and recovers the remaining
    // cargo from the den as a normal physical pickup, rather than reading a
    // hidden cache or fabricating a replacement parcel.
    // Pickup also runs while walking through the worksite: account for any
    // dropped cargo recovered on the approach, not only at the den.
    let carried_before = exposed
        .inventory()
        .get_stockpile(carried)
        .unwrap()
        .get_mass(wood_chips);
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut exposed,
        fork,
        "inspect raided worksite route",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut exposed,
        junction,
        "inspect raided open stockpile",
    );
    let den = available_adjacent_surface_resources(&exposed)
        .find(|resource| resource.commodity() == wood_chips)
        .expect("recovered industrial stock must be observable next to its worksite")
        .position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut exposed,
        den,
        "recover industrial raid cargo",
    );
    let carried_after = exposed
        .inventory()
        .get_stockpile(carried)
        .unwrap()
        .get_mass(wood_chips);
    assert_eq!(carried_after.checked_sub(carried_before), Some(stolen));
    assert_eq!(
        calculate_matter_accounting(&exposed).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &exposed), Ok(()));

    if print {
        std::println!(
            "KOBOLD INDUSTRIAL WORKSITE seed=0x{:016X} episode=coverage/rested/scavenger-industrial-worksite evidence=ordinary-generated-world player-built=stone-flywheel+field-box witness=[drive-assembled:{}t mobilized-observed:{}t] choices=[unattended-stock:stolen-{}mg unalerted-stock:safe-{}mg guarded-stock:safe-{}mg enclosed-stock:safe-{}mg] intervention=[guard:presence box:joinery-{}t] recovery=[route+den:actual-pickup-{}mg] save-replay:true matter=conserved witness-wait:{}t",
            seed.value(),
            constructed_at.value(),
            witnessed_at.value(),
            stolen.milligrams(),
            EXPOSED_STOCK.milligrams(),
            EXPOSED_STOCK.milligrams(),
            EXPOSED_STOCK.milligrams(),
            box_work_ticks,
            stolen.milligrams(),
            witnessed_at.value() - start,
        );
    }
}
