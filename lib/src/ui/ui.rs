use bevy::render::camera::Viewport;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContext, EguiContextSettings, PrimaryEguiContext};
use bevy_inspector_egui::bevy_egui;
use bevy_inspector_egui::bevy_inspector::hierarchy::SelectedEntities;
use egui_dock::{DockArea, DockState, NodeIndex, Style};

use super::*;

#[derive(Resource)]
pub struct Ui {
    viewport: egui::Rect,
    dock: Option<DockState<Tab>>,
    pub(super) inspector: inspector::Inspector,
    pub selected: SelectedEntities,
    pub visible: bool,
}

#[derive(Debug)]
pub enum Tab {
    Viewport,
    Entities,
    Inspector,
    Audio,
    Resources,
}

impl Default for Ui {
    fn default() -> Self {
        let mut dock = DockState::new(vec![Tab::Viewport]);
        let tree = dock.main_surface_mut();
        let [_game, hierarchy] = tree.split_left(NodeIndex::root(), 0.2, vec![Tab::Entities]);
        let [_hierarchy, inspector] = tree.split_below(hierarchy, 0.25, vec![Tab::Inspector]);
        let [_inspector, _other] = tree.split_below(inspector, 0.5, vec![Tab::Audio, Tab::Resources]);

        Self {
            dock: Some(dock),
            selected: SelectedEntities::default(),
            inspector: inspector::Inspector::Entities,
            viewport: egui::Rect::NOTHING,
            visible: false,
        }
    }
}

struct TabViewer<'a> {
    ui: &'a mut Ui,
    world: &'a mut World,
}

impl egui_dock::TabViewer for TabViewer<'_> {
    type Tab = Tab;

    #[rustfmt::skip]
    fn ui(&mut self, egui: &mut egui_dock::egui::Ui, tab: &mut Self::Tab) {
        let ui = &mut self.ui;
        let world = &mut self.world;
        let types = world.resource::<AppTypeRegistry>().0.clone();
        let types = types.read();

        match tab {
            Tab::Viewport => ui.viewport = egui.clip_rect(),
            Tab::Entities  => inspector::draw_entities(egui, world, &types, ui),
            Tab::Inspector => inspector::draw(egui, world, &types, ui),
            Tab::Audio     => audio_inspector::draw(egui, world),
            Tab::Resources => inspector::draw_resources(egui, &types, ui),
        }
    }

    fn title(&mut self, window: &mut Self::Tab) -> egui_dock::egui::WidgetText {
        format!("{window:?}").into()
    }

    fn clear_background(&self, window: &Self::Tab) -> bool {
        !matches!(window, Tab::Viewport)
    }
}

#[rustfmt::skip]
pub fn draw(world: &mut World) {
    let ctx = world.query_filtered::<&mut EguiContext, With<PrimaryEguiContext>>().single(world).unwrap();
    let mut ctx = ctx.clone();

    world.resource_scope::<Ui, _>(|world, mut ui| {
        let egui = ctx.get_mut();

        if !ui.visible {
            return;
        }

        let mut dock = ui.dock.take().unwrap();
        let mut tab_viewer = TabViewer { world, ui: &mut *ui };

        DockArea::new(&mut dock)
            .style(Style::from_egui(&egui.style()))
            .show(egui, &mut tab_viewer);

        ui.dock = Some(dock);
    });
}

/// Make camera only render to view not obstructed by UI
pub fn update_viewport(
    ui_state: Res<Ui>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut cam: Single<&mut Camera, Without<PrimaryEguiContext>>,
    egui_settings: Single<&EguiContextSettings>,
) {
    let scale_factor = window.scale_factor() * egui_settings.scale_factor;

    let viewport_pos = ui_state.viewport.left_top().to_vec2() * scale_factor;
    let viewport_size = ui_state.viewport.size() * scale_factor;

    let physical_position = UVec2::new(viewport_pos.x as u32, viewport_pos.y as u32);
    let physical_size = UVec2::new(viewport_size.x as u32, viewport_size.y as u32);

    let rect = physical_position + physical_size;

    let window_size = window.physical_size();
    // wgpu will panic if trying to set a viewport rect which has coordinates extending
    // past the size of the render target, i.e. the physical window in our case.
    // Typically this shouldn't happen- but during init and resizing etc. edge cases might occur.
    // Simply do nothing in those cases.
    if rect.x <= window_size.x && rect.y <= window_size.y {
        cam.viewport = Some(Viewport { physical_position, physical_size, depth: 0.0..1.0 });
    }
}
