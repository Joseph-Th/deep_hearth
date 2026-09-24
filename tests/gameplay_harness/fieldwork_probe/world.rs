//! Deterministic fixture construction for an ordinary fieldwork episode.

use deep_hearth::content::gameplay_fixture::{
    GeologicalDepositSeed, seed_geological_deposit, seed_lot,
};
use deep_hearth::content::{
    FORM_NATIVE_METAL, FORM_ORE, MATERIAL_COPPER, PROCESS_HAND_SORT_NATIVE_COPPER,
    PROSPECTING_LOCAL_TRANSECT,
};
use deep_hearth::core::quantity::{AggregateMass, Mass, Pressure};
use deep_hearth::core::state::AppState;
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::survival::initialize_player_survival;

use super::super::environment::ROOM_TEMPERATURE;
use super::super::inventory_support::add_solid_stockpile;
use super::super::ore_fixture::copper_ore_composition;
use super::super::seed::mix64;
use super::planning::{
    FieldworkMiningLimits, fieldwork_mining_limits, fieldwork_raw_opportunity, multiplied_mass,
};
use super::survey::{CHANNEL_COUNT, CHANNEL_START_X, FOLLOWUP_CHANNEL_STARTS, horizontal_region};

pub(super) struct FieldworkWorld {
    pub(super) state: AppState,
    pub(super) raw: StockpileId,
    pub(super) parts: StockpileId,
    pub(super) destination: StockpileId,
    pub(super) followup_destination: StockpileId,
    pub(super) recovery_crushed: StockpileId,
    pub(super) recovery_residue: StockpileId,
    pub(super) channel_voxels: i64,
    pub(super) mining_limits: FieldworkMiningLimits,
    pub(super) geology_label: &'static str,
    pub(super) excavation_hardness: Pressure,
    pub(super) copper_rich: bool,
    pub(super) starting_native_copper: Mass,
    pub(super) native_copper: CommodityKey,
    pub(super) matter_before: AggregateMass,
}

#[derive(Clone, Copy)]
struct FieldworkGeologyProfile {
    hardness: Pressure,
    copper_ppm: u32,
    clay_share_ppm: u32,
}

/// Controlled world generation, independent of demand and tool capabilities. One quarter of mixed
/// worlds are shallow, while a sparse large-reserve tail gives bulk extraction tools a legitimate
/// organic opportunity instead of making every non-shallow site the same 4-8 kg scale. Exact
/// reserve stays hidden from the actor and demand never influences which reserve class is generated.
pub(super) fn fieldwork_supply(seed: u64) -> Mass {
    let variation = mix64(seed ^ 0x4649_454C_4452_5356);
    let shallow = mix64(seed ^ 0x4649_454C_4453_5554) % 4 == 1;
    let bulk = !shallow && mix64(seed ^ 0x4649_454C_4442_554C).is_multiple_of(8);
    let milligrams = if shallow {
        600_000 + variation % 300_001
    } else if bulk {
        24_000_000 + variation % 8_000_001
    } else {
        4_000_000 + variation % 4_000_001
    };
    Mass::from_milligrams(milligrams)
}

const FOLLOWUP_SITE_SALTS: [u64; 6] = [
    0x4649_454C_4453_3252,
    0x4649_454C_4453_3352,
    0x4649_454C_4453_3452,
    0x4649_454C_4453_3552,
    0x4649_454C_4453_3652,
    0x4649_454C_4453_3752,
];

fn followup_site_seeds(seed: u64) -> [u64; 6] {
    FOLLOWUP_SITE_SALTS.map(|salt| mix64(seed ^ salt))
}

pub(super) fn fieldwork_followup_supplies(seed: u64) -> [Mass; 6] {
    followup_site_seeds(seed).map(fieldwork_supply)
}

fn hidden_location(
    seed: u64,
    channel_voxels: i64,
    channel_salt: u64,
    slot_salt: u64,
) -> (i64, i64) {
    let channel = i64::try_from(
        mix64(seed ^ channel_salt)
            % u64::try_from(CHANNEL_COUNT)
                .unwrap_or_else(|_| unreachable!("positive channel count fits u64")),
    )
    .unwrap_or_else(|_| unreachable!("fieldwork channel is bounded"));
    let slot = i64::try_from(
        mix64(seed ^ slot_salt)
            % u64::try_from(channel_voxels)
                .unwrap_or_else(|_| unreachable!("positive channel span fits u64")),
    )
    .unwrap_or_else(|_| unreachable!("fieldwork slot is bounded"));
    (channel, slot)
}

fn geology_profile(
    seed: u64,
    limits: FieldworkMiningLimits,
) -> (u64, &'static str, FieldworkGeologyProfile) {
    let hardness_tier = mix64(seed ^ 0x4649_454C_4448_4152) % 3;
    let base_pa = limits.base_quarry_hardness.pascals();
    let reinforced_quarry_pa = limits.reinforced_quarry_hardness.pascals();
    let reinforced_pick_pa = limits.reinforced_pick_hardness.pascals();
    let (label, hardness) = match hardness_tier {
        0 => {
            let floor = base_pa.saturating_mul(3) / 4;
            let span = base_pa - floor;
            (
                "quarry-soft",
                Pressure::from_pascals(floor + mix64(seed ^ 0x4649_454C_4453_4F46) % (span + 1)),
            )
        }
        1 => {
            let gap = reinforced_quarry_pa
                .checked_sub(base_pa)
                .unwrap_or_else(|| {
                    unreachable!("reinforced quarry hardness exceeds base hardness")
                });
            (
                "quarry-reinforcement",
                Pressure::from_pascals(base_pa + 1 + mix64(seed ^ 0x4649_454C_444D_4544) % gap),
            )
        }
        2 => {
            let gap = reinforced_pick_pa
                .checked_sub(reinforced_quarry_pa)
                .unwrap_or_else(|| {
                    unreachable!("reinforced pick hardness exceeds reinforced quarry hardness")
                });
            (
                "hard-pick-specialist",
                Pressure::from_pascals(
                    reinforced_quarry_pa + 1 + mix64(seed ^ 0x4649_454C_4448_4152) % gap,
                ),
            )
        }
        _ => unreachable!("three fieldwork hardness tiers are exhaustive"),
    };
    (
        hardness_tier,
        label,
        FieldworkGeologyProfile {
            hardness,
            copper_ppm: 350_000 + (mix64(seed ^ 0x4649_454C_4447_5241) % 300_001) as u32,
            clay_share_ppm: (mix64(seed ^ 0x4649_454C_4443_4C41) % 600_001) as u32,
        },
    )
}

fn seed_channel_deposit(
    registries: &Registries,
    state: &mut AppState,
    start_x: i64,
    channel_voxels: i64,
    hidden: (i64, i64),
    supply: Mass,
    profile: FieldworkGeologyProfile,
) {
    let region = horizontal_region(start_x + hidden.0 * channel_voxels + hidden.1, 1);
    seed_geological_deposit(
        registries,
        state,
        GeologicalDepositSeed::new(
            region,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            supply,
            ROOM_TEMPERATURE,
            profile.hardness,
            copper_ore_composition(profile.copper_ppm, profile.clay_share_ppm),
        ),
    );
}

pub(super) fn build_fieldwork_world(
    registries: &Registries,
    seed: u64,
    requested_mine_mass: Mass,
    deposit_mass: Mass,
) -> FieldworkWorld {
    assert!(!requested_mine_mass.is_zero());
    assert!(!deposit_mass.is_zero());
    let channel_voxels = i64::try_from(
        registries
            .labor()
            .get_prospecting(PROSPECTING_LOCAL_TRANSECT)
            .map(|definition| definition.maximum_region_voxels())
            .unwrap_or_else(|| panic!("fieldwork local-transect definition disappeared")),
    )
    .unwrap_or_else(|_| panic!("fieldwork transect span exceeds coordinate range"));
    assert!(channel_voxels > 0);

    let mining_limits = fieldwork_mining_limits(registries);
    let (hardness_tier, geology_label, profile) = geology_profile(seed, mining_limits);
    let mut state = AppState::new();
    let (mut raw_opportunity, parts_capacity) = fieldwork_raw_opportunity(registries);
    let native_copper = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    let copper_rich = hardness_tier != 0 || !mix64(seed ^ 0x4649_454C_445F_4355).is_multiple_of(2);
    if !copper_rich {
        raw_opportunity.remove(&native_copper);
    }
    let starting_native_copper = raw_opportunity
        .get(&native_copper)
        .copied()
        .unwrap_or(Mass::ZERO);
    let raw_capacity = raw_opportunity
        .values()
        .copied()
        .try_fold(Mass::ZERO, |total, mass| total.checked_add(mass))
        .unwrap_or_else(|| panic!("fieldwork raw opportunity capacity overflowed"));
    let raw = add_solid_stockpile(&mut state, raw_capacity);
    for (commodity, mass) in raw_opportunity {
        seed_lot(
            registries,
            &mut state,
            raw,
            commodity,
            mass,
            ROOM_TEMPERATURE,
        );
    }
    let parts = add_solid_stockpile(&mut state, parts_capacity);
    let destination = add_solid_stockpile(&mut state, requested_mine_mass);
    // Diagnostic known-site exploitation gets one bounded landing area reserved before actor
    // admission. Capacity depends only on the disclosed repeat order and fixed harness horizon,
    // never on hidden geological reserve.
    let followup_capacity = multiplied_mass(
        requested_mine_mass,
        super::FIELDWORK_KNOWN_SITE_REPEAT_HORIZON
            .checked_add(1)
            .unwrap_or_else(|| unreachable!("bounded fieldwork repeat horizon fits u64")),
        "known-site repeat plus reroute landing capacity",
    );
    let followup_destination = add_solid_stockpile(&mut state, followup_capacity);
    let manual_sorting = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("fieldwork recovery lost hand-sorting definition"));
    let recovery_buffer_capacity = manual_sorting.max_batch_mass();
    let recovery_crushed = add_solid_stockpile(&mut state, recovery_buffer_capacity);
    let recovery_residue = add_solid_stockpile(
        &mut state,
        multiplied_mass(
            recovery_buffer_capacity,
            u64::try_from(FOLLOWUP_CHANNEL_STARTS.len())
                .unwrap_or_else(|_| unreachable!("bounded recovery horizon fits u64")),
            "fieldwork recovery residue capacity",
        ),
    );

    let primary = hidden_location(
        seed,
        channel_voxels,
        0x4649_454C_4443_484E,
        0x4649_454C_4453_4C4F,
    );
    seed_channel_deposit(
        registries,
        &mut state,
        CHANNEL_START_X,
        channel_voxels,
        primary,
        deposit_mass,
        profile,
    );
    let followup_seeds = followup_site_seeds(seed);
    let followup_supplies = fieldwork_followup_supplies(seed);
    // Each new site is an independent geological opportunity. Reusing the primary profile here
    // made depletion look like a quantity-only problem and prevented the actor from encountering
    // a new hardness or grade after paying to search elsewhere. Six bounded follow-up sites keep
    // the ordinary 24 kg bulk order from being mechanically doomed by a three-site fixture cap.
    for ((start_x, site_seed), supply) in FOLLOWUP_CHANNEL_STARTS
        .into_iter()
        .zip(followup_seeds)
        .zip(followup_supplies)
    {
        let hidden = hidden_location(
            site_seed,
            channel_voxels,
            0x4649_454C_4453_4348,
            0x4649_454C_4453_534C,
        );
        let followup_profile =
            geology_profile(mix64(site_seed ^ 0x5349_5445_5052_4F46), mining_limits).2;
        seed_channel_deposit(
            registries,
            &mut state,
            start_x,
            channel_voxels,
            hidden,
            supply,
            followup_profile,
        );
    }
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("fieldwork initial matter audit failed: {error}"))
        .total();
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("fieldwork survival setup failed: {error}"));

    FieldworkWorld {
        state,
        raw,
        parts,
        destination,
        followup_destination,
        recovery_crushed,
        recovery_residue,
        channel_voxels,
        mining_limits,
        geology_label,
        excavation_hardness: profile.hardness,
        copper_rich,
        starting_native_copper,
        native_copper,
        matter_before,
    }
}
