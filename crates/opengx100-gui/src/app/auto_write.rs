//! Auto-write: WRITE the live patch into the patch the pedal is on every few minutes, when it has
//! changes. The same WRITE as the dialog's (name, then PATCH_WRITE), into the current patch, under the
//! live patch's own name. The period runs from the last write (ours, the dialog's or the pedal's).

use super::{EditorApp, View, AMBER};
use crate::worker::{Cmd, PatchId};
use eframe::egui::{self, RichText};
use std::time::{Duration, Instant};

const MINUTES: std::ops::RangeInclusive<u32> = 1..=120;

impl EditorApp {
    fn auto_write_period(&self) -> Duration {
        Duration::from_secs(u64::from(self.settings.auto_write_minutes.clamp(*MINUTES.start(), *MINUTES.end())) * 60)
    }

    /// Called every frame: writes when the period is over and the live patch has changes. Waits while
    /// something else is going on (a dialog, a drag, a chain edit, a setlist write, the tuner).
    pub(super) fn auto_write(&mut self, ctx: &egui::Context) {
        if !self.settings.auto_write {
            self.auto_write_from = None;
            return;
        }
        // Once a second, for the countdown next to the switch.
        ctx.request_repaint_after(Duration::from_secs(1));
        let start = *self.auto_write_from.get_or_insert_with(Instant::now);
        if start.elapsed() < self.auto_write_period() {
            return;
        }
        let busy = !self.connected()
            || self.dialog.is_some()
            || self.chain_busy
            || self.setlist_write.is_some()
            || self.view == View::Tuner
            || ctx.dragged_id().is_some()
            || self.loading.contains(&PatchId::Temp);
        if busy {
            return;
        }
        self.auto_write_from = Some(Instant::now());
        if self.unsaved_changes() != Some(true) {
            return;
        }
        let (Some(target), Some(name)) = (self.current, self.patches.get(&PatchId::Temp).map(|p| p.name())) else {
            return;
        };
        crate::log::line(&format!("auto-write: patch {target}"));
        self.auto_writing = Some(target);
        let _ = self.tx.send(Cmd::SavePatch { target, name });
    }

    /// A WRITE happened (any kind): the next auto-write is a whole period away.
    pub(super) fn auto_write_restart(&mut self) {
        if self.settings.auto_write {
            self.auto_write_from = Some(Instant::now());
        }
    }

    /// The switch and the period, next to the Write… button.
    pub(super) fn auto_write_controls(&mut self, ui: &mut egui::Ui) {
        let before = self.settings.clone();
        // The header lays out right to left: add the pieces in reverse there, so they read left to right.
        let rtl = ui.layout().prefer_right_to_left();
        ui.horizontal(|ui| {
            let mut order = [0, 1, 2, 3];
            if rtl {
                order.reverse();
            }
            for piece in order {
                match piece {
                    0 => {
                        if let Some(on) = super::controls::switch_labelled(ui, self.settings.auto_write, "Auto-write") {
                            self.settings.auto_write = on;
                            self.auto_write_from = None; // a full period from now
                        }
                    }
                    1 => {
                        ui.label("every");
                    }
                    2 => {
                        ui.add(
                            egui::DragValue::new(&mut self.settings.auto_write_minutes)
                                .range(MINUTES)
                                .suffix(" min")
                                .speed(0.2),
                        );
                    }
                    _ if self.settings.auto_write => {
                        let left = self
                            .auto_write_from
                            .map(|t| self.auto_write_period().saturating_sub(t.elapsed()))
                            .unwrap_or(self.auto_write_period());
                        let s = left.as_secs();
                        let text = match self.unsaved_changes() {
                            Some(true) => RichText::new(format!("next in {}:{:02}", s / 60, s % 60)).color(AMBER),
                            _ => RichText::new("nothing to write").weak(),
                        };
                        ui.label(text);
                    }
                    _ => {}
                }
            }
        })
        .response
        .on_hover_text(
            "Writes the live patch into the patch the pedal is on, under its current name, every few minutes \
             while it has changes. Waits while a dialog is open, a control is being dragged or the tuner is shown.",
        );
        if self.settings != before {
            if let Some(store) = &self.store {
                if let Err(e) = store.save_settings(&self.settings) {
                    self.last_error = Some(format!("couldn't save the settings: {e}"));
                }
            }
        }
    }
}
