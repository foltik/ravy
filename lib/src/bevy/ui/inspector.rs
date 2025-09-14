use std::any::TypeId;

use bevy::picking::pointer::PointerInteraction;
use bevy::reflect::{TypeData, TypeRegistry};
use bevy::window::Monitor;
use bevy_inspector_egui::bevy_inspector;
use bevy_inspector_egui::bevy_inspector::hierarchy::Hierarchy;

use crate::bevy::ui::Ui;
use crate::prelude::*;

#[derive(Debug, Eq, PartialEq)]
pub enum Inspector {
    Entities,
    Resource(TypeId, String),
}

#[derive(Component)]
pub struct UiHidden;

/// Populate `Hidden` on internal system entities we don't want to show in the ui
pub fn update_hidden(
    mut cmds: Commands,
    pointers: Query<Entity, (With<PointerInteraction>, Without<UiHidden>)>,
    windows: Query<Entity, (With<Window>, Without<UiHidden>)>,
    monitors: Query<Entity, (With<Monitor>, Without<UiHidden>)>,
    observers: Query<Entity, (With<Observer>, Without<UiHidden>)>,
) {
    let entities = pointers.iter().chain(&windows).chain(&monitors).chain(&observers);
    for entity in entities {
        cmds.entity(entity).insert(UiHidden);
    }
}

pub fn draw(egui: &mut egui::Ui, world: &mut World, types: &TypeRegistry, ui: &mut Ui) {
    match ui.inspector {
        Inspector::Entities => match ui.selected.as_slice() {
            &[entity] => bevy_inspector::ui_for_entity_with_children(world, entity, egui),
            entities => bevy_inspector::ui_for_entities_shared_components(world, entities, egui),
        },
        Inspector::Resource(type_id, ref name) => {
            egui.label(name);
            bevy_inspector::by_type_id::ui_for_resource(world, type_id, egui, name, types)
        }
    }
}

pub fn draw_entities(egui: &mut egui::Ui, world: &mut World, types: &TypeRegistry, ui: &mut Ui) {
    let selected = Hierarchy {
        world,
        type_registry: types,
        selected: &mut ui.selected,
        context_menu: None,
        shortcircuit_entity: None,
        extra_state: &mut (),
    }
    .show::<Without<UiHidden>>(egui);

    if selected {
        ui.inspector = Inspector::Entities;
    }
}

pub fn draw_resources(egui: &mut egui::Ui, types: &TypeRegistry, ui: &mut Ui) {
    let selected_type_id = match &ui.inspector {
        Inspector::Resource(id, _) => Some(*id),
        _ => None,
    };

    for (name, type_id, _) in reflect::<ReflectResource>(types) {
        if egui.selectable_label(Some(type_id) == selected_type_id, name).clicked() {
            ui.inspector = Inspector::Resource(type_id, name.to_string());
        }
    }
}

/// Returns a sorted list of all types in a registry with the given reflect data, e.g. `ReflectResource` or `ReflectAsset`.
fn reflect<T: TypeData>(type_registry: &TypeRegistry) -> impl Iterator<Item = (&str, TypeId, &T)> {
    let mut types = type_registry
        .iter()
        .filter(|reg| reg.data::<T>().is_some())
        .filter_map(|reg| {
            Some((reg.type_info().type_path_table().short_path(), reg.type_id(), reg.data::<T>()?))
        })
        .collect::<Vec<_>>();

    types.sort_by(|(a, ..), (b, ..)| a.cmp(b));
    types.into_iter()
}
