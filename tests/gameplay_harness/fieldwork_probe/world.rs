//! Deterministic fixture construction for an ordinary-system spatial-proxy fieldwork episode.

use deep_hearth::content::gameplay_fixture::{
    GeologicalDepositSeed, seed_geological_deposit, seed_lot,
};
use deep_hearth::content::{
    FORM_NATIVE_METAL, FORM_ORE, MATERIAL_COPPER, PROCESS_HAND_SORT_NATIVE_COPPER,
    PROSPECTING_LOCAL_TRANSECT,
};
use deep_hearth::core::quantity::{Mass, Pressure};
use deep_hearth::core::state::AppState;
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::CommodityKey;
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
    pub(super) native_copper: CommodityKey,
}

/// Fixture truth retained only for post-action audit/reporting.
///
/// This type is deliberately separate from [`FieldworkWorld`]. Actor planning must operate on the
/// actor-visible spatial-proxy world plus acquired runtime evidence and must never receive exact
/// geological truth.
#[derive(Clone, Copy)]
pub(super) struct FieldworkFixtureDiagnostics {
    pub(super) geology_label: &'static str,
    pub(super) excavation_hardness: Pressure,
    pub(super) deposit_mass: Mass,
}

#[derive(Clone, Copy)]
struct FieldworkGeologyProfile {
    hardness: Pressure,
    copper_ppm: u32,
    clay_share_ppm: u32,
}

/// Controlled world generation, independent of demand and actor policy. Reserve classes are scaled
/// from the current ordinary quarry batch so retuning extraction capacity does not silently turn
/// every organic site into the same tool regime. Exact reserve stays hidden from the actor.
pub(super) const FIELDWORK_SHALLOW_SUPPLY_MIN_PPM: u64 = 1_200_000;
pub(super) const FIELDWORK_SHALLOW_SUPPLY_MAX_PPM: u64 = 1_800_000;
pub(super) const FIELDWORK_COMMON_SUPPLY_MIN_PPM: u64 = 8_000_000;
pub(super) const FIELDWORK_COMMON_SUPPLY_MAX_PPM: u64 = 16_000_000;
// Rich sites must extend beyond the current ordinary bulk-order envelope. Otherwise acquired
// reserve evidence can trim every organic project back below the heavy-tool crossover and the
// evaluator only experiences that investment in maintained scripts.
pub(super) const FIELDWORK_BULK_SUPPLY_MIN_PPM: u64 = 64_000_000;
pub(super) const FIELDWORK_BULK_SUPPLY_MAX_PPM: u64 = 112_000_000;

pub(super) fn scaled_fieldwork_supply(base_batch: Mass, scale_ppm: u64) -> Mass {
    Mass::from_milligrams(
        u128::from(base_batch.milligrams())
            .checked_mul(u128::from(scale_ppm))
            .map(|scaled| scaled / 1_000_000)
            .and_then(|scaled| u64::try_from(scaled).ok())
            .unwrap_or_else(|| panic!("fieldwork reserve scale overflowed")),
    )
}

pub(super) fn fieldwork_supply(registries: &Registries, seed: u64) -> Mass {
    let base_batch = fieldwork_mining_limits(registries).base_quarry_batch;
    let variation = mix64(seed ^ 0x4649_454C_4452_5356);
    // Demand uses the lower two replay bits. Reserve scale uses the next two, so an exploratory
    // sample can span both dimensions without correlating the player's goal with geological truth.
    // Exact reserve remains high-entropy and the actor only receives the normal acquired estimate.
    let reserve_stratum = (seed >> 2) & 0b11;
    let shallow = reserve_stratum == 0;
    let bulk = reserve_stratum == 2;
    let scale_ppm = if shallow {
        FIELDWORK_SHALLOW_SUPPLY_MIN_PPM
            + variation % (FIELDWORK_SHALLOW_SUPPLY_MAX_PPM - FIELDWORK_SHALLOW_SUPPLY_MIN_PPM + 1)
    } else if bulk {
        FIELDWORK_BULK_SUPPLY_MIN_PPM
            + variation % (FIELDWORK_BULK_SUPPLY_MAX_PPM - FIELDWORK_BULK_SUPPLY_MIN_PPM + 1)
    } else {
        FIELDWORK_COMMON_SUPPLY_MIN_PPM
            + variation % (FIELDWORK_COMMON_SUPPLY_MAX_PPM - FIELDWORK_COMMON_SUPPLY_MIN_PPM + 1)
    };
    scaled_fieldwork_supply(base_batch, scale_ppm)
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

pub(super) fn fieldwork_followup_opportunities(
    registries: &Registries,
    seed: u64,
) -> [Option<Mass>; 6] {
    followup_site_seeds(seed).map(|site_seed| {
        // Neighboring search areas are not guaranteed to contain the requested resource. Keep the
        // outcome independent of demand, equipment, and actor policy so paid fieldwork can reveal
        // a real dead end without turning the primary known opportunity into a lottery.
        (!mix64(site_seed ^ 0x4241_5252_454E_5349).is_multiple_of(4))
            .then(|| fieldwork_supply(registries, site_seed))
    })
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
    stratify_hardness: bool,
) -> (u64, &'static str, FieldworkGeologyProfile) {
    let hardness_tier = if stratify_hardness {
        // Demand and reserve already own the low four replay bits. Fold those coarse physical
        // strata into hardness so one bounded exploratory sample cannot accidentally miss the
        // live quarry-investment regime. Exact hardness, grade, location, and follow-up geology
        // remain high-entropy and hidden until the actor acquires geological evidence.
        let demand_stratum = seed & 0b11;
        let reserve_stratum = (seed >> 2) & 0b11;
        (demand_stratum + reserve_stratum.saturating_mul(2)) % 3
    } else {
        mix64(seed ^ 0x4649_454C_4448_4152) % 3
    };
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
            let sampled_gap = if stratify_hardness {
                // Keep the bounded exploratory witness away from the capability edge. The ordinary
                // detailed survey reports 50 MPa hardness buckets; sampling the lower half of the
                // live 500-600 MPa reinforced-quarry band leaves the actor's conservative upper
                // estimate inside the tool envelope instead of creating a hidden-truth-only niche.
                (gap / 2).max(1)
            } else {
                gap
            };
            (
                "quarry-reinforcement",
                Pressure::from_pascals(
                    base_pa + 1 + mix64(seed ^ 0x4649_454C_444D_4544) % sampled_gap,
                ),
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

fn build_fieldwork_world_inner(
    registries: &Registries,
    seed: u64,
    requested_mine_mass: Mass,
    deposit_mass: Mass,
    stratify_hardness: bool,
) -> (FieldworkWorld, FieldworkFixtureDiagnostics) {
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
    let (hardness_tier, geology_label, profile) =
        geology_profile(seed, mining_limits, stratify_hardness);
    let mut state = AppState::new();
    let (mut raw_opportunity, parts_capacity) = fieldwork_raw_opportunity(registries);
    let native_copper = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    let copper_rich = hardness_tier != 0 || !mix64(seed ^ 0x4649_454C_445F_4355).is_multiple_of(2);
    if !copper_rich {
        raw_opportunity.remove(&native_copper);
    }
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
    let followup_opportunities = fieldwork_followup_opportunities(registries, seed);
    // Follow-up search areas are independent geological opportunities. Some contain no copper at
    // all; productive areas vary reserve, hardness, and grade. Search therefore reveals the world
    // instead of receiving a harness guarantee that every reroute is productive.
    for ((start_x, site_seed), opportunity) in FOLLOWUP_CHANNEL_STARTS
        .into_iter()
        .zip(followup_seeds)
        .zip(followup_opportunities)
    {
        let Some(supply) = opportunity else {
            continue;
        };
        let hidden = hidden_location(
            site_seed,
            channel_voxels,
            0x4649_454C_4453_4348,
            0x4649_454C_4453_534C,
        );
        let followup_profile = geology_profile(
            mix64(site_seed ^ 0x5349_5445_5052_4F46),
            mining_limits,
            false,
        )
        .2;
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
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("fieldwork survival setup failed: {error}"));
    super::spatial_proxy_runtime::assert_spatial_proxy_runtime_ready(
        registries,
        &state,
        "fieldwork",
    );

    (
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
            native_copper,
        },
        FieldworkFixtureDiagnostics {
            geology_label,
            excavation_hardness: profile.hardness,
            deposit_mass,
        },
    )
}

/// Builds only the actor-visible spatial-proxy world. Tests of player choices should prefer this
/// surface so hidden fixture truth cannot accidentally enter policy code.
#[cfg(test)]
#[allow(
    dead_code,
    reason = "the focused fieldwork test target excludes owner contracts; the separate fieldwork contract target uses this actor-only fixture surface"
)]
pub(super) fn build_fieldwork_world(
    registries: &Registries,
    seed: u64,
    requested_mine_mass: Mass,
    deposit_mass: Mass,
) -> FieldworkWorld {
    build_fieldwork_world_inner(registries, seed, requested_mine_mass, deposit_mass, false).0
}

/// Builds the actor-visible spatial-proxy world plus hidden truth for post-action evidence audits.
pub(super) fn build_fieldwork_world_with_diagnostics(
    registries: &Registries,
    seed: u64,
    requested_mine_mass: Mass,
    deposit_mass: Mass,
    stratify_hardness: bool,
) -> (FieldworkWorld, FieldworkFixtureDiagnostics) {
    build_fieldwork_world_inner(
        registries,
        seed,
        requested_mine_mass,
        deposit_mass,
        stratify_hardness,
    )
}
