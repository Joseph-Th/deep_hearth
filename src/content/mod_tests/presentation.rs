//! Built-in texture binding, appearance, and maintenance-output presentation tests.

use super::*;

#[test]
fn built_in_texture_bindings_resolve_for_material_forms_and_equipment() {
    let registries = build_registries();
    let textures = registries.textures();
    let baked = textures.bake_texture_array();

    for (commodity, block, object) in [
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Some(BLOCK_TIMBER),
            OBJECT_LOG,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Some(BLOCK_COPPER_ORE),
            OBJECT_COPPER_ORE,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
            None,
            OBJECT_CRUSHED_ORE,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_CONCENTRATE),
            None,
            OBJECT_CRUSHED_ORE,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
            Some(BLOCK_COPPER),
            OBJECT_COPPER_INGOT,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            None,
            OBJECT_COPPER_REINFORCEMENT,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
            None,
            OBJECT_NATIVE_COPPER,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
            None,
            OBJECT_COPPER_SCRAP,
        ),
        (
            CommodityKey::new(MATERIAL_SLAG, FORM_CRUSHED),
            None,
            OBJECT_TAILINGS,
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_CRUSHED),
            None,
            OBJECT_TAILINGS,
        ),
        (
            CommodityKey::new(MATERIAL_CLAY, FORM_CRUSHED),
            None,
            OBJECT_TAILINGS,
        ),
        (
            CommodityKey::new(MATERIAL_SLAG, FORM_TAILINGS),
            None,
            OBJECT_TAILINGS,
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TAILINGS),
            None,
            OBJECT_TAILINGS,
        ),
        (
            CommodityKey::new(MATERIAL_CLAY, FORM_TAILINGS),
            None,
            OBJECT_TAILINGS,
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            None,
            OBJECT_STONE_LUMP,
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            None,
            OBJECT_STONE_TOOL,
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT),
            None,
            OBJECT_STONE_DRILL_BIT,
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
            None,
            OBJECT_STONE_CHIP,
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            None,
            OBJECT_STONE_FLYWHEEL,
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            None,
            OBJECT_WOOD_HANDLE,
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
            None,
            OBJECT_TIMBER_CHEST_BODY,
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_DOUBLE_WALL_CHEST_BODY),
            None,
            OBJECT_DOUBLE_WALL_TIMBER_CHEST_BODY,
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY),
            None,
            OBJECT_ROUGH_TIMBER_FIELD_BOX_BODY,
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_BULK_CRATE_BODY),
            None,
            OBJECT_BULK_TIMBER_CRATE_BODY,
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_INSULATED_PANTRY_BODY),
            None,
            OBJECT_INSULATED_TIMBER_PANTRY_BODY,
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY),
            None,
            OBJECT_STONE_PROVISIONS_CROCK_BODY,
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL),
            None,
            OBJECT_TIMBER_RIDDLE_PANEL,
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_FLYWHEEL),
            None,
            OBJECT_TIMBER_FLYWHEEL,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_SCREEN_PLATE),
            None,
            OBJECT_COPPER_SCREEN_PLATE,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE),
            None,
            OBJECT_COPPER_SAW_BLADE,
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_SCRAP),
            None,
            OBJECT_WOOD_CHIP,
        ),
    ] {
        let binding = match textures.get_commodity_appearance(commodity) {
            Some(binding) => binding,
            None => panic!("missing commodity appearance {}", commodity.value()),
        };
        assert_eq!(binding.block(), block);
        assert_eq!(binding.object(), Some(object));
        if let Some(block) = block {
            let baked_block = match baked.get_block(block) {
                Some(block) => block,
                None => panic!("missing baked block appearance {}", block.value()),
            };
            let authored_block = match textures.get_block(block) {
                Some(block) => block,
                None => panic!("missing authored block appearance {}", block.value()),
            };
            let top_texture = authored_block.texture(crate::texture::CubeFace::Top);
            assert_eq!(
                baked_block.texture(crate::texture::CubeFace::Top),
                match baked.get_descriptor(top_texture) {
                    Some(descriptor) => descriptor,
                    None => panic!("missing baked texture {}", top_texture.value()),
                }
            );
        }
    }

    for (equipment, object) in [
        (EQUIPMENT_JAW_CRUSHER, OBJECT_JAW_CRUSHER),
        (EQUIPMENT_ELECTRIC_FURNACE, OBJECT_ELECTRIC_FURNACE),
        (EQUIPMENT_CASTING_MOLD, OBJECT_CASTING_MOLD),
        (EQUIPMENT_DRY_SCREEN, OBJECT_DRY_SCREEN),
        (EQUIPMENT_GRINDING_MILL, OBJECT_GRINDING_MILL),
        (EQUIPMENT_GRAVITY_SEPARATOR, OBJECT_GRAVITY_SEPARATOR),
        (EQUIPMENT_STONE_PICK, OBJECT_STONE_PICK),
        (EQUIPMENT_STONE_HAND_CRANK, OBJECT_STONE_HAND_CRANK),
        (EQUIPMENT_STONE_QUARRY_PICK, OBJECT_STONE_QUARRY_PICK),
        (EQUIPMENT_TIMBER_TREADLE_DRIVE, OBJECT_TIMBER_TREADLE_DRIVE),
        (EQUIPMENT_STONE_CRUSHER, OBJECT_STONE_CRUSHER),
        (EQUIPMENT_STONE_SEPARATOR, OBJECT_STONE_SEPARATOR),
        (EQUIPMENT_STONE_ROTARY_QUERN, OBJECT_STONE_ROTARY_QUERN),
        (
            EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
            OBJECT_STONE_GEOLOGICAL_HAMMER,
        ),
        (
            EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
            OBJECT_TIMBER_RIDDLE_SIZING_SCREEN,
        ),
        (
            EQUIPMENT_COPPER_PLATE_SIZING_SCREEN,
            OBJECT_COPPER_PLATE_SIZING_SCREEN,
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_PICK,
            OBJECT_COPPER_REINFORCED_PICK,
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
            OBJECT_COPPER_REINFORCED_HAND_CRANK,
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
            OBJECT_COPPER_REINFORCED_STONE_QUARRY_PICK,
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
            OBJECT_COPPER_REINFORCED_STONE_CRUSHER,
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
            OBJECT_COPPER_REINFORCED_STONE_SEPARATOR,
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
            OBJECT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
            OBJECT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
        ),
        (
            EQUIPMENT_STONE_WOODWORKING_ADZE,
            OBJECT_STONE_WOODWORKING_ADZE,
        ),
        (EQUIPMENT_STONE_COBBING_HAMMER, OBJECT_STONE_COBBING_HAMMER),
        (
            EQUIPMENT_STONE_FLYWHEEL_PUMP_DRILL,
            OBJECT_STONE_FLYWHEEL_PUMP_DRILL,
        ),
        (EQUIPMENT_TIMBER_SPINDLE_DRILL, OBJECT_TIMBER_SPINDLE_DRILL),
        (
            EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
            OBJECT_COPPER_REINFORCED_WOODWORKING_ADZE,
        ),
        (
            EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
            OBJECT_TIMBER_FRAME_SAW_BENCH,
        ),
        (
            EQUIPMENT_TIMBER_DRESSING_BENCH,
            OBJECT_TIMBER_DRESSING_BENCH,
        ),
        (
            EQUIPMENT_TIMBER_FRAME_COMMINUTION_MILL,
            OBJECT_TIMBER_FRAME_COMMINUTION_MILL,
        ),
        (
            EQUIPMENT_TIMBER_ORE_DRESSING_TABLE,
            OBJECT_TIMBER_ORE_DRESSING_TABLE,
        ),
        (
            EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
            OBJECT_TIMBER_WALKING_WHEEL_DRIVE,
        ),
        (
            EQUIPMENT_TIMBER_TREADLE_HAMMER,
            OBJECT_TIMBER_TREADLE_HAMMER,
        ),
    ] {
        let binding = match textures.get_equipment_appearance(equipment) {
            Some(binding) => binding,
            None => panic!("missing equipment appearance {}", equipment.value()),
        };
        assert_eq!(binding.object(), object);
        let appearance = match textures.get_object(object) {
            Some(appearance) => appearance,
            None => panic!("missing object appearance {}", object.value()),
        };
        for texture in appearance.textures() {
            assert!(baked.get_descriptor(*texture).is_some());
        }
        let baked_object = match baked.get_object(object) {
            Some(appearance) => appearance,
            None => panic!("missing baked object appearance {}", object.value()),
        };
        assert_eq!(baked_object.textures().len(), appearance.textures().len());
    }
}

#[test]
fn every_builtin_equipment_definition_has_a_complete_object_appearance() {
    let registries = build_registries();
    let textures = registries.textures();
    let baked = textures.bake_texture_array();

    for definition in registries.equipment().definitions() {
        let binding = textures
            .get_equipment_appearance(definition.id())
            .unwrap_or_else(|| {
                panic!(
                    "equipment {} ({}) must have a player-legible object appearance",
                    definition.id().value(),
                    definition.name()
                )
            });
        let object = binding.object();
        let appearance = textures.get_object(object).unwrap_or_else(|| {
            panic!(
                "equipment {} appearance references missing object {}",
                definition.id().value(),
                object.value()
            )
        });
        assert!(
            !appearance.textures().is_empty(),
            "equipment {} object appearance must contain at least one texture",
            definition.id().value()
        );
        assert!(
            appearance
                .textures()
                .iter()
                .all(|texture| baked.get_descriptor(*texture).is_some()),
            "equipment {} object appearance must bake every referenced texture",
            definition.id().value()
        );
        assert!(
            baked.get_object(object).is_some(),
            "equipment {} object appearance {} must be present in the baked atlas",
            definition.id().value(),
            object.value()
        );
    }
}

#[test]
fn every_supported_separation_residue_host_has_legible_crushed_and_tailings_appearances() {
    let registries = build_registries();
    let textures = registries.textures();

    for material in [MATERIAL_STONE, MATERIAL_CLAY, MATERIAL_SLAG] {
        for form in [FORM_CRUSHED, FORM_TAILINGS, FORM_EXHAUSTED_TAILINGS] {
            let commodity = CommodityKey::new(material, form);
            assert!(
                registries.materials().has_commodity(commodity),
                "supported separation residue host {} lost authored form {}",
                material.value(),
                form.value()
            );
            assert!(
                textures
                    .get_commodity_appearance(commodity)
                    .and_then(|binding| binding.object())
                    .is_some(),
                "supported separation residue host {} form {} must remain player-legible",
                material.value(),
                form.value()
            );
        }
    }
}

#[test]
fn every_builtin_maintenance_spent_output_has_a_player_legible_appearance() {
    let registries = build_registries();
    let textures = registries.textures();

    for definition in registries.equipment().definitions() {
        let Some(maintenance) = definition.maintenance_profile() else {
            continue;
        };
        let spent = maintenance.spent();
        assert!(
            textures
                .get_commodity_appearance(spent)
                .and_then(|binding| binding.object())
                .is_some(),
            "equipment {} maintenance spent commodity {} must remain player-legible",
            definition.id().value(),
            spent.value()
        );
    }
}
