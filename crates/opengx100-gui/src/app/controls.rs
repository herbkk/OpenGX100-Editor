//! Pedal- and amp-style controls for the FX editor: knobs, faders, toggle switches and option
//! buttons, and which one each FX parameter gets.
//!
//! Knobs and faders: drag up/down (Shift = fine), double-click = the parameter's default.
//! A click on a fader's track moves the cap there. The mouse wheel turns the control under the
//! pointer (Shift: one step at a time), but only once the pointer has rested on it: while the page
//! is scrolling, a control that slides under the pointer lets the wheel scroll on, so scrolling past
//! a control never changes a sound.

use super::eq_graph::Setting;
use super::{GREEN, SIGNAL};
use eframe::egui::{self, pos2, vec2, Align, Color32, FontId, Layout, Pos2, Rect, Sense, Stroke};
use opengx100_protocol::tables::{FxParamDef, FxTypeDef, Init};

/// Width of a knob's column (name, knob, value).
pub(super) const KNOB_W: f32 = 78.0;
/// Width of a fader's column.
pub(super) const FADER_W: f32 = 58.0;
const KNOB_R: f32 = 19.0;
const FADER_H: f32 = 128.0;
/// Room for a name of up to two lines above each knob and fader, so a row lines up.
const NAME_H: f32 = 26.0;
/// How far (in points) a knob is dragged to go from its minimum to its maximum.
const KNOB_DRAG: f32 = 220.0;

/// The kind of control an FX parameter gets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Control {
    Knob,
    Fader,
    /// OFF/ON.
    Switch,
    /// A short list: every option visible, one picked.
    Buttons,
    /// A long list of names: a drop-down.
    List,
}

/// Knobs for amounts and times (anything with a scale), faders for volumes and graphic EQ bands,
/// a switch for OFF/ON, option buttons for short lists, a drop-down for long lists of names.
/// On an amp (PREAMP) and a drive pedal every amount is a knob, as on the real thing.
pub(super) fn control_for(t: &FxTypeDef, p: &FxParamDef) -> Control {
    let labels = p.labels();
    let scale = if labels.is_empty() { p.max - p.min > 1 } else { labels.len() >= 6 && is_scale(labels) };
    if scale {
        return if is_volume(t, p) { Control::Fader } else { Control::Knob };
    }
    if p.max - p.min == 1 && (labels.is_empty() || is_off_on(labels)) {
        return Control::Switch;
    }
    if labels.len() < 6 {
        Control::Buttons
    } else {
        Control::List
    }
}

/// Labels that are values on a scale (times, rates, frequencies, ratios), not names.
fn is_scale(labels: &[&str]) -> bool {
    let numeric = labels
        .iter()
        .filter(|l| l.trim_start().starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '+'))
        .count();
    numeric * 5 >= labels.len() * 4
}

fn is_off_on(labels: &[&str]) -> bool {
    labels.len() == 2 && labels[0].eq_ignore_ascii_case("OFF") && labels[1].eq_ignore_ascii_case("ON")
}

/// The AIRD (BASS) PREAMP settings that are faders; the rest are knobs, as on the amp.
const AMP_FADERS: &[&str] = &["sag", "resonance", "direct-mix", "mic-position", "mic-level"];

/// Volumes (levels, mixes) and graphic EQ bands, but not on a drive pedal; on an amp, [`AMP_FADERS`].
fn is_volume(t: &FxTypeDef, p: &FxParamDef) -> bool {
    if t.name.contains("GRAPHIC EQ") {
        return true;
    }
    // The amp's knobs are its tone stack; its other settings (power amp, cabinet, mic) go on faders below.
    if t.name.contains("PREAMP") {
        return AMP_FADERS.contains(&p.unique_name);
    }
    // On drive pedals, LEVEL is a knob like DRIVE.
    if ["OVERDRIVE", "DISTORTION", "FUZZ", "BOOSTER"].iter().any(|k| t.name.contains(k)) {
        return false;
    }
    let name = short_name(p.name).1.to_uppercase();
    name.contains("LEVEL") || name.contains("VOLUME") || name == "DIRECT MIX" || p.unique_name == "pedal-position"
}

/// `"1: RATE"` → `(Some("1"), "RATE")`: some effects have sets of settings (a dual CHORUS's "1:"/"2:",
/// HARMONIST's "HR1:" scale, DIVIDER's "A:"/"B:" channels).
pub(super) fn short_name(name: &str) -> (Option<&str>, &str) {
    match name.split_once(": ") {
        Some((g, n))
            if (1..=4).contains(&g.len()) && g.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) =>
        {
            (Some(g), n)
        }
        _ => (None, name),
    }
}

/// The value the pedal starts a parameter at, if the tables say.
pub(super) fn default_value(init: Init, (min, max): (i32, i32)) -> Option<i32> {
    match init {
        Init::Num(n) => Some(n),
        Init::Text(s) => s.trim().parse().ok(),
    }
    .filter(|v| (min..=max).contains(v))
}

/// A parametric EQ as an EQ unit: one section per band, divided by lines, each with its knobs (frequency
/// over Q) and its gain fader on the right. Used by the PARAMETRIC EQUALIZER effect and the Global EQ;
/// `control` draws one knob or fader for a setting, with the given label.
pub(super) fn eq_sections(ui: &mut egui::Ui, mut control: impl FnMut(&mut egui::Ui, Setting, &str, Control)) {
    use Setting::*;
    /// A setting and its label.
    type Ctl = (Setting, &'static str);
    // (section, knobs top to bottom, fader)
    const BANDS: &[(&str, &[Ctl], Ctl)] = &[
        ("LOW", &[(LowCut, "LOW CUT")], (LowGain, "GAIN")),
        ("LOW-MID", &[(LowMidFreq, "FREQ"), (LowMidQ, "Q")], (LowMidGain, "GAIN")),
        ("HIGH-MID", &[(HighMidFreq, "FREQ"), (HighMidQ, "Q")], (HighMidGain, "GAIN")),
        ("HIGH", &[(HighCut, "HIGH CUT")], (HighGain, "GAIN")),
        ("OUTPUT", &[], (Level, "LEVEL")),
    ];
    row(ui, |ui| {
        // The dividing lines are drawn afterwards, as tall as the sections (a separator would run
        // down the whole panel).
        const GAP: f32 = 14.0;
        let mut rects: Vec<Rect> = Vec::new();
        for (i, (section, knobs, (fader, fader_name))) in BANDS.iter().enumerate() {
            if i > 0 {
                ui.add_space(GAP);
            }
            let r = ui.vertical(|ui| {
                ui.label(egui::RichText::new(*section).strong());
                ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                    if !knobs.is_empty() {
                        ui.vertical(|ui| {
                            for (k, label) in knobs.iter() {
                                control(ui, *k, label, Control::Knob);
                            }
                        });
                    }
                    control(ui, *fader, fader_name, Control::Fader);
                });
            });
            rects.push(r.response.rect);
        }
        let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
        for w in rects.windows(2) {
            // Only between sections side by side (not where the row wrapped).
            if (w[0].top() - w[1].top()).abs() < 1.0 {
                let x = (w[0].right() + w[1].left()) / 2.0;
                let bottom = w[0].bottom().max(w[1].bottom());
                ui.painter().vline(x, w[0].top()..=bottom, stroke);
            }
        }
    });
}

/// A row of knobs or faders, wrapping to the width there is. Top-aligned: in egui's centred rows each
/// column is centred on the row as it grows, which made the knobs step downwards.
pub(super) fn row<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.with_layout(Layout::left_to_right(Align::Min).with_main_wrap(true), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        add(ui)
    })
    .inner
}

/// What a knob or fader shows and does.
pub(super) struct Scale<'a> {
    pub name: &'a str,
    /// The name's colour (amber while the pedal changes the value).
    pub name_color: Color32,
    pub value: i32,
    pub min: i32,
    pub max: i32,
    pub default: Option<i32>,
    pub display: &'a dyn Fn(i32) -> String,
    /// The parameter guide's explanation, on hover.
    pub hover: Option<&'a str>,
}

impl Scale<'_> {
    fn frac(&self, v: i32) -> f32 {
        if self.max <= self.min {
            0.0
        } else {
            (v - self.min) as f32 / (self.max - self.min) as f32
        }
    }

    /// Where a range around zero (e.g. -20…+20 dB) has its zero.
    fn zero(&self) -> Option<f32> {
        (self.min < 0 && self.max > 0).then(|| self.frac(0))
    }
}

/// The name, centred at the bottom of a fixed-height area, so the controls of a row line up.
fn name_area(ui: &mut egui::Ui, width: f32, s: &Scale) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, NAME_H), Sense::hover());
    let font = FontId::proportional(11.0);
    let galley = ui.painter().layout(s.name.to_string(), font, s.name_color, width - 4.0);
    let pos = pos2(rect.center().x - galley.size().x / 2.0, rect.bottom() - galley.size().y);
    ui.painter().galley(pos, galley, s.name_color);
}

fn value_text(ui: &mut egui::Ui, text: String, active: bool) {
    let color = if active { ui.visuals().strong_text_color() } else { ui.visuals().text_color() };
    ui.label(egui::RichText::new(text).monospace().size(12.0).color(color));
}

/// Turns a drag on `resp` into a new value. `per_point`: how much one point of drag moves the value.
fn drag_value(ui: &egui::Ui, resp: &egui::Response, s: &Scale, per_point: f32) -> Option<i32> {
    let id = resp.id.with("drag");
    if resp.double_clicked() {
        return s.default.filter(|&d| d != s.value);
    }
    if resp.drag_started() {
        ui.ctx().data_mut(|d| d.insert_temp(id, s.value as f32));
    }
    if !resp.dragged() {
        return None;
    }
    let fine = ui.input(|i| i.modifiers.shift);
    let per_point = if fine { (per_point / 8.0).min(0.25) } else { per_point };
    let start = ui.ctx().data(|d| d.get_temp::<f32>(id)).unwrap_or(s.value as f32);
    let acc = (start - resp.drag_delta().y * per_point).clamp(s.min as f32, s.max as f32);
    ui.ctx().data_mut(|d| d.insert_temp(id, acc));
    let v = acc.round() as i32;
    (v != s.value).then_some(v)
}

const HELP: &str = "Drag up/down or use the mouse wheel (Shift: fine) · double-click: default";

/// How long the pointer must rest on a control before the wheel turns it.
const WHEEL_SETTLE: f64 = 0.25;
/// How long after the wheel last scrolled the page a control still lets it scroll.
const WHEEL_PAGE_GRACE: f64 = 0.5;
/// Wheel travel (points) per step.
const WHEEL_POINTS_PER_STEP: f32 = 12.0;

/// Turns the mouse wheel over `resp` into a new value (up = more). Takes the wheel from the page
/// only when the pointer has rested on the control and the page isn't being scrolled.
fn wheel_value(ui: &egui::Ui, resp: &egui::Response, s: &Scale) -> Option<i32> {
    let ctx = ui.ctx();
    let now = ui.input(|i| i.time);
    let hover_since = resp.id.with("hover-since");
    let page_scrolled = egui::Id::unique("controls-page-scrolled");
    if !resp.hovered() || resp.dragged() {
        ctx.data_mut(|d| d.remove::<f64>(hover_since));
        return None;
    }
    let since = ctx.data_mut(|d| *d.get_temp_mut_or_insert_with(hover_since, || now));
    let delta = ui.input(|i| {
        let d = i.smooth_scroll_delta;
        // macOS turns Shift + wheel into a sideways scroll.
        if d.y != 0.0 {
            d.y
        } else {
            d.x
        }
    });
    if delta == 0.0 {
        return None;
    }
    let last_page = ctx.data(|d| d.get_temp::<f64>(page_scrolled)).unwrap_or(f64::NEG_INFINITY);
    if now - since < WHEEL_SETTLE || now - last_page < WHEEL_PAGE_GRACE {
        // The page is scrolling past: let it.
        ctx.data_mut(|d| d.insert_temp(page_scrolled, now));
        return None;
    }
    ui.input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
    let acc_id = resp.id.with("wheel");
    let acc = ctx.data(|d| d.get_temp::<f32>(acc_id)).unwrap_or(0.0) + delta;
    let steps = (acc / WHEEL_POINTS_PER_STEP).trunc();
    ctx.data_mut(|d| d.insert_temp(acc_id, acc - steps * WHEEL_POINTS_PER_STEP));
    if steps == 0.0 {
        return None;
    }
    // Big ranges (a 2,000-step delay time) move about 1% a step; Shift: one step.
    let fine = ui.input(|i| i.modifiers.shift);
    let unit = if fine { 1 } else { ((s.max - s.min) / 100).max(1) };
    let v = (s.value + steps as i32 * unit).clamp(s.min, s.max);
    (v != s.value).then_some(v)
}

fn with_help(resp: egui::Response, s: &Scale) -> egui::Response {
    match s.hover {
        Some(h) => resp.on_hover_text(format!("{h}\n\n{HELP}")),
        None => resp.on_hover_text(HELP),
    }
}

/// A knob or a fader, as `kind` says.
pub(super) fn knob_or_fader(ui: &mut egui::Ui, kind: Control, s: &Scale) -> Option<i32> {
    match kind {
        Control::Fader => fader(ui, s),
        _ => knob(ui, s),
    }
}

/// An amp-style knob: the name above, the value below. Returns the new value when changed.
pub(super) fn knob(ui: &mut egui::Ui, s: &Scale) -> Option<i32> {
    ui.allocate_ui_with_layout(vec2(KNOB_W, 0.0), Layout::top_down(Align::Center), |ui| {
        ui.set_width(KNOB_W);
        name_area(ui, KNOB_W, s);
        let side = 2.0 * KNOB_R + 12.0;
        let (rect, resp) = ui.allocate_exact_size(vec2(side, side), Sense::click_and_drag());
        let resp = with_help(resp, s);
        let per_point = (s.max - s.min).max(1) as f32 / KNOB_DRAG;
        let new = drag_value(ui, &resp, s, per_point).or_else(|| wheel_value(ui, &resp, s));
        let shown = new.unwrap_or(s.value);
        paint_knob(ui, rect, s, shown, resp.hovered() || resp.dragged());
        value_text(ui, (s.display)(shown), resp.dragged());
        new
    })
    .inner
}

/// Angle (radians, 0 = up, clockwise) of a knob at `frac` of its travel: 7 o'clock to 5 o'clock.
fn knob_angle(frac: f32) -> f32 {
    let sweep = 135f32.to_radians();
    -sweep + 2.0 * sweep * frac
}

fn paint_knob(ui: &egui::Ui, rect: Rect, s: &Scale, value: i32, hot: bool) {
    let p = ui.painter();
    let c = rect.center();
    let at = |a: f32, r: f32| c + vec2(a.sin(), -a.cos()) * r;
    let arc = |from: f32, to: f32, r: f32| -> Vec<Pos2> {
        let n = ((to - from).abs() * 12.0).ceil().max(1.0) as usize;
        (0..=n).map(|i| at(from + (to - from) * i as f32 / n as f32, r)).collect()
    };
    let track_r = KNOB_R + 4.5;
    p.line(arc(knob_angle(0.0), knob_angle(1.0), track_r), Stroke::new(3.0, Color32::from_gray(60)));
    let from = knob_angle(s.zero().unwrap_or(0.0));
    let to = knob_angle(s.frac(value));
    if (to - from).abs() > 0.001 {
        p.line(arc(from, to, track_r), Stroke::new(3.0, SIGNAL));
    }
    // The knob: a dark skirt, a cap, and a white pointer line.
    let rim = if hot { Color32::from_gray(150) } else { Color32::from_gray(95) };
    p.circle_filled(c, KNOB_R, Color32::from_gray(22));
    p.circle_stroke(c, KNOB_R, Stroke::new(1.5, rim));
    p.circle_filled(c, KNOB_R * 0.7, Color32::from_gray(38));
    p.line_segment([at(to, KNOB_R * 0.2), at(to, KNOB_R * 0.92)], Stroke::new(2.5, Color32::WHITE));
}

/// A mixer-style vertical fader: the name above, the value below. Returns the new value when changed.
pub(super) fn fader(ui: &mut egui::Ui, s: &Scale) -> Option<i32> {
    ui.allocate_ui_with_layout(vec2(FADER_W, 0.0), Layout::top_down(Align::Center), |ui| {
        ui.set_width(FADER_W);
        name_area(ui, FADER_W, s);
        let (rect, resp) = ui.allocate_exact_size(vec2(40.0, FADER_H), Sense::click_and_drag());
        let resp = with_help(resp, s);
        let (top, bottom) = (rect.top() + 8.0, rect.bottom() - 8.0);
        let per_point = (s.max - s.min).max(1) as f32 / (bottom - top);
        let mut new = drag_value(ui, &resp, s, per_point).or_else(|| wheel_value(ui, &resp, s));
        // A click on the track puts the cap there.
        if resp.clicked() && !resp.double_clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let frac = ((bottom - pos.y) / (bottom - top)).clamp(0.0, 1.0);
                let v = s.min + (frac * (s.max - s.min) as f32).round() as i32;
                new = (v != s.value).then_some(v);
            }
        }
        let shown = new.unwrap_or(s.value);
        paint_fader(ui, rect, (top, bottom), s, shown, resp.hovered() || resp.dragged());
        value_text(ui, (s.display)(shown), resp.dragged());
        new
    })
    .inner
}

fn paint_fader(ui: &egui::Ui, rect: Rect, (top, bottom): (f32, f32), s: &Scale, value: i32, hot: bool) {
    let p = ui.painter();
    let x = rect.center().x;
    let y = |frac: f32| bottom - frac * (bottom - top);
    // Scale marks every tenth, the zero (or bottom) mark longer.
    for i in 0..=10 {
        let yy = y(i as f32 / 10.0);
        let w = if i % 5 == 0 { 9.0 } else { 6.0 };
        let col = Color32::from_gray(80);
        p.line_segment([pos2(x - 6.0 - w, yy), pos2(x - 6.0, yy)], Stroke::new(1.0, col));
        p.line_segment([pos2(x + 6.0, yy), pos2(x + 6.0 + w, yy)], Stroke::new(1.0, col));
    }
    let base = s.zero().unwrap_or(0.0);
    if s.zero().is_some() {
        let yy = y(base);
        p.line_segment([pos2(x - 17.0, yy), pos2(x + 17.0, yy)], Stroke::new(1.5, Color32::from_gray(140)));
    }
    // The slot, lit from the zero (or bottom) up to the cap.
    p.rect_filled(Rect::from_min_max(pos2(x - 2.5, top), pos2(x + 2.5, bottom)), 2.0, Color32::from_gray(15));
    let (a, b) = (y(base), y(s.frac(value)));
    p.rect_filled(Rect::from_min_max(pos2(x - 2.0, a.min(b)), pos2(x + 2.0, a.max(b))), 2.0, SIGNAL);
    // The cap.
    let cap = Rect::from_center_size(pos2(x, b), vec2(30.0, 13.0));
    let fill = if hot { Color32::from_gray(225) } else { Color32::from_gray(190) };
    p.rect_filled(cap, 3.0, fill);
    p.rect_stroke(cap, 3.0, Stroke::new(1.0, Color32::from_gray(60)), egui::StrokeKind::Inside);
    p.line_segment([pos2(cap.left() + 3.0, b), pos2(cap.right() - 3.0, b)], Stroke::new(1.5, Color32::from_gray(30)));
}

/// A toggle switch with ON/OFF beside it. Returns the new state when clicked.
pub(super) fn switch(ui: &mut egui::Ui, on: bool) -> Option<bool> {
    let state = egui::RichText::new(if on { "ON" } else { "OFF" }).strong().color(if on {
        GREEN
    } else {
        Color32::from_gray(140)
    });
    switch_with(ui, on, state)
}

/// A toggle switch with `label` beside it. Returns the new state when clicked.
pub(super) fn switch_labelled(ui: &mut egui::Ui, on: bool, label: &str) -> Option<bool> {
    switch_with(ui, on, egui::RichText::new(label))
}

fn switch_with(ui: &mut egui::Ui, on: bool, text: egui::RichText) -> Option<bool> {
    let h = 18.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(2.1 * h, h), Sense::click());
    let t = ui.ctx().animate_bool(resp.id, on);
    let bg = if on { GREEN.gamma_multiply(0.85) } else { Color32::from_gray(70) };
    let p = ui.painter();
    p.rect_filled(rect, h / 2.0, bg);
    let r = h / 2.0 - 2.5;
    let x = egui::lerp(rect.left() + h / 2.0..=rect.right() - h / 2.0, t);
    let knob = if resp.hovered() { Color32::WHITE } else { Color32::from_gray(225) };
    p.circle_filled(pos2(x, rect.center().y), r, knob);
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let label = ui.add(egui::Label::new(text).sense(Sense::click()));
    (resp.clicked() || label.clicked()).then_some(!on)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_are_told_from_names() {
        assert!(is_scale(&["0.5", "1", "2", "4", "8", "16"]));
        assert!(is_scale(&["FLAT", "20.0Hz", "25.0Hz", "31.5Hz", "40.0Hz", "50.0Hz"]));
        assert!(!is_scale(&["DYN57", "DYN421", "CND451", "CND87", "RBN121", "FLAT"]));
        assert!(!is_scale(&["P1", "P2", "P3", "P4", "P5", "P6"]));
    }

    #[test]
    fn numbered_sets() {
        assert_eq!(short_name("1: RATE"), (Some("1"), "RATE"));
        assert_eq!(short_name("RATE"), (None, "RATE"));
        assert_eq!(short_name("HR1: C"), (Some("HR1"), "C"));
        assert_eq!(short_name("a: b"), (None, "a: b"));
        assert_eq!(short_name("A/B BALANCE"), (None, "A/B BALANCE"));
    }

    #[test]
    fn knob_travel() {
        assert!((knob_angle(0.0) + 135f32.to_radians()).abs() < 1e-6);
        assert!(knob_angle(0.5).abs() < 1e-6);
    }
}
