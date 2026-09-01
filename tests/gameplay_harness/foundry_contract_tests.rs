//! Cheap foundry generator contracts kept beside the focused foundry executable.

use std::collections::BTreeSet;

use deep_hearth::content::{PROCESS_MELT_PURE_COPPER, build_registries};

use super::foundry_probe::probe_setup;

#[test]
fn foundry_generation_covers_authored_feed_forms_and_varies_conditions() {
    let registries = build_registries();
    let authored_feed_forms = registries
        .thermal()
        .get_melting(PROCESS_MELT_PURE_COPPER)
        .unwrap_or_else(|| panic!("canonical copper melting definition disappeared"))
        .solid_forms()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    assert!(
        !authored_feed_forms.is_empty(),
        "canonical copper melting must retain at least one solid recovery feed form"
    );

    let sample_count = authored_feed_forms.len().max(8);
    let setups = (1_u64
        ..=u64::try_from(sample_count)
            .unwrap_or_else(|_| unreachable!("bounded foundry sample count fits u64")))
        .map(|seed| probe_setup(&registries, seed))
        .collect::<Vec<_>>();
    assert_eq!(
        setups
            .iter()
            .map(|setup| setup.feed_form)
            .collect::<BTreeSet<_>>(),
        authored_feed_forms,
        "bounded foundry generation must exercise every authored pure-copper recovery feed form"
    );
    assert!(
        setups
            .iter()
            .map(|setup| (setup.mass.milligrams(), setup.preheat_target.millikelvin()))
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "foundry generation collapsed to one thermal batch"
    );
    assert!(
        setups
            .iter()
            .map(|setup| {
                (
                    setup.furnace_condition.parts_per_million(),
                    setup.mold_condition.parts_per_million(),
                    setup.electrical_energy.nanojoules(),
                    setup.thermal_sink_energy.nanojoules(),
                )
            })
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "foundry generation collapsed to one operating state"
    );
}
