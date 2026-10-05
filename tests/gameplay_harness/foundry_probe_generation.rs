//! Deterministic foundry probe input generation shared by probes and generator contracts.

use deep_hearth::content::{
    ENERGY_ELECTRICAL_BUFFER, ENERGY_THERMAL_SINK, EQUIPMENT_CASTING_MOLD,
    EQUIPMENT_ELECTRIC_FURNACE, MATERIAL_COPPER, PROCESS_CAST_PURE_COPPER,
    PROCESS_HEAT_MATERIAL_BATCH, PROCESS_MELT_PURE_COPPER,
};
use deep_hearth::core::quantity::{Energy, Mass, Temperature};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::{COMPOSITION_PARTS_PER_MILLION, FormId, MaterialComposition};
use deep_hearth::registry::Registries;
use deep_hearth::thermal::{calculate_fusion_heat, calculate_sensible_heat};

use super::environment::ROOM_TEMPERATURE;
use super::equipment_support::nominal_equipment_mass_capability;
use super::production_support::varied_healthy_condition;
use super::seed::mix64;

#[derive(Clone, Copy)]
pub(super) struct FoundrySetup {
    pub(super) mass: Mass,
    pub(super) feed_form: FormId,
    pub(super) preheat_target: Temperature,
    pub(super) furnace_condition: Condition,
    pub(super) mold_condition: Condition,
    pub(super) electrical_energy: Energy,
    pub(super) thermal_sink_energy: Energy,
}

pub(super) fn probe_setup(registries: &Registries, seed: u64) -> FoundrySetup {
    let melting = registries
        .thermal()
        .get_melting(PROCESS_MELT_PURE_COPPER)
        .unwrap_or_else(|| panic!("canonical melting definition disappeared"));
    let heating = registries
        .thermal()
        .get_sensible_heating(PROCESS_HEAT_MATERIAL_BATCH)
        .unwrap_or_else(|| panic!("canonical sensible-heating definition disappeared"));
    let casting = registries
        .thermal()
        .get_casting(PROCESS_CAST_PURE_COPPER)
        .unwrap_or_else(|| panic!("canonical casting definition disappeared"));
    let melt_maximum = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_ELECTRIC_FURNACE,
        melting.max_batch_mass_capability(),
    );
    let heat_maximum = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_ELECTRIC_FURNACE,
        heating.max_batch_mass_capability(),
    );
    let cast_maximum = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_CASTING_MOLD,
        casting.max_batch_mass_capability(),
    );
    let maximum = heat_maximum
        .milligrams()
        .min(melt_maximum.milligrams())
        .min(cast_maximum.milligrams());
    assert!(maximum > 0, "foundry probe requires a nonzero legal batch");
    let feed_forms = melting.solid_forms();
    let feed_form_count = u64::try_from(feed_forms.len())
        .unwrap_or_else(|_| panic!("foundry feed-form count exceeded u64"));
    let feed_index = usize::try_from(seed % feed_form_count)
        .unwrap_or_else(|_| panic!("foundry feed-form index exceeded usize"));
    let feed_form = feed_forms[feed_index];
    let minimum = maximum.div_ceil(2);
    let mass = Mass::from_milligrams(minimum + mix64(seed ^ 0xF0A1_DA7A) % (maximum - minimum + 1));
    let melting_point = registries
        .materials()
        .get_material(MATERIAL_COPPER)
        .and_then(|material| material.properties().thermal().melting_point())
        .unwrap_or_else(|| panic!("foundry probe copper melting point disappeared"));
    let ambient = ROOM_TEMPERATURE.millikelvin();
    let melting = melting_point.millikelvin();
    assert!(
        melting > ambient,
        "foundry probe requires copper to melt above room temperature"
    );
    let preheat_span = (melting - ambient) * 3 / 4;
    assert!(preheat_span > 0, "foundry preheat span must be nonzero");
    let preheat_offset =
        u32::try_from(1 + mix64(seed ^ 0x5448_4552_4D41_4C49) % u64::from(preheat_span))
            .unwrap_or_else(|_| panic!("foundry preheat offset exceeded u32"));
    let preheat_target = Temperature::from_millikelvin(ambient + preheat_offset);
    let composition = MaterialComposition::pure(MATERIAL_COPPER);
    let sensible = calculate_sensible_heat(
        registries.materials(),
        mass,
        &composition,
        ROOM_TEMPERATURE,
        melting_point,
    )
    .unwrap_or_else(|error| panic!("foundry probe sensible heating calculation failed: {error}"))
    .energy();
    let fusion = calculate_fusion_heat(registries.materials(), mass, MATERIAL_COPPER)
        .unwrap_or_else(|error| panic!("foundry probe fusion calculation failed: {error}"))
        .energy();
    let required_electrical = sensible
        .checked_add(fusion)
        .unwrap_or_else(|| panic!("foundry probe required electrical energy overflowed"));
    let electrical_capacity = registries
        .energy()
        .get_store(ENERGY_ELECTRICAL_BUFFER)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("foundry probe electrical-buffer definition disappeared"));
    assert!(
        electrical_capacity >= required_electrical,
        "foundry electrical buffer must remain capable of the maintained full-batch contract"
    );
    let energy_budget_ppm = 400_000 + (mix64(seed ^ 0x454C_4543_4845_4147) % 950_001) as u32;
    let electrical_budget = Energy::from_nanojoules(
        required_electrical
            .nanojoules()
            .checked_mul(u128::from(energy_budget_ppm))
            .map(|scaled| scaled / u128::from(COMPOSITION_PARTS_PER_MILLION))
            .unwrap_or_else(|| panic!("foundry electrical budget scaling overflowed")),
    );
    let electrical_energy = std::cmp::min(electrical_budget, electrical_capacity);
    let thermal_capacity = registries
        .energy()
        .get_store(ENERGY_THERMAL_SINK)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("foundry probe thermal-sink definition disappeared"));
    let thermal_roll = mix64(seed ^ 0x5448_4552_4D53_494F);
    let thermal_pressure_ppm = if thermal_roll.is_multiple_of(5) {
        (thermal_roll % 250_001) as u32
    } else if thermal_roll % 5 == 1 {
        900_000 + ((thermal_roll >> 8) % 100_001) as u32
    } else {
        500_000 + ((thermal_roll >> 8) % 350_001) as u32
    };
    let thermal_sink_energy = Energy::from_nanojoules(
        thermal_capacity
            .nanojoules()
            .checked_mul(u128::from(thermal_pressure_ppm))
            .map(|scaled| scaled / u128::from(COMPOSITION_PARTS_PER_MILLION))
            .unwrap_or_else(|| panic!("foundry thermal pressure scaling overflowed")),
    );
    FoundrySetup {
        mass,
        feed_form,
        preheat_target,
        furnace_condition: varied_healthy_condition(
            registries,
            EQUIPMENT_ELECTRIC_FURNACE,
            mix64(seed ^ 0x4655_524E_4143_4543),
        ),
        mold_condition: varied_healthy_condition(
            registries,
            EQUIPMENT_CASTING_MOLD,
            mix64(seed ^ 0x4D4F_4C44_434F_4E44),
        ),
        electrical_energy,
        thermal_sink_energy,
    }
}
