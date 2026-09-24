//! Deterministic primitive-progression world construction before player admission.

use super::*;

pub(super) struct ProgressionWorldSetup {
    pub(super) state: AppState,
    pub(super) mined_mass: Mass,
    pub(super) soft_ore_deposit_mass: Mass,
    pub(super) hard_ore_deposit_mass: Mass,
    pub(super) ore_copper_ppm: u32,
    pub(super) hard_ore_copper_ppm: u32,
    pub(super) trace_copper_ppm: u32,
    pub(super) raw_surplus: Mass,
    pub(super) stone_pick_batch_limit: Mass,
    pub(super) stone_hardness_limit: Pressure,
    pub(super) reinforced_hardness_limit: Pressure,
    pub(super) hard_seam_hardness: Pressure,
    pub(super) pick_upgrade_native: Mass,
    pub(super) crank_upgrade_native: Mass,
    pub(super) concurrent_soft_mass: Mass,
    pub(super) native_surplus: Mass,
    pub(super) raw: deep_hearth::inventory::StockpileId,
    pub(super) shaped: deep_hearth::inventory::StockpileId,
    pub(super) ore_storage: deep_hearth::inventory::StockpileId,
    pub(super) hard_ore_storage: deep_hearth::inventory::StockpileId,
    pub(super) refined_clue_storage: deep_hearth::inventory::StockpileId,
    pub(super) native_storage: deep_hearth::inventory::StockpileId,
    pub(super) crushed_storage: deep_hearth::inventory::StockpileId,
    pub(super) separation_residue_storage: deep_hearth::inventory::StockpileId,
    pub(super) visible_clue_requests: [MiningTargetRequest; 4],
    pub(super) soft_ore_target: MiningTargetRequest,
    pub(super) hard_ore_target: MiningTargetRequest,
    pub(super) native_target: MiningTargetRequest,
    pub(super) trace_target: MiningTargetRequest,
    pub(super) refined_clue_sample_mass: Mass,
}

pub(super) fn setup_progression_world(
    registries: &Registries,
    seed: u64,
    deferred_trace_refinement: bool,
    ore_opportunity_batch_budget: u64,
) -> ProgressionWorldSetup {
    assert!(
        ore_opportunity_batch_budget >= SHALLOW_OPPORTUNITY_MIN_BATCHES,
        "primitive progression opportunity budget must leave room for discovery, convergence, and at least one repeated-work cycle"
    );
    let mined_mass = progression_mining_mass(registries, seed);
    let crushed_storage_capacity = multiply_mass(
        mined_mass,
        MAX_STEADY_STATE_CRUSH_CYCLES + 2,
        "crushed-storage capacity",
    );
    let soft_ore_surplus = Mass::from_milligrams(
        1 + mix64(seed ^ 0x534F_4654_5F4F_5245) % mined_mass.milligrams().max(1),
    );
    let hard_ore_surplus = Mass::from_milligrams(
        mined_mass.milligrams().div_ceil(2)
            + mix64(seed ^ 0x4841_5244_5F4F_5245) % (mined_mass.milligrams() + 1),
    );
    // Geological opportunity is finite independently of the probe's repeat limit. Maintained worlds
    // deliberately carry a deep reserve so they can prove automation payback. Organic worlds use a
    // smaller seed-derived budget, allowing the same machinery to be physically useful without
    // guaranteeing that local opportunity is large enough to repay its setup attention.
    let soft_ore_deposit_mass = multiply_mass(
        mined_mass,
        ore_opportunity_batch_budget,
        "soft-ore concurrent-work reserve",
    )
    .checked_add(soft_ore_surplus)
    .unwrap_or_else(|| panic!("primitive progression soft-ore reserve mass overflowed"));
    let hard_ore_deposit_mass = multiply_mass(
        mined_mass,
        ore_opportunity_batch_budget,
        "hard-ore concurrent-work reserve",
    )
    .checked_add(hard_ore_surplus)
    .unwrap_or_else(|| panic!("primitive progression hard-ore reserve mass overflowed"));
    // The actor may mine while the crusher runs. Size staging capacity from the finite world reserve
    // so the experiment measures authored resource limits; logistics and haulage are outside this
    // slice.
    let ore_storage_capacity = soft_ore_deposit_mass
        .checked_add(hard_ore_deposit_mass)
        .unwrap_or_else(|| panic!("primitive progression ore staging capacity overflowed"));
    let ore_copper_ppm = 450_000 + (mix64(seed ^ 0x5052_4F47_4752_4144) % 300_001) as u32;
    let soft_gangue_clay_share_ppm = u32::try_from(mix64(seed ^ 0x534F_4654_5F47_414E) % 750_001)
        .unwrap_or_else(|_| unreachable!("bounded soft-ore gangue variation fits u32"));
    // Hardness is an access constraint, not a promise of grade. A difficult seam can be excellent,
    // mediocre, or disappointing relative to easier ore. The player must buy access from bounded
    // evidence, then reassess the extracted sample instead of receiving a guaranteed jackpot.
    let hard_ore_copper_ppm = 500_000 + (mix64(seed ^ 0x4841_5244_5F47_5244) % 400_001) as u32;
    let hard_gangue_clay_share_ppm = u32::try_from(mix64(seed ^ 0x4841_5244_5F47_414E) % 750_001)
        .unwrap_or_else(|_| unreachable!("bounded hard-ore gangue variation fits u32"));
    let trace_copper_ppm = if deferred_trace_refinement {
        50_000 + (mix64(seed ^ 0x5452_4143_455F_4752) % 40_001) as u32
    } else {
        // A second legitimate information topology for organic worlds: cheap field inspection can
        // resolve this low-value occurrence immediately, but its entire evidence envelope remains
        // below the bulk ore's conservative lower bound. The actor can therefore rule it out as a
        // processing feed without paying for a redundant detailed survey or extraction sample.
        125_000 + (mix64(seed ^ 0x5452_4143_455F_4752) % 75_001) as u32
    };
    let trace_gangue_clay_share_ppm = u32::try_from(mix64(seed ^ 0x5452_4143_5F47_414E) % 750_001)
        .unwrap_or_else(|_| unreachable!("bounded trace-ore gangue variation fits u32"));
    let PrimitiveMaterialPlan {
        raw_inputs,
        raw_capacity,
        shaped_capacity,
        native_copper: total_native_copper,
    } = primitive_material_plan(registries);
    let raw_seed_inputs = raw_inputs
        .into_iter()
        .enumerate()
        .map(|(index, (commodity, required))| {
            let maximum_extra = required.milligrams().div_ceil(2).max(1);
            let extra = Mass::from_milligrams(
                1 + mix64(seed ^ 0x5241_575F_5355_5250 ^ index as u64) % maximum_extra,
            );
            let seeded = required
                .checked_add(extra)
                .unwrap_or_else(|| panic!("primitive progression raw-material surplus overflowed"));
            (commodity, seeded)
        })
        .collect::<Vec<_>>();
    let raw_seed_capacity = raw_seed_inputs
        .iter()
        .try_fold(Mass::ZERO, |total, (_, mass)| total.checked_add(*mass))
        .unwrap_or_else(|| panic!("primitive progression raw-material capacity overflowed"));
    let raw_surplus = raw_seed_capacity
        .checked_sub(raw_capacity)
        .unwrap_or_else(|| unreachable!("seeded raw material includes every required input"));
    let stone_pick_batch_limit = stone_pick_mining_batch_limit(registries);
    let (stone_hardness_limit, reinforced_hardness_limit, hard_seam_hardness) =
        mining_hardness_limits(registries);
    let native_seam_hardness =
        Pressure::from_pascals(stone_hardness_limit.pascals().div_ceil(2).max(1));
    let pick_upgrade_native =
        native_input_for_upgrade(registries, EQUIPMENT_COPPER_REINFORCED_PICK);
    let crank_upgrade_native =
        native_input_for_upgrade(registries, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK);
    assert_eq!(
        pick_upgrade_native, crank_upgrade_native,
        "primitive competing copper upgrades must require the same scarce native-copper investment"
    );
    let two_upgrade_native = pick_upgrade_native
        .checked_add(crank_upgrade_native)
        .unwrap_or_else(|| panic!("primitive two-upgrade native-copper requirement overflowed"));
    assert_eq!(
        two_upgrade_native, total_native_copper,
        "primitive material plan must provision both sequential copper upgrades"
    );
    assert!(
        pick_upgrade_native.milligrams() > 1,
        "primitive scarce-copper episode requires a nontrivial upgrade parcel"
    );
    let maximum_sample_mg = stone_pick_batch_limit
        .milligrams()
        .min(pick_upgrade_native.milligrams() / 2)
        .max(1);
    let minimum_sample_mg = maximum_sample_mg.div_ceil(2);
    let refined_clue_sample_mass = Mass::from_milligrams(
        minimum_sample_mg
            + mix64(seed ^ 0x5341_4D50_4C45_4D47) % (maximum_sample_mg - minimum_sample_mg + 1),
    );
    // Replenish in a normal owned-tool batch, not a tiny copper-upgrade parcel.
    // mined_mass is bounded by the stone pick, so both sequence branches can admit it.
    let concurrent_soft_mass = mined_mass;
    assert!(concurrent_soft_mass <= stone_pick_batch_limit);
    let native_surplus = Mass::from_milligrams(
        1 + mix64(seed ^ 0x4E41_5449_5645_5355) % (pick_upgrade_native.milligrams() - 1),
    );
    let native_deposit_mass = pick_upgrade_native
        .checked_add(native_surplus)
        .unwrap_or_else(|| panic!("primitive progression native-copper reserve overflowed"));

    let mut state = AppState::new();
    let raw = add_solid_stockpile(&mut state, raw_seed_capacity);
    let shaped = add_solid_stockpile(&mut state, shaped_capacity);
    let ore_storage = add_solid_stockpile(&mut state, ore_storage_capacity);
    let hard_ore_storage = add_solid_stockpile(&mut state, mined_mass);
    let refined_clue_storage = add_solid_stockpile(&mut state, refined_clue_sample_mass);
    // Output staging is sized for the bounded primitive-processing horizon, not merely the first
    // two 20 g upgrades. Later reinvestment can legitimately route a reinforced-separator batch
    // through these same pre-admission stockpiles without manufacturing storage after play begins.
    let native_storage = add_solid_stockpile(&mut state, crushed_storage_capacity);
    let crushed_storage = add_solid_stockpile(&mut state, crushed_storage_capacity);
    let separation_residue_storage = add_solid_stockpile(&mut state, crushed_storage_capacity);
    for (commodity, mass) in raw_seed_inputs {
        seed_lot(
            registries,
            &mut state,
            raw,
            commodity,
            mass,
            ROOM_TEMPERATURE,
        );
    }
    let clue_slots = varied_four_way_order(seed ^ 0x434C_5545_5F4C_4159);
    // Local clue locations are actor-visible starting facts. Build that input independently from the
    // hidden assignment of geological roles to those locations so setup truth cannot choose the
    // actor's search candidates or their order.
    let visible_clue_requests: [MiningTargetRequest; 4] = std::array::from_fn(|slot| {
        MiningTargetRequest::new(progression_clue_bounds(slot), MATERIAL_COPPER)
    });
    let soft_ore_bounds = progression_clue_bounds(clue_slots[0]);
    let ore_composition = copper_ore_composition(ore_copper_ppm, soft_gangue_clay_share_ppm);
    let hard_ore_composition =
        copper_ore_composition(hard_ore_copper_ppm, hard_gangue_clay_share_ppm);
    seed_geological_deposit(
        registries,
        &mut state,
        GeologicalDepositSeed::new(
            soft_ore_bounds,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            soft_ore_deposit_mass,
            ROOM_TEMPERATURE,
            stone_hardness_limit,
            ore_composition,
        ),
    );
    let soft_ore_target = MiningTargetRequest::new(soft_ore_bounds, MATERIAL_COPPER);
    let hard_ore_bounds = progression_clue_bounds(clue_slots[1]);
    seed_geological_deposit(
        registries,
        &mut state,
        GeologicalDepositSeed::new(
            hard_ore_bounds,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            hard_ore_deposit_mass,
            ROOM_TEMPERATURE,
            hard_seam_hardness,
            hard_ore_composition,
        ),
    );
    let hard_ore_target = MiningTargetRequest::new(hard_ore_bounds, MATERIAL_COPPER);
    let native_bounds = progression_clue_bounds(clue_slots[2]);
    seed_geological_deposit(
        registries,
        &mut state,
        GeologicalDepositSeed::new(
            native_bounds,
            CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
            native_deposit_mass,
            ROOM_TEMPERATURE,
            native_seam_hardness,
            MaterialComposition::pure(MATERIAL_COPPER),
        ),
    );
    let native_target = MiningTargetRequest::new(native_bounds, MATERIAL_COPPER);
    let trace_bounds = progression_clue_bounds(clue_slots[3]);
    let trace_composition = copper_ore_composition(trace_copper_ppm, trace_gangue_clay_share_ppm);
    seed_geological_deposit(
        registries,
        &mut state,
        GeologicalDepositSeed::new(
            trace_bounds,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            refined_clue_sample_mass
                .checked_add(refined_clue_sample_mass)
                .unwrap_or_else(|| panic!("primitive trace-copper reserve mass overflowed")),
            ROOM_TEMPERATURE,
            stone_hardness_limit,
            trace_composition,
        ),
    );
    let trace_target = MiningTargetRequest::new(trace_bounds, MATERIAL_COPPER);
    // Finish every fixture-only world mutation before admitting the player. From this point onward,
    // the episode may only use production-visible observations and canonical player/runtime actions.
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("primitive progression survival setup failed: {error}"));

    ProgressionWorldSetup {
        state,
        mined_mass,
        soft_ore_deposit_mass,
        hard_ore_deposit_mass,
        ore_copper_ppm,
        hard_ore_copper_ppm,
        trace_copper_ppm,
        raw_surplus,
        stone_pick_batch_limit,
        stone_hardness_limit,
        reinforced_hardness_limit,
        hard_seam_hardness,
        pick_upgrade_native,
        crank_upgrade_native,
        concurrent_soft_mass,
        native_surplus,
        raw,
        shaped,
        ore_storage,
        hard_ore_storage,
        refined_clue_storage,
        native_storage,
        crushed_storage,
        separation_residue_storage,
        visible_clue_requests,
        soft_ore_target,
        hard_ore_target,
        native_target,
        trace_target,
        refined_clue_sample_mass,
    }
}
