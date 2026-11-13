//! Typed player-visible consequences of physical and resident events.

use super::{AppState, Registries, fire, view};

/// Local control receipts expose physical outcomes, never inventory or surface identities.
pub(in super::super) fn pickup(
    registries: &Registries,
    event: deep_hearth::surface::SurfaceDropPickupOutcome,
) -> String {
    // The surface owner's receipt preserves the actual commodity across
    // subsequent actions on this same control frame.
    let material = registries
        .materials()
        .commodity_name(event.commodity())
        .unwrap_or("material");
    format!(
        "{} g {material} entered your pack.",
        view::grams(event.picked_mass())
    )
}

pub(in super::super) fn excavation(event: deep_hearth::world::BlockExcavationOutcome) -> String {
    use deep_hearth::world::BlockExcavationOutputReceipt;
    let custody = match event.output() {
        BlockExcavationOutputReceipt::Carried { .. } => "entered your pack",
        BlockExcavationOutputReceipt::Surface { .. } => "landed on the ground",
    };
    let target = event.target();
    format!(
        "{} g excavated at ({},{},{}); {} g remains; material {custody}.",
        view::grams(event.excavated_mass()),
        target.x(),
        target.y(),
        target.z(),
        view::grams(event.remaining_mass())
    )
}

/// Impact is physically immediate; its vitality cost settles at the next slow boundary.
pub(in super::super) fn landing(event: deep_hearth::logistics::PlayerFallImpactOutcome) -> String {
    let speed = event.impact_speed().nanometers_per_second();
    let speed = format!(
        "{}.{:02}",
        speed / 1_000_000_000,
        speed % 1_000_000_000 / 10_000_000
    );
    if event.vitality_loss_ppm() == 0 {
        format!("Landed at {speed} m/s without injury.")
    } else {
        let loss = event.vitality_loss_ppm();
        format!(
            "Hard landing at {speed} m/s; {}.{:04}% vitality loss pending settlement.",
            loss / 10_000,
            loss % 10_000
        )
    }
}

pub(in super::super) fn tree_felled(event: deep_hearth::world::TreeFellingOutcome) -> String {
    let root = event.root();
    format!(
        "{} g timber fell at ({},{},{}); the wood remains on the ground.",
        view::grams(event.wood()),
        root.x(),
        root.y(),
        root.z()
    )
}

pub(in super::super) fn maintenance(
    registries: &Registries,
    state: &AppState,
    event: deep_hearth::equipment::EquipmentMaintenanceOutcome,
) -> String {
    let name = state
        .equipment()
        .get_equipment(event.equipment())
        .and_then(|equipment| registries.equipment().get_equipment(equipment.definition()))
        .map_or("equipment", |definition| definition.name());
    let before = event.condition_before().parts_per_million();
    let after = event.condition_after().parts_per_million();
    format!(
        "{name} condition restored from {}.{:02}% to {}.{:02}%.",
        before / 10_000,
        before % 10_000 / 100,
        after / 10_000,
        after % 10_000 / 100
    )
}

pub(in super::super) fn power(event: deep_hearth::labor::ManualPowerOutcome) -> String {
    format!(
        "{} of mechanical or electrical work transferred into finite storage.",
        fire::energy_label(event.energy())
    )
}

/// Only the scout owner's already-filtered observer receipts may generate these messages.
/// Never infer a hidden route or material identity from unrelated global state.
pub(in super::super) fn scout(event: deep_hearth::kobolds::KoboldScoutEvent) -> Option<String> {
    let mut messages = Vec::new();
    if let Some((_, mass)) = event.raided_stockpile() {
        messages.push(format!(
            "A scout took {} g from an exposed local cache.",
            view::grams(mass)
        ));
    }
    if let Some((_, mass)) = event.split_drop() {
        messages.push(format!(
            "A scout split {} g from a loose pile.",
            view::grams(mass)
        ));
    }
    if event.relocated_drop().is_some() {
        messages.push("A scout carried a loose parcel away.".to_owned());
    }
    if event.discovered_player_drop() {
        messages.push("A scout noticed exposed loose material.".to_owned());
    }
    if let Some((_, mass)) = event.foraged_food() {
        messages.push(format!(
            "A scout gathered {} g of wild food.",
            view::grams(mass)
        ));
    }
    if let Some((_, mass)) = event.consumed_food() {
        messages.push(format!("A scout ate {} g of food.", view::grams(mass)));
    }
    if let Some((_, volume)) = event.drank_water() {
        messages.push(format!(
            "A scout drank {} mL of water.",
            view::milliliters(volume)
        ));
    }
    if event.abandoned_drop().is_some() {
        messages.push("A scout left a parcel on the ground.".to_owned());
    }
    if event.discarded_food().is_some() {
        messages.push("A scout discarded unfit food.".to_owned());
    }
    if event.cleared_cache().is_some() {
        messages.push("A scout moved material out of its cache.".to_owned());
    }
    if event.fled_player() {
        messages.push("A scout retreated from a nearby threat.".to_owned());
    } else if event.fled_light() {
        messages.push("A scout retreated from light.".to_owned());
    } else if event.turned_back_at_barrier() {
        messages.push("A scout turned back at an obstruction.".to_owned());
    }
    if event.cancelled_empty_cache_raid() {
        messages.push("A scout abandoned an empty-cache raid.".to_owned());
    }
    if let Some((_, loss)) = event.defensive_contact() {
        messages.push(format!(
            "A resident struck you; {}.{:04}% vitality lost.",
            loss / 10_000,
            loss % 10_000
        ));
    }
    (!messages.is_empty()).then(|| messages.join(" "))
}

pub(in super::super) fn alarm(event: deep_hearth::kobolds::KoboldAlarmPulse) -> String {
    use deep_hearth::kobolds::KoboldJunkRigKind;
    let noise = match event.kind() {
        KoboldJunkRigKind::TimberRatchet => "wooden ratchet",
        KoboldJunkRigKind::StoneClatter => "stone clatter",
        KoboldJunkRigKind::IronChime => "iron chime",
    };
    match event.position() {
        Some(at) => format!("A {noise} sounded at ({},{},{}).", at.x(), at.y(), at.z()),
        None => format!("A {noise} sounded nearby."),
    }
}
