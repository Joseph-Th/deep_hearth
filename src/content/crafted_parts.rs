//! Canonical masses for fabricated parts that cross built-in content-domain boundaries intact.
//!
//! These values are shared by the manual processes that create a part and the equipment,
//! energy, or storage definitions that consume that exact part. Bulk material quantities and
//! generic forms intentionally stay with their owning recipe or assembly.

use crate::core::quantity::Mass;

pub(super) const COPPER_REINFORCEMENT_MASS: Mass = Mass::from_milligrams(20_000);
pub(super) const COPPER_SCREEN_PLATE_MASS: Mass = Mass::from_milligrams(18_000);
pub(super) const COPPER_SAW_BLADE_MASS: Mass = Mass::from_milligrams(54_000);

pub(super) const STONE_FLYWHEEL_MASS: Mass = Mass::from_milligrams(900_000);
pub(super) const STONE_DRILL_BIT_MASS: Mass = Mass::from_milligrams(100_000);
pub(super) const STONE_GRINDSTONE_WHEEL_MASS: Mass = Mass::from_milligrams(1_400_000);
pub(super) const TIMBER_FLYWHEEL_MASS: Mass = Mass::from_milligrams(2_000_000);
pub(super) const TIMBER_RIDDLE_PANEL_MASS: Mass = Mass::from_milligrams(1_400_000);

pub(super) const ROUGH_TIMBER_FIELD_BOX_BODY_MASS: Mass = Mass::from_milligrams(1_600_000);
pub(super) const TIMBER_PROVISIONS_CHEST_BODY_MASS: Mass = Mass::from_milligrams(2_400_000);
pub(super) const DOUBLE_WALL_TIMBER_PROVISIONS_CHEST_BODY_MASS: Mass =
    Mass::from_milligrams(4_000_000);
pub(super) const BULK_TIMBER_PROVISIONS_CRATE_BODY_MASS: Mass = Mass::from_milligrams(3_200_000);
pub(super) const INSULATED_TIMBER_PANTRY_BODY_MASS: Mass = Mass::from_milligrams(4_800_000);
pub(super) const STONE_PROVISIONS_CROCK_BODY_MASS: Mass = Mass::from_milligrams(2_400_000);
