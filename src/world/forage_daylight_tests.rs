//! Perennial yield must integrate the same actual phase durations as skylight.

use super::*;

#[test]
fn odd_day_length_prices_actual_light_not_four_equal_quarters() {
    let short_day = CalendarDefinition::new(5, 18, 8, 12);
    let authored_day = crate::content::build_registries().core().calendar();
    let reserve = Mass::from_milligrams(2_000);
    let original_fruit = Mass::from_milligrams(4_000);

    assert_eq!(
        ripened_crop(short_day, reserve, original_fruit, RootMoisture::NoBasin, 0),
        Mass::from_milligrams(1_000),
        "unshaded plants retain their quarter-fruit crop across calendars"
    );
    assert_eq!(
        ripened_crop(short_day, reserve, original_fruit, RootMoisture::NoBasin, 8),
        Mass::from_milligrams(210),
        "shaded plants receive 8 of 38 actual tick-level daylight units"
    );
    assert_eq!(
        ripened_crop(
            authored_day,
            reserve,
            original_fruit,
            RootMoisture::NoBasin,
            8
        ),
        Mass::from_milligrams(241),
        "the ordinary evenly divided day keeps its existing crop size"
    );
    assert_eq!(
        ripened_crop(
            short_day,
            reserve,
            original_fruit,
            RootMoisture::NoBasin,
            14
        ),
        Mass::ZERO
    );
    assert_eq!(
        ripened_crop(
            short_day,
            Mass::from_milligrams(100),
            original_fruit,
            RootMoisture::NoBasin,
            0,
        ),
        Mass::from_milligrams(100),
        "daylight may not create more biomass than the retained root reserve"
    );
}
