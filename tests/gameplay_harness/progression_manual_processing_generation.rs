//! Manual primitive ore-processing setup generation without episode execution.

use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_PICK, PROCESS_HAND_BREAK_ORE, PROCESS_HAND_SORT_NATIVE_COPPER,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::material::COMPOSITION_PARTS_PER_MILLION;
use deep_hearth::registry::Registries;

use super::progression_upgrade_planning::native_input_for_upgrade;
use super::seed::mix64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ManualProcessingSetup {
    pub(super) ore_mass: Mass,
    pub(super) copper_ppm: u32,
    pub(super) clay_share_ppm: u32,
}

pub(super) fn manual_processing_setup(registries: &Registries, seed: u64) -> ManualProcessingSetup {
    let breaking = registries
        .ore_processing()
        .get_manual_comminution(PROCESS_HAND_BREAK_ORE)
        .unwrap_or_else(|| panic!("manual processing fallback lost its hand-breaking definition"));
    let sorting = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("manual processing fallback lost its hand-sorting definition"));
    let reinforcement = native_input_for_upgrade(registries, EQUIPMENT_COPPER_REINFORCED_PICK);
    let maximum_mass = breaking.max_batch_mass().min(sorting.max_batch_mass());
    assert!(
        !maximum_mass.is_zero(),
        "manual processing fallback requires a nonzero legal shared batch envelope"
    );
    let minimum_mass_mg: u64 = (u128::from(maximum_mass.milligrams()) * 55 / 100)
        .max(1)
        .try_into()
        .unwrap_or_else(|_| unreachable!("scaled bounded manual batch fits u64"));
    let ore_mass_mg = minimum_mass_mg
        + mix64(seed ^ 0x4841_4E44_4D41_5353) % (maximum_mass.milligrams() - minimum_mass_mg + 1);
    let minimum_copper_ppm = sorting
        .minimum_homogeneous_constituent_ppm_for_target_recovery(
            reinforcement,
            Mass::from_milligrams(ore_mass_mg),
        )
        .unwrap_or_else(|| {
            panic!("no legal homogeneous assay can recover the required primitive reinforcement")
        });
    assert!(
        minimum_copper_ppm < COMPOSITION_PARTS_PER_MILLION,
        "no single legal manual-processing batch can recover the copper required by the real primitive upgrade"
    );
    let maximum_copper_ppm = minimum_copper_ppm
        .checked_add(250_000)
        .unwrap_or_else(|| unreachable!("bounded composition variation fits u32"))
        .min(COMPOSITION_PARTS_PER_MILLION - 1);
    let copper_ppm = minimum_copper_ppm
        + u32::try_from(
            mix64(seed ^ 0x4841_4E44_4752_4144)
                % u64::from(maximum_copper_ppm - minimum_copper_ppm + 1),
        )
        .unwrap_or_else(|_| unreachable!("bounded composition variation fits u32"));
    let clay_share_ppm = u32::try_from(mix64(seed ^ 0x4841_4E44_4741_4E47) % 750_001)
        .unwrap_or_else(|_| unreachable!("bounded gangue-share variation fits u32"));
    ManualProcessingSetup {
        ore_mass: Mass::from_milligrams(ore_mass_mg),
        copper_ppm,
        clay_share_ppm,
    }
}
