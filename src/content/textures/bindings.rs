//! Renderer-neutral commodity and equipment appearance binding assembly.

mod commodity;
mod equipment;

pub(super) use commodity::build_commodity_bindings;
pub(super) use equipment::build_equipment_bindings;
