//! Content-relative world generation and provider discovery for primitive liberation.

use deep_hearth::capability::CapabilityValue;
use deep_hearth::content::{
    ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
    EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR, EQUIPMENT_STONE_ROTARY_QUERN,
    EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN, PROCESS_CONCENTRATE_COPPER, PROCESS_CRUSH_ORE,
    PROCESS_FINE_GRIND_SCREEN_OVERSIZE, PROCESS_GRIND_CRUSHED_ORE, PROCESS_SCREEN_CRUSHED_ORE,
};
use deep_hearth::core::quantity::{Mass, Power};
use deep_hearth::energy::EnergyCarrier;
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::labor::ManualPowerMethodId;
use deep_hearth::maintenance::Condition;
use deep_hearth::ore_processing::{
    project_powered_ore_replenished_batch_capacity, resolve_representable_screening_mass,
};
use deep_hearth::registry::Registries;

use super::inherited_condition::healthy_used_equipment_condition;
use super::seed::mix64;

const PRIMITIVE_LIBERATION_CAMPAIGN_BATCHES: u64 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LiberationGenerationRole {
    MaintainedAnchor,
    MaintainedCoverage,
    Organic,
}

impl LiberationGenerationRole {
    const fn is_maintained(self) -> bool {
        matches!(self, Self::MaintainedAnchor | Self::MaintainedCoverage)
    }
}

pub(super) fn ordinary_manual_electrical_power_providers(
    registries: &Registries,
) -> Vec<(ManualPowerMethodId, EquipmentDefinitionId, Power)> {
    registries
        .labor()
        .manual_power_definitions()
        .filter(|method| method.carrier() == EnergyCarrier::Electrical)
        .flat_map(|method| {
            registries
                .equipment()
                .definitions()
                .filter(|equipment| equipment.has_authored_acquisition_edge())
                .filter_map(move |equipment| {
                    match equipment
                        .capabilities()
                        .get_capability(method.power_capability())
                    {
                        Some(CapabilityValue::Power(power)) if !power.is_zero() => {
                            Some((method.id(), equipment.id(), power))
                        }
                        Some(_) | None => None,
                    }
                })
        })
        .collect()
}

pub(super) fn inherited_progression_condition(
    registries: &Registries,
    definition: EquipmentDefinitionId,
    seed: u64,
    role: LiberationGenerationRole,
    salt: u64,
) -> Condition {
    if role.is_maintained() {
        return Condition::PRISTINE;
    }
    healthy_used_equipment_condition(
        registries,
        definition,
        mix64(seed ^ 0x4C49_4245_494E_4845 ^ salt),
    )
}

fn disclosed_campaign_batches(
    seed: u64,
    role: LiberationGenerationRole,
    manual_fallback_coverage: bool,
) -> u64 {
    match role {
        LiberationGenerationRole::MaintainedAnchor => PRIMITIVE_LIBERATION_CAMPAIGN_BATCHES,
        LiberationGenerationRole::MaintainedCoverage if manual_fallback_coverage => 1,
        LiberationGenerationRole::MaintainedCoverage => PRIMITIVE_LIBERATION_CAMPAIGN_BATCHES,
        LiberationGenerationRole::Organic => {
            let stratum = seed & 0b11;
            let width = PRIMITIVE_LIBERATION_CAMPAIGN_BATCHES.div_ceil(4);
            let lower = 1 + stratum * width;
            let upper = lower
                .saturating_add(width.saturating_sub(1))
                .min(PRIMITIVE_LIBERATION_CAMPAIGN_BATCHES);
            lower + mix64(seed ^ 0x4C49_4245_5248_4F52) % (upper - lower + 1)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PrimitiveLiberationWorldParameters {
    pub(super) planned_batches: u64,
    pub(super) batch_mass: Mass,
    pub(super) copper_ppm: u32,
    pub(super) clay_share_ppm: u32,
}

pub(super) fn primitive_liberation_batch_ceiling(
    registries: &Registries,
    seed: u64,
    role: LiberationGenerationRole,
) -> Mass {
    [
        (
            PROCESS_CRUSH_ORE,
            EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
            inherited_progression_condition(
                registries,
                EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
                seed,
                role,
                0x4352_5553_4800_0001,
            ),
        ),
        (
            PROCESS_GRIND_CRUSHED_ORE,
            EQUIPMENT_STONE_ROTARY_QUERN,
            Condition::PRISTINE,
        ),
        (
            PROCESS_SCREEN_CRUSHED_ORE,
            EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
            Condition::PRISTINE,
        ),
        (
            PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
            EQUIPMENT_STONE_ROTARY_QUERN,
            Condition::PRISTINE,
        ),
        (
            PROCESS_CONCENTRATE_COPPER,
            EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
            inherited_progression_condition(
                registries,
                EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
                seed,
                role,
                0x5345_5041_5241_544F,
            ),
        ),
    ]
    .into_iter()
    .map(|(process, equipment, condition)| {
        project_powered_ore_replenished_batch_capacity(
            registries,
            process,
            equipment,
            condition,
            ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
        )
        .unwrap_or_else(|error| {
            panic!("primitive liberation authored batch projection failed: {error}")
        })
    })
    .min()
    .unwrap_or_else(|| unreachable!("primitive liberation has a nonempty processing route"))
}

pub(super) fn primitive_liberation_world_parameters(
    registries: &Registries,
    seed: u64,
    role: LiberationGenerationRole,
    manual_fallback_coverage: bool,
) -> PrimitiveLiberationWorldParameters {
    let route_ceiling = primitive_liberation_batch_ceiling(registries, seed, role).milligrams();
    let minimum_batch = (route_ceiling / 2).max(1);
    let maximum_batch = route_ceiling
        .checked_mul(4)
        .unwrap_or_else(|| panic!("primitive liberation route ceiling scaling overflowed"))
        / 5;
    let maximum_batch = maximum_batch.max(minimum_batch);
    let requested_batch_mass = Mass::from_milligrams(
        minimum_batch + mix64(seed ^ 0x4C49_4245_5241_5445) % (maximum_batch - minimum_batch + 1),
    );
    let grinding = registries
        .ore_processing()
        .get_comminution(PROCESS_GRIND_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("primitive liberation grinding definition disappeared"));
    let screening = registries
        .ore_processing()
        .get_screening(PROCESS_SCREEN_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("primitive liberation screening definition disappeared"));
    let batch_mass = resolve_representable_screening_mass(
        screening,
        grinding.output_particle_size_distribution(),
        requested_batch_mass,
    )
    .unwrap_or_else(|error| panic!("primitive liberation batch planning failed: {error}"));
    assert!(
        !batch_mass.is_zero(),
        "primitive liberation generated no representable batch"
    );
    PrimitiveLiberationWorldParameters {
        planned_batches: disclosed_campaign_batches(seed, role, manual_fallback_coverage),
        batch_mass,
        copper_ppm: 300_000 + (mix64(seed ^ 0x4C49_4245_5243_5550) % 300_001) as u32,
        clay_share_ppm: (mix64(seed ^ 0x4C49_4245_5243_4C41) % 650_001) as u32,
    }
}
