//! Cross-domain registry-operability contract tests.

use super::*;

use crate::content::build_registries;
use crate::core::quantity::{Energy, Volume};
use crate::core::time::TickSpan;

#[test]
fn built_fixed_player_work_is_operable() {
    let _ = build_registries();
}

#[test]
fn fixed_player_work_rejects_duration_beyond_full_reserves() {
    let physiology = build_registries().survival().physiology();
    let impossible_duration = TickSpan::new(
        physiology
            .maximum_hydration()
            .microliters()
            .saturating_add(1),
    );
    let result = std::panic::catch_unwind(|| {
        assert_player_work_fits_reserves(
            physiology,
            SurvivalExertion::new(Energy::from_nanojoules(1), Volume::from_microliters(1)),
            impossible_duration,
            "test work",
            1,
        );
    });

    assert!(result.is_err());
}
