//! Content-catalog evidence rendering for exploratory gameplay reports.

use deep_hearth::energy::EnergyCarrier;
use deep_hearth::registry::{ProcessEnergyRole, ProcessEquipmentRole, Registries};

use super::super::catalog::{ProcessResolverKind, process_catalog_entries};

fn process_resolver_label(resolver: ProcessResolverKind) -> &'static str {
    match resolver {
        ProcessResolverKind::ManualCraft => "manual-craft",
        ProcessResolverKind::PoweredCraft => "powered-craft",
        ProcessResolverKind::ManualComminution => "manual-comminution",
        ProcessResolverKind::ManualSeparation => "manual-separation",
        ProcessResolverKind::Comminution => "comminution",
        ProcessResolverKind::Screening => "screening",
        ProcessResolverKind::ConstituentSeparation => "constituent-separation",
        ProcessResolverKind::SensibleHeating => "sensible-heating",
        ProcessResolverKind::Melting => "melting",
        ProcessResolverKind::Casting => "casting",
    }
}

fn process_equipment_role_label(role: ProcessEquipmentRole) -> &'static str {
    match role {
        ProcessEquipmentRole::None => "none",
        ProcessEquipmentRole::Optional => "optional",
        ProcessEquipmentRole::Required => "required",
    }
}

fn process_energy_role_label(role: ProcessEnergyRole) -> &'static str {
    match role {
        ProcessEnergyRole::None => "none",
        ProcessEnergyRole::Supply(EnergyCarrier::Mechanical) => "mechanical-supply",
        ProcessEnergyRole::Supply(EnergyCarrier::Electrical) => "electrical-supply",
        ProcessEnergyRole::Supply(EnergyCarrier::Thermal) => "thermal-supply",
        ProcessEnergyRole::Sink(EnergyCarrier::Mechanical) => "mechanical-sink",
        ProcessEnergyRole::Sink(EnergyCarrier::Electrical) => "electrical-sink",
        ProcessEnergyRole::Sink(EnergyCarrier::Thermal) => "thermal-sink",
    }
}

pub(crate) fn print_content_summary(registries: &Registries, include_catalog: bool) {
    let equipment_count = registries.equipment().definitions().count();
    let equipment_assembly_edges = registries
        .equipment()
        .definitions()
        .filter(|definition| definition.assembly_profile().is_some())
        .count();
    let equipment_upgrade_edges = registries
        .equipment()
        .definitions()
        .filter(|definition| definition.upgrade_profile().is_some())
        .count();
    let structurally_installed_equipment = registries
        .equipment()
        .definitions()
        .filter(|definition| definition.requires_structural_support())
        .count();
    let energy_count = registries.energy().definitions().count();
    let energy_assembly_edges = registries
        .energy()
        .definitions()
        .filter(|definition| definition.assembly_profile().is_some())
        .count();
    let process_catalog = process_catalog_entries(registries);
    let process_count = process_catalog.len();
    let manual_process_count = process_catalog
        .iter()
        .filter(|entry| {
            matches!(
                entry.resolver,
                ProcessResolverKind::ManualCraft
                    | ProcessResolverKind::ManualComminution
                    | ProcessResolverKind::ManualSeparation
            )
        })
        .count();
    let machine_process_count = process_count
        .checked_sub(manual_process_count)
        .unwrap_or_else(|| {
            unreachable!("manual process classification cannot exceed the complete process catalog")
        });
    let storage_count = registries.storage().definitions().count();
    let mining_method_count = registries.mining().definitions().count();
    let prospecting_method_count = registries.labor().prospecting_definitions().count();
    let food_count = registries.survival().foods().count();
    let drink_count = registries.survival().drinks().count();
    std::println!(
        "CONTENT registry_schema={} equipment=[authored:{} assembly_edges:{} upgrade_edges:{} structural_installation_required:{}] energy=[authored:{} assembly_edges:{}] storage=[authored:{}] processes=[authored:{} manual:{} machine:{}] mining_methods={} prospecting_methods={} survival=[foods:{} drinks:{}]",
        registries.schema_version().value(),
        equipment_count,
        equipment_assembly_edges,
        equipment_upgrade_edges,
        structurally_installed_equipment,
        energy_count,
        energy_assembly_edges,
        storage_count,
        process_count,
        manual_process_count,
        machine_process_count,
        mining_method_count,
        prospecting_method_count,
        food_count,
        drink_count,
    );

    let authored_edge_equipment = registries
        .equipment()
        .definitions()
        .filter(|definition| definition.has_authored_acquisition_edge())
        .count();
    let authored_edge_energy = registries
        .energy()
        .definitions()
        .filter(|definition| definition.has_authored_assembly_edge())
        .count();
    std::println!(
        "CONTENT ACQUISITION EDGES equipment=[authored-edge:{authored_edge_equipment} no-authored-edge:{}] energy=[authored-edge:{authored_edge_energy} no-authored-edge:{}] reachability=direct-edge-not-end-to-end-proof",
        equipment_count - authored_edge_equipment,
        energy_count - authored_edge_energy,
    );
    if !include_catalog {
        return;
    }

    let acquisition_declared_equipment = registries
        .equipment()
        .definitions()
        .filter(|definition| definition.has_authored_acquisition_edge())
        .map(|definition| definition.name())
        .collect::<Vec<_>>()
        .join(",");
    let no_acquisition_equipment = registries
        .equipment()
        .definitions()
        .filter(|definition| !definition.has_authored_acquisition_edge())
        .map(|definition| definition.name())
        .collect::<Vec<_>>()
        .join(",");
    let assembly_declared_energy = registries
        .energy()
        .definitions()
        .filter(|definition| definition.has_authored_assembly_edge())
        .map(|definition| definition.name())
        .collect::<Vec<_>>()
        .join(",");
    let no_assembly_energy = registries
        .energy()
        .definitions()
        .filter(|definition| !definition.has_authored_assembly_edge())
        .map(|definition| definition.name())
        .collect::<Vec<_>>()
        .join(",");
    std::println!(
        "CONTENT ACQUISITION authored-edge-equipment=[{acquisition_declared_equipment}] authored-edge-energy=[{assembly_declared_energy}] no-authored-edge-equipment=[{no_acquisition_equipment}] no-authored-edge-energy=[{no_assembly_energy}] evidence-note=direct-edge-is-not-end-to-end-reachability"
    );

    let equipment = registries
        .equipment()
        .definitions()
        .map(|definition| {
            let acquisition = match (
                definition.assembly_profile().is_some(),
                definition.upgrade_profile().is_some(),
            ) {
                (true, true) => "assemble+upgrade",
                (true, false) => "assemble",
                (false, true) => "upgrade-only",
                (false, false) => "no-authored-acquisition-edge",
            };
            let installation = if definition.requires_structural_support() {
                "fixed"
            } else {
                "portable"
            };
            format!(
                "{}:{}:{}:{}",
                definition.id().value(),
                definition.name(),
                acquisition,
                installation,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let energy = registries
        .energy()
        .definitions()
        .map(|definition| {
            let acquisition = if definition.assembly_profile().is_some() {
                "assemble"
            } else {
                "no-authored-assembly-edge"
            };
            format!(
                "{}:{}:{}",
                definition.id().value(),
                definition.name(),
                acquisition
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let processes = registries
        .production()
        .definitions()
        .map(|definition| format!("{}:{}", definition.id().value(), definition.name()))
        .collect::<Vec<_>>()
        .join(",");
    let storage = registries
        .storage()
        .definitions()
        .map(|definition| {
            format!(
                "{}:{}:capacity={}mg:preservation={}ppm:embodied={}mg",
                definition.id().value(),
                definition.name(),
                definition.maximum_stockpile_capacity().milligrams(),
                definition.storage_profile().preservation_multiplier_ppm(),
                definition.assembly_profile().input_mass().milligrams(),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    std::println!(
        "CONTENT CATALOG equipment=[{equipment}] energy=[{energy}] storage=[{storage}] processes=[{processes}]"
    );
    let prospecting = registries
    .labor()
    .prospecting_definitions()
    .map(|definition| {
        let exertion = definition.exertion();
        format!(
            "{}:{:?}:spatial={:?}:duration={}t:max-region={}vox:uncertainty={}ppm:exertion={}nJ+{}uL/t",
            definition.id().value(),
            definition.evidence(),
            definition.spatial_resolution(),
            definition.duration().value(),
            definition.maximum_region_voxels(),
            definition.abundance_uncertainty_ppm(),
            exertion.energy_cost_per_tick().nanojoules(),
            exertion.hydration_loss_per_tick().microliters(),
        )
    })
    .collect::<Vec<_>>()
    .join(",");
    let foods = registries
        .survival()
        .foods()
        .map(|food| {
            let commodity = food.commodity();
            let material = registries
                .materials()
                .get_material(commodity.material())
                .unwrap_or_else(|| unreachable!("validated food commodity has a material"));
            let form = registries
                .materials()
                .get_form(commodity.form())
                .unwrap_or_else(|| unreachable!("validated food commodity has a form"));
            format!(
                "{}:{}/{}:{:?}:energy={}nJ/mg:hydration={}ppm:shelf={}t",
                commodity.value(),
                material.name(),
                form.name(),
                food.category(),
                food.dietary_energy().nanojoules_per_milligram(),
                food.hydration_multiplier_ppm(),
                food.shelf_life().value(),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let drinks = registries
        .survival()
        .drinks()
        .map(|drink| {
            let fluid = registries
                .fluid()
                .get_fluid(drink.fluid())
                .unwrap_or_else(|| unreachable!("validated drink has a fluid definition"));
            format!(
                "{}:{}:hydration={}ppm",
                drink.fluid().value(),
                fluid.name(),
                drink.hydration_multiplier_ppm(),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    std::println!("CONTENT PROSPECTING [{prospecting}]");
    std::println!("CONTENT SURVIVAL foods=[{foods}] drinks=[{drinks}]");
    let process_routes = process_catalog
    .into_iter()
    .map(|entry| {
        format!(
            "{}:{}:resolver={}:equipment={}:capability-providers={}/{}authored-acquisition:energy={}:compatible-stores={}/{}authored-assembly",
            entry.process.value(),
            entry.name,
            process_resolver_label(entry.resolver),
            process_equipment_role_label(entry.equipment_role),
            entry.nominal_provider_count,
            entry.authored_acquisition_provider_count,
            process_energy_role_label(entry.energy_role),
            entry.compatible_energy_store_count,
            entry.authored_assembly_energy_store_count,
        )
    })
    .collect::<Vec<_>>()
    .join(",");
    std::println!("CONTENT PROCESS ROUTES [{process_routes}]");
}
