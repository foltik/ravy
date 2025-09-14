use crate::prelude::*;

pub fn dropdown_opt(ui: &mut egui::Ui, id: &str, value: &mut Option<String>, options: &[String]) {
    let mut selection = value.clone().unwrap_or_else(|| "None".to_string());

    egui::ComboBox::from_id_salt(id)
        .selected_text(selection.clone())
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut selection, "None".to_string(), "None");
            for name in options {
                ui.selectable_value(&mut selection, name.to_string(), name);
            }
        });

    *value = if selection == "None" { None } else { Some(selection) };
}
