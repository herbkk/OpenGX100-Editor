//! FX presets: the settings of one effect block saved as a file of their own
//! (`<data dir>/presets/<FX TYPE>/<name>.json`, see [`crate::store::Preset`]), loaded back into an
//! effect of the same type. Loading writes each parameter that differs, one DT1 each, exactly as
//! turning that knob would; the FX type, the ON/OFF switch and the expression-pedal position are
//! not part of a preset.

use super::editing::Dialog;
use super::{EditorApp, AMBER};
use crate::store::{Preset, PresetInfo, Store};
use eframe::egui::{self, RichText};
use opengx100_protocol::patch::PatchData;
use opengx100_protocol::tables::{FxField, FxParamDef, FxTypeDef};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Parameters a preset holds: every setting of the type except the live pedal position.
fn preset_params(t: &FxTypeDef) -> impl Iterator<Item = &'static FxParamDef> {
    t.params.iter().filter(|p| matches!(p.field, FxField::Param(_)) && p.unique_name != "pedal-position")
}

/// The settings of the effect in `slot` as a preset called `name`.
pub(super) fn capture(data: &PatchData, slot: u8, t: &FxTypeDef, name: &str) -> Preset {
    Preset {
        fx_type: t.name.to_string(),
        name: name.to_string(),
        created: crate::store::now(),
        params: preset_params(t).filter_map(|p| Some((p.unique_name.to_string(), data.fx_param(slot, p)?))).collect(),
    }
}

/// What loading `preset` into the effect in `slot` changes: (parameter, new value) for each setting
/// that differs, and how many of the type's settings the preset can't set (missing or out of range).
pub(super) fn changes(
    data: &PatchData,
    slot: u8,
    t: &FxTypeDef,
    preset: &BTreeMap<String, i32>,
) -> (Vec<(&'static FxParamDef, i32)>, usize) {
    let mut out = Vec::new();
    let mut skipped = 0;
    for p in preset_params(t) {
        match preset.get(p.unique_name) {
            Some(&v) if (p.min..=p.max).contains(&v) => {
                if data.fx_param(slot, p) != Some(v) {
                    out.push((p, v));
                }
            }
            _ => skipped += 1,
        }
    }
    (out, skipped)
}

/// Whether `name` can be a preset name (shown as typed; the file name is made safe separately).
fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.chars().count() <= 40 && !name.chars().any(char::is_control)
}

/// The presets menu's view of the disk, read when the menu opens.
#[derive(Default)]
pub(super) struct PresetCache {
    fx_type: String,
    list: Vec<PresetInfo>,
}

impl EditorApp {
    /// "Presets ▾" next to the effect's name: save the block's settings, load one of its type's presets.
    pub(super) fn presets_menu(&mut self, ui: &mut egui::Ui, data: &PatchData, slot: u8, t: &'static FxTypeDef) {
        if preset_params(t).next().is_none() || self.store.is_none() {
            return;
        }
        let resp = ui.menu_button("Presets ▾", |ui| {
            ui.set_min_width(200.0);
            if ui.button("Save as preset…").clicked() {
                ui.close();
                let name = self.patches_temp_fx_name(t);
                self.dialog = Some(Dialog::SavePreset { slot, fx_type: t.name, name });
            }
            ui.separator();
            let list = std::mem::take(&mut self.preset_cache.list);
            if list.is_empty() {
                ui.label(RichText::new(format!("No {} presets yet.", t.name)).weak());
            }
            for info in &list {
                ui.horizontal(|ui| {
                    let (diff, skipped) = changes(data, slot, t, &info.preset.params);
                    let tip = match (diff.len(), skipped) {
                        (0, _) => "Same as the current settings.".to_string(),
                        (n, 0) => format!("Load: changes {n} setting(s)."),
                        (n, s) => format!("Load: changes {n} setting(s); {s} aren't in the preset and stay."),
                    };
                    let label = if diff.is_empty() {
                        RichText::new(format!("✓ {}", info.preset.name))
                    } else {
                        RichText::new(&info.preset.name)
                    };
                    if ui.button(label).on_hover_text(tip).clicked() {
                        ui.close();
                        self.load_preset(slot, &info.preset);
                    }
                    if ui.add(egui::Button::new("×").small().frame(true)).on_hover_text("Delete this preset…").clicked()
                    {
                        ui.close();
                        self.dialog =
                            Some(Dialog::DeletePreset { path: info.path.clone(), name: info.preset.name.clone() });
                    }
                });
            }
            self.preset_cache.list = list;
            ui.separator();
            if ui.button("Show presets folder").clicked() {
                ui.close();
                if let Some(dir) = self.store.as_ref().map(|s| s.preset_type_dir(t.name)) {
                    let _ = std::fs::create_dir_all(&dir);
                    super::backups::reveal(&dir, false);
                }
            }
        });
        if resp.response.clicked() || self.preset_cache.fx_type != t.name {
            self.refresh_presets(t.name);
        }
    }

    /// A name to suggest when saving: the patch's name, so presets say where they came from.
    fn patches_temp_fx_name(&self, t: &FxTypeDef) -> String {
        let patch = self.patches.get(&crate::worker::PatchId::Temp).map(|p| p.name()).unwrap_or_default();
        let patch = patch.trim();
        if patch.is_empty() {
            t.name.to_string()
        } else {
            patch.to_string()
        }
    }

    fn refresh_presets(&mut self, fx_type: &str) {
        let Some(store) = &self.store else { return };
        let (list, errors) = store.list_presets(fx_type);
        if let Some(e) = errors.first() {
            self.last_error = Some(format!("unreadable preset: {e}"));
        }
        self.preset_cache = PresetCache { fx_type: fx_type.to_string(), list };
    }

    fn load_preset(&mut self, slot: u8, preset: &Preset) {
        let Some(data) = self.patches.get(&crate::worker::PatchId::Temp).cloned() else { return };
        let Some(t) = data.fx_slot(slot).and_then(|s| s.fx_type()).filter(|t| t.name == preset.fx_type) else {
            self.last_error = Some(format!("preset \"{}\" is for {}", preset.name, preset.fx_type));
            return;
        };
        let (diff, skipped) = changes(&data, slot, t, &preset.params);
        crate::log::line(&format!("preset: load \"{}\" into slot {slot} ({} changes)", preset.name, diff.len()));
        for &(p, v) in &diff {
            self.set_fx_param(slot, p, v);
        }
        let more = if skipped > 0 {
            format!(" {skipped} setting(s) weren't in it and kept their value.")
        } else {
            String::new()
        };
        self.notice = Some(format!("Loaded preset \"{}\" into {}.{more}", preset.name, t.name));
    }

    /// The Save-preset and Delete-preset dialogs. Returns false once answered.
    pub(super) fn preset_dialog(&mut self, ui: &mut egui::Ui, dialog: &mut Dialog) -> bool {
        match dialog {
            Dialog::SavePreset { slot, fx_type, name } => {
                let (slot, fx_type) = (*slot, *fx_type);
                ui.label(format!("Save the settings of {fx_type} (slot {slot}) as a preset."));
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(name).char_limit(40).desired_width(220.0));
                });
                let Some(store) = &self.store else { return false };
                let path = store.preset_path(fx_type, name.trim());
                let valid = valid_name(name);
                if valid && path.exists() {
                    ui.colored_label(AMBER, "Replaces the preset with that name.");
                }
                ui.label(RichText::new(format!("Saved in {}", store.preset_type_dir(fx_type).display())).weak());
                let mut open = true;
                ui.horizontal(|ui| {
                    if ui.add_enabled(valid, egui::Button::new("Save")).clicked() {
                        open = false;
                        self.save_preset(slot, fx_type, name.trim());
                    }
                    if ui.button("Cancel").clicked() {
                        open = false;
                    }
                });
                open
            }
            Dialog::DeletePreset { path, name } => {
                ui.label(format!("Delete the preset \"{name}\"?"));
                ui.label(RichText::new(path.display().to_string()).weak());
                let mut open = true;
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        open = false;
                        self.delete_preset(path.clone(), name);
                    }
                    if ui.button("Cancel").clicked() {
                        open = false;
                    }
                });
                open
            }
            _ => false,
        }
    }

    fn save_preset(&mut self, slot: u8, fx_type: &str, name: &str) {
        let Some(data) = self.patches.get(&crate::worker::PatchId::Temp) else { return };
        let Some(t) = data.fx_slot(slot).and_then(|s| s.fx_type()).filter(|t| t.name == fx_type) else {
            self.last_error = Some(format!("slot {slot} no longer holds {fx_type}"));
            return;
        };
        let preset = capture(data, slot, t, name);
        let Some(store) = &self.store else { return };
        match store.save_preset(&preset) {
            Ok(path) => {
                crate::log::line(&format!("preset: saved {}", path.display()));
                self.notice = Some(format!("Saved preset \"{name}\" ({fx_type})."));
            }
            Err(e) => self.last_error = Some(format!("saving the preset: {e}")),
        }
        self.refresh_presets(fx_type);
    }

    fn delete_preset(&mut self, path: PathBuf, name: &str) {
        match Store::delete_preset(&path) {
            Ok(()) => self.notice = Some(format!("Deleted preset \"{name}\".")),
            Err(e) => self.last_error = Some(format!("deleting the preset: {e}")),
        }
        let t = self.preset_cache.fx_type.clone();
        self.refresh_presets(&t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opengx100_protocol::patch::patch_span;
    use opengx100_protocol::tables;

    #[test]
    fn capture_then_changes() {
        // Any type with settings, put into slot 1 of an empty patch.
        let t = tables::FX_TYPES.iter().find(|t| preset_params(t).count() >= 2).expect("an FX type with settings");
        let mut data = PatchData::from_bytes(vec![0; patch_span() as usize]);
        let base = super::super::slot_offset(1).unwrap();
        data.apply(base, &[t.id]);
        for p in preset_params(t) {
            data.apply(base + p.offset.linear(), &p.spec().encode(p.min).unwrap());
        }
        let preset = capture(&data, 1, t, "x");
        assert_eq!(preset.params.len(), preset_params(t).count());
        let (diff, skipped) = changes(&data, 1, t, &preset.params);
        assert!(diff.is_empty());
        assert_eq!(skipped, 0);

        let mut other = preset.params.clone();
        let ps: Vec<_> = preset_params(t).collect();
        other.insert(ps[0].unique_name.into(), ps[0].max);
        other.remove(ps[1].unique_name);
        other.insert("not-a-param".into(), 1);
        let (diff, skipped) = changes(&data, 1, t, &other);
        let expect = if ps[0].max != ps[0].min { 1 } else { 0 };
        assert_eq!(diff.len(), expect);
        assert_eq!(skipped, 1);
        // out of range counts as missing
        other.insert(ps[0].unique_name.into(), ps[0].max + 1);
        assert_eq!(changes(&data, 1, t, &other).1, 2);
    }

    #[test]
    fn names() {
        assert!(valid_name("Big muff-ish"));
        assert!(!valid_name("  "));
        assert!(!valid_name("a\nb"));
    }
}
