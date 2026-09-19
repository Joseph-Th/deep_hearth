//! Casting resolution and trusted-load validation error contracts.

mod resolution;
mod validation;

pub use resolution::CastingResolutionError;
pub use validation::CastingJobValidationError;
