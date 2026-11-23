//! The frequency response of the PARAMETRIC EQUALIZER effect and of the global EQ, which have the
//! same settings: low cut, low shelf, two peaking bands, high shelf, high cut and level.
//!
//! The curve is our own approximation, drawn from textbook filters: the pedal doesn't say at which
//! frequencies its shelves sit or how steep its cut filters are. The dots can be dragged; the
//! settings they change are written like any other edit.

use super::SIGNAL;
use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, Sense, Stroke};

/// The settings both EQs share.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Setting {
    LowCut,
    LowGain,
    LowMidFreq,
    LowMidQ,
    LowMidGain,
    HighMidFreq,
    HighMidQ,
    HighMidGain,
    HighGain,
    HighCut,
    Level,
}

impl Setting {
    pub(super) const ALL: [Setting; 11] = [
        Setting::LowCut,
        Setting::LowGain,
        Setting::LowMidFreq,
        Setting::LowMidQ,
        Setting::LowMidGain,
        Setting::HighMidFreq,
        Setting::HighMidQ,
        Setting::HighMidGain,
        Setting::HighGain,
        Setting::HighCut,
        Setting::Level,
    ];

    /// The parameter of the PARAMETRIC EQUALIZER effect.
    pub(super) fn fx_name(self) -> &'static str {
        match self {
            Setting::LowCut => "low-cut",
            Setting::LowGain => "low-gain",
            Setting::LowMidFreq => "low-mid-freq",
            Setting::LowMidQ => "low-mid-q",
            Setting::LowMidGain => "low-mid-gain",
            Setting::HighMidFreq => "high-mid-freq",
            Setting::HighMidQ => "high-mid-q",
            Setting::HighMidGain => "high-mid-gain",
            Setting::HighGain => "high-gain",
            Setting::HighCut => "high-cut",
            Setting::Level => "level",
        }
    }

    /// The global EQ parameter (the current values, as the IN/OUT page edits them).
    pub(super) fn global_path(self) -> &'static str {
        match self {
            Setting::LowCut => "Setup_efct.IOOutGlobalEqLowCut",
            Setting::LowGain => "Setup_efct.IOOutGlobalEqLowGain",
            Setting::LowMidFreq => "Setup_efct.IOOutGlobalEqLowMidFreq",
            Setting::LowMidQ => "Setup_efct.IOOutGlobalEqLowMidQ",
            Setting::LowMidGain => "Setup_efct.IOOutGlobalEqLowMidGain",
            Setting::HighMidFreq => "Setup_efct.IOOutGlobalEqHighMidFreq",
            Setting::HighMidQ => "Setup_efct.IOOutGlobalEqHighMidQ",
            Setting::HighMidGain => "Setup_efct.IOOutGlobalEqHighMidGain",
            Setting::HighGain => "Setup_efct.IOOutGlobalEqHighGain",
            Setting::HighCut => "Setup_efct.IOOutGlobalEqHighCut",
            Setting::Level => "Setup_efct.IOOutGlobalEqLevel",
        }
    }
}

/// The frequency steps of the FREQ and cut settings: third-octave centres, 20 Hz–12.5 kHz.
const FREQS: [f32; 29] = [
    20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0, 500.0, 630.0, 800.0,
    1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0, 5000.0, 6300.0, 8000.0, 10000.0, 12500.0,
];
/// LOW CUT 0 and HIGH CUT 29 are FLAT (no filter).
const LOW_CUT_FLAT: i32 = 0;
const HIGH_CUT_FLAT: i32 = 29;
/// Where the shelves are drawn. Assumed: the pedal doesn't say.
const LOW_SHELF_HZ: f32 = 100.0;
const HIGH_SHELF_HZ: f32 = 8000.0;
const GAIN_RANGE: (i32, i32) = (-20, 20);

/// The plotted range.
const F_MIN: f32 = 15.0;
const F_MAX: f32 = 20000.0;
/// Where a cut filter set to FLAT has its dot (at the edges, outside the steps).
const LOW_FLAT_HZ: f32 = 16.0;
const HIGH_FLAT_HZ: f32 = 17000.0;
const DB_RANGE: f32 = 25.0;

fn step_freq(i: i32) -> f32 {
    FREQS[i.clamp(0, FREQS.len() as i32 - 1) as usize]
}

/// Q 0.5, 1, 2, 4, 8, 16.
fn q(i: i32) -> f32 {
    0.5 * 2f32.powi(i.clamp(0, 5))
}

/// The stored values of an EQ.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Eq {
    values: [i32; 11],
    /// Off: the curve is drawn dimmed.
    pub(super) on: bool,
}

impl Eq {
    /// Reads every setting; `None` when one is missing.
    pub(super) fn read(on: bool, get: impl Fn(Setting) -> Option<i32>) -> Option<Eq> {
        let mut values = [0; 11];
        for (v, s) in values.iter_mut().zip(Setting::ALL) {
            *v = get(s)?;
        }
        Some(Eq { values, on })
    }

    fn get(&self, s: Setting) -> i32 {
        self.values[s as usize]
    }

    fn low_cut(&self) -> Option<f32> {
        let v = self.get(Setting::LowCut);
        (v != LOW_CUT_FLAT).then(|| step_freq(v - 1))
    }

    fn high_cut(&self) -> Option<f32> {
        let v = self.get(Setting::HighCut);
        (v != HIGH_CUT_FLAT).then(|| step_freq(v))
    }

    /// The response at `f` Hz, in dB (level included).
    pub(super) fn response_db(&self, f: f32) -> f32 {
        let mut db = self.get(Setting::Level) as f32;
        if let Some(fc) = self.low_cut() {
            db += high_pass_db(f / fc);
        }
        if let Some(fc) = self.high_cut() {
            db += high_pass_db(fc / f);
        }
        db += low_shelf_db(f / LOW_SHELF_HZ, self.get(Setting::LowGain) as f32);
        db += high_shelf_db(f / HIGH_SHELF_HZ, self.get(Setting::HighGain) as f32);
        for (freq, qs, gain) in [
            (Setting::LowMidFreq, Setting::LowMidQ, Setting::LowMidGain),
            (Setting::HighMidFreq, Setting::HighMidQ, Setting::HighMidGain),
        ] {
            db += peak_db(f / step_freq(self.get(freq)), q(self.get(qs)), self.get(gain) as f32);
        }
        db
    }
}

// Magnitudes of analog second-order sections at normalised frequency w = f / f0 (shelves and peaks
// as in R. Bristow-Johnson's "Audio EQ Cookbook"; cuts as 12 dB/octave Butterworth filters).

fn db_of_power(p: f32) -> f32 {
    10.0 * p.max(1e-12).log10()
}

/// High pass; a low pass is the same at 1/w.
fn high_pass_db(w: f32) -> f32 {
    let w4 = w.powi(4);
    db_of_power(w4 / (1.0 + w4))
}

fn peak_db(w: f32, q: f32, gain: f32) -> f32 {
    let a = 10f32.powf(gain / 40.0);
    let re = (1.0 - w * w).powi(2);
    db_of_power((re + (a * w / q).powi(2)) / (re + (w / (a * q)).powi(2)))
}

/// Shelf slope S = 1 (1/Q² = 2).
fn low_shelf_db(w: f32, gain: f32) -> f32 {
    let a = 10f32.powf(gain / 40.0);
    let w2 = w * w;
    let mid = 2.0 * a * w2;
    db_of_power(a * a * ((a - w2).powi(2) + mid) / ((1.0 - a * w2).powi(2) + mid))
}

fn high_shelf_db(w: f32, gain: f32) -> f32 {
    let a = 10f32.powf(gain / 40.0);
    let w2 = w * w;
    let mid = 2.0 * a * w2;
    db_of_power(a * a * ((1.0 - a * w2).powi(2) + mid) / ((a - w2).powi(2) + mid))
}

fn hz_text(f: f32) -> String {
    if f >= 1000.0 {
        format!("{}k", (f / 100.0).round() / 10.0)
    } else {
        format!("{}", f)
    }
}

fn db_text(db: i32) -> String {
    format!("{}{db} dB", if db > 0 { "+" } else { "" })
}

/// What a frequency range does to an electric guitar's sound: (from Hz, to Hz, name, what boosting or
/// cutting it does). Rules of thumb (after Neural DSP's "Electric guitar EQ guide" and the GMC wiki's
/// "Parametric EQ" page); the borders are approximate.
const REGIONS: &[(f32, f32, &str, &str)] = &[
    (
        F_MIN,
        80.0,
        "Rumble",
        "Mostly mud and noise on guitar. Cutting it (LOW CUT around 70–90 Hz) tightens the tone; \
         boosting makes it flabby and noisy.",
    ),
    (
        80.0,
        150.0,
        "Low end",
        "Depth and warmth. A small boost near 80–100 Hz fills out the tone; big boosts boom and clash \
         with bass and kick drum.",
    ),
    (
        150.0,
        250.0,
        "Body",
        "Thickness: where the guitar's body comes from. Boost a little for a fuller solo; cut when the \
         mix gets thick.",
    ),
    (
        250.0,
        500.0,
        "Mud",
        "Cutting a few dB clears muddiness and focuses the tone (common for high-gain rhythm); boosting \
         sounds warm but cloudy.",
    ),
    (
        500.0,
        1000.0,
        "Boxy / life",
        "Much of what makes an electric guitar sound electric. Around 800 Hz adds clarity; too much \
         sounds boxy, too little sounds hollow.",
    ),
    (
        1000.0,
        2000.0,
        "Honk",
        "Nasal, honky midrange. A wide cut smooths it out; a boost pushes the guitar forward but can \
         sound like a telephone.",
    ),
    (
        2000.0,
        4000.0,
        "Edge / bite",
        "Attack and aggression. Boosting around 3 kHz adds bite and cut; too much becomes harsh and \
         tiring.",
    ),
    (
        4000.0,
        8000.0,
        "Presence",
        "Articulation, brilliance and presence (around 5 kHz). Boosting helps the guitar cut through but \
         brings out noise and fizz; cutting makes room for vocals.",
    ),
    (
        8000.0,
        F_MAX,
        "Air / fizz",
        "Sparkle and air on clean tones; fizz and hiss on distorted ones. Cutting above about 10 kHz \
         (HIGH CUT) removes harshness.",
    ),
];

fn region(f: f32) -> &'static (f32, f32, &'static str, &'static str) {
    REGIONS.iter().find(|r| f < r.1).unwrap_or(&REGIONS[REGIONS.len() - 1])
}

/// A draggable dot.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Band {
    LowCut,
    LowShelf,
    LowMid,
    HighMid,
    HighShelf,
    HighCut,
}

const BANDS: [(Band, Color32); 6] = [
    (Band::LowCut, Color32::from_rgb(200, 160, 90)),
    (Band::LowShelf, Color32::from_rgb(210, 90, 90)),
    (Band::LowMid, Color32::from_rgb(150, 200, 70)),
    (Band::HighMid, Color32::from_rgb(70, 190, 170)),
    (Band::HighShelf, Color32::from_rgb(110, 110, 220)),
    (Band::HighCut, Color32::from_rgb(200, 90, 200)),
];

/// Maps frequencies and dB to points of the plot.
#[derive(Clone, Copy)]
struct Plot(Rect);

impl Plot {
    fn x(self, f: f32) -> f32 {
        let t = (f.ln() - F_MIN.ln()) / (F_MAX.ln() - F_MIN.ln());
        self.0.left() + t * self.0.width()
    }

    fn freq(self, x: f32) -> f32 {
        let t = (x - self.0.left()) / self.0.width();
        (F_MIN.ln() + t * (F_MAX.ln() - F_MIN.ln())).exp()
    }

    fn y(self, db: f32) -> f32 {
        self.0.center().y - db / DB_RANGE * self.0.height() / 2.0
    }

    fn db(self, y: f32) -> f32 {
        (self.0.center().y - y) / (self.0.height() / 2.0) * DB_RANGE
    }
}

/// The value among `choices` (value, frequency) closest to `f` on the log scale.
fn nearest(f: f32, choices: impl Iterator<Item = (i32, f32)>) -> i32 {
    choices.min_by(|a, b| (a.1.ln() - f.ln()).abs().total_cmp(&(b.1.ln() - f.ln()).abs())).map_or(0, |c| c.0)
}

fn steps() -> impl Iterator<Item = (i32, f32)> {
    FREQS.iter().enumerate().map(|(i, &f)| (i as i32, f))
}

/// Draws the response of `eq`. Returns the settings the user changed by dragging (or double-clicking)
/// a dot, with their new values.
pub(super) fn graph(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, eq: &Eq) -> Vec<(Setting, i32)> {
    let id = ui.make_persistent_id(id);
    let width = ui.available_width().clamp(320.0, 760.0);
    let (outer, hover) = ui.allocate_exact_size(vec2(width, 200.0), Sense::hover());
    let plot = Plot(Rect::from_min_max(outer.min + vec2(44.0, 8.0), outer.max - vec2(10.0, 20.0)));
    let painter = ui.painter_at(outer);
    let visuals = ui.visuals();
    let text = visuals.weak_text_color();
    let font = egui::FontId::proportional(10.0);
    painter.rect_filled(plot.0, 2.0, visuals.extreme_bg_color);

    // Grid: a line per 1-2-…-9 step of each decade, labelled at 20, 50, 100…; every 5 dB.
    let minor = Stroke::new(0.5, visuals.weak_text_color().gamma_multiply(0.25));
    let major = Stroke::new(0.5, visuals.weak_text_color().gamma_multiply(0.6));
    for decade in [10.0f32, 100.0, 1000.0, 10000.0] {
        for k in 1..10 {
            let f = decade * k as f32;
            if !(F_MIN..=F_MAX).contains(&f) {
                continue;
            }
            let x = plot.x(f);
            let labelled = matches!(k, 1 | 2 | 5);
            painter.vline(x, plot.0.y_range(), if labelled { major } else { minor });
            if labelled && f < F_MAX {
                painter.text(pos2(x, plot.0.bottom() + 3.0), egui::Align2::CENTER_TOP, hz_text(f), font.clone(), text);
            }
        }
    }
    for db in (-20..=20).step_by(5) {
        let y = plot.y(db as f32);
        let stroke = if db == 0 {
            Stroke::new(1.0, text)
        } else if db % 10 == 0 {
            major
        } else {
            minor
        };
        painter.hline(plot.0.x_range(), y, stroke);
        if db % 10 == 0 {
            let label = if db > 0 { format!("+{db}") } else { db.to_string() };
            painter.text(pos2(plot.0.left() - 4.0, y), egui::Align2::RIGHT_CENTER, label, font.clone(), text);
        }
    }
    painter.text(pos2(outer.left() + 2.0, plot.0.center().y), egui::Align2::LEFT_CENTER, "dB", font.clone(), text);
    painter.text(plot.0.right_bottom() + vec2(0.0, 3.0), egui::Align2::RIGHT_TOP, "Hz", font, text);

    // Where the dots go: at gain + level, so that they sit on (or near) the curve.
    let level = eq.get(Setting::Level);
    let dots: Vec<(f32, i32, bool, Pos2)> = BANDS
        .iter()
        .map(|&(band, _)| {
            let (f, gain, active) = match band {
                Band::LowCut => (eq.low_cut().unwrap_or(LOW_FLAT_HZ), 0, eq.low_cut().is_some()),
                Band::HighCut => (eq.high_cut().unwrap_or(HIGH_FLAT_HZ), 0, eq.high_cut().is_some()),
                Band::LowShelf => (LOW_SHELF_HZ, eq.get(Setting::LowGain), true),
                Band::HighShelf => (HIGH_SHELF_HZ, eq.get(Setting::HighGain), true),
                Band::LowMid => (step_freq(eq.get(Setting::LowMidFreq)), eq.get(Setting::LowMidGain), true),
                Band::HighMid => (step_freq(eq.get(Setting::HighMidFreq)), eq.get(Setting::HighMidGain), true),
            };
            let at = pos2(plot.x(f), plot.y((gain + level) as f32).clamp(plot.0.top(), plot.0.bottom()));
            (f, gain, active, at)
        })
        .collect();

    // Hovering the plot (not a dot, not while dragging): a line at the pointer, the frequency range
    // it falls in shaded, and what that range does to the sound.
    let pointer = ui
        .ctx()
        .pointer_hover_pos()
        .filter(|p| plot.0.contains(*p) && ui.is_rect_visible(plot.0))
        .filter(|p| !dots.iter().any(|d| d.3.distance(*p) <= 9.0))
        .filter(|_| !ui.input(|i| i.pointer.any_down()));
    if let Some(p) = pointer {
        let f = plot.freq(p.x);
        let &(lo, hi, name, about) = region(f);
        let band = Rect::from_x_y_ranges(plot.x(lo)..=plot.x(hi), plot.0.y_range());
        painter.rect_filled(band.intersect(plot.0), 0.0, SIGNAL.gamma_multiply(0.08));
        painter.vline(p.x, plot.0.y_range(), Stroke::new(1.0, visuals.text_color().gamma_multiply(0.6)));
        let range = format!("{}–{} Hz", hz_text(lo.max(20.0)), hz_text(hi));
        let at = format!("{} Hz: {:+.1} dB", hz_text(f.round()), eq.response_db(f));
        // A readout beside the line that follows the pointer (the tooltip waits until it stops).
        let galley = painter.layout_no_wrap(
            format!("{} · {name}", at),
            egui::FontId::proportional(11.0),
            visuals.strong_text_color(),
        );
        let right = p.x + 6.0 + galley.size().x <= plot.0.right();
        let x = if right { p.x + 6.0 } else { p.x - 6.0 - galley.size().x };
        let tag = Rect::from_min_size(pos2(x, plot.0.top() + 3.0), galley.size()).expand(2.0);
        painter.rect_filled(tag, 2.0, visuals.extreme_bg_color.gamma_multiply(0.9));
        painter.galley(tag.min + vec2(2.0, 2.0), galley, visuals.strong_text_color());
        hover.on_hover_ui_at_pointer(|ui| {
            ui.set_max_width(260.0);
            ui.label(egui::RichText::new(at).strong());
            ui.label(egui::RichText::new(format!("{name} · {range}")).color(SIGNAL));
            ui.label(about);
        });
    }

    // The curve.
    let n = plot.0.width().max(2.0) as usize;
    let points: Vec<Pos2> = (0..=n)
        .map(|i| {
            let x = plot.0.left() + plot.0.width() * i as f32 / n as f32;
            pos2(x, plot.y(eq.response_db(plot.freq(x))))
        })
        .collect();
    let curve = if eq.on { SIGNAL } else { visuals.weak_text_color() };
    painter.with_clip_rect(plot.0).add(egui::Shape::line(points, Stroke::new(2.0, curve)));

    // The dots.
    let mut changes = Vec::new();
    for (i, (&(band, color), &(f, gain, active, at))) in BANDS.iter().zip(&dots).enumerate() {
        let r = ui.interact(Rect::from_center_size(at, vec2(16.0, 16.0)), id.with(i), Sense::click_and_drag());
        let hot = r.hovered() || r.dragged();
        let radius = if hot { 7.0 } else { 5.5 };
        if active {
            painter.circle(at, radius, color, Stroke::new(if hot { 1.5 } else { 0.0 }, Color32::WHITE));
        } else {
            painter.circle_stroke(at, radius, Stroke::new(1.5, color));
        }
        if r.dragged() || r.drag_started() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if r.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }

        let gain_at = |p: Pos2| (plot.db(p.y) - level as f32).round().clamp(GAIN_RANGE.0 as f32, GAIN_RANGE.1 as f32);
        let mut set = |s: Setting, v: i32| {
            if eq.get(s) != v {
                changes.push((s, v));
            }
        };
        if r.double_clicked() {
            // Back to neutral: no gain, or no cut.
            match band {
                Band::LowCut => set(Setting::LowCut, LOW_CUT_FLAT),
                Band::HighCut => set(Setting::HighCut, HIGH_CUT_FLAT),
                Band::LowShelf => set(Setting::LowGain, 0),
                Band::HighShelf => set(Setting::HighGain, 0),
                Band::LowMid => set(Setting::LowMidGain, 0),
                Band::HighMid => set(Setting::HighMidGain, 0),
            }
        } else if let Some(p) = r.dragged().then(|| r.interact_pointer_pos()).flatten() {
            let f = plot.freq(p.x);
            match band {
                Band::LowCut => {
                    let flat = std::iter::once((LOW_CUT_FLAT, LOW_FLAT_HZ));
                    set(Setting::LowCut, nearest(f, flat.chain(steps().map(|(i, f)| (i + 1, f)))));
                }
                Band::HighCut => {
                    let flat = std::iter::once((HIGH_CUT_FLAT, HIGH_FLAT_HZ));
                    set(Setting::HighCut, nearest(f, steps().chain(flat)));
                }
                Band::LowShelf => set(Setting::LowGain, gain_at(p) as i32),
                Band::HighShelf => set(Setting::HighGain, gain_at(p) as i32),
                Band::LowMid => {
                    set(Setting::LowMidFreq, nearest(f, steps()));
                    set(Setting::LowMidGain, gain_at(p) as i32);
                }
                Band::HighMid => {
                    set(Setting::HighMidFreq, nearest(f, steps()));
                    set(Setting::HighMidGain, gain_at(p) as i32);
                }
            }
        }

        if !r.dragged() {
            let what = match band {
                Band::LowCut => match eq.low_cut() {
                    Some(f) => format!("LOW CUT {} Hz\nDrag sideways; double-click: FLAT", hz_text(f)),
                    None => "LOW CUT FLAT\nDrag right to cut lows".to_string(),
                },
                Band::HighCut => match eq.high_cut() {
                    Some(f) => format!("HIGH CUT {} Hz\nDrag sideways; double-click: FLAT", hz_text(f)),
                    None => "HIGH CUT FLAT\nDrag left to cut highs".to_string(),
                },
                Band::LowShelf => format!("LOW {}\nDrag up/down; double-click: 0 dB", db_text(gain)),
                Band::HighShelf => format!("HIGH {}\nDrag up/down; double-click: 0 dB", db_text(gain)),
                Band::LowMid | Band::HighMid => {
                    let (name, qs) = if band == Band::LowMid {
                        ("LOW-MID", Setting::LowMidQ)
                    } else {
                        ("HIGH-MID", Setting::HighMidQ)
                    };
                    format!(
                        "{name} {} Hz · Q {} · {}\nDrag to change frequency and gain; double-click: 0 dB",
                        hz_text(f),
                        q(eq.get(qs)),
                        db_text(gain)
                    )
                }
            };
            r.on_hover_text(what);
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat() -> Eq {
        let mut values = [0; 11];
        values[Setting::LowMidFreq as usize] = 14;
        values[Setting::HighMidFreq as usize] = 23;
        values[Setting::LowMidQ as usize] = 1;
        values[Setting::HighMidQ as usize] = 1;
        values[Setting::HighCut as usize] = HIGH_CUT_FLAT;
        Eq { values, on: true }
    }

    #[test]
    fn neutral_settings_are_flat() {
        let eq = flat();
        for f in [20.0, 100.0, 1000.0, 10000.0, 20000.0] {
            assert!(eq.response_db(f).abs() < 0.01, "{f} Hz: {}", eq.response_db(f));
        }
    }

    #[test]
    fn bands_reach_their_gain() {
        let mut eq = flat();
        eq.values[Setting::LowMidGain as usize] = -12;
        assert!((eq.response_db(500.0) + 12.0).abs() < 0.01);
        eq.values[Setting::Level as usize] = 3;
        assert!((eq.response_db(500.0) + 9.0).abs() < 0.01);

        let mut eq = flat();
        eq.values[Setting::LowGain as usize] = 10;
        eq.values[Setting::HighGain as usize] = -10;
        assert!((eq.response_db(15.0) - 10.0).abs() < 0.3);
        assert!((eq.response_db(20000.0) + 10.0).abs() < 1.0);
    }

    #[test]
    fn cuts_are_3_db_down_at_their_frequency() {
        let mut eq = flat();
        eq.values[Setting::LowCut as usize] = 8; // 100 Hz
        eq.values[Setting::HighCut as usize] = 17; // 1 kHz
        assert!((eq.response_db(100.0) + 3.0).abs() < 0.1);
        assert!((eq.response_db(1000.0) + 3.0).abs() < 0.1);
        assert!(eq.response_db(25.0) < -20.0);
    }

    #[test]
    fn regions_cover_the_plot() {
        assert_eq!(region(F_MIN).2, "Rumble");
        assert_eq!(region(100.0).2, "Low end");
        assert_eq!(region(3000.0).2, "Edge / bite");
        assert_eq!(region(F_MAX).2, "Air / fizz");
        assert!(REGIONS.windows(2).all(|w| w[0].1 == w[1].0));
    }

    #[test]
    fn dragging_snaps_to_steps() {
        assert_eq!(nearest(1100.0, steps()), 17);
        assert_eq!(nearest(18.0, std::iter::once((0, LOW_FLAT_HZ)).chain(steps().map(|(i, f)| (i + 1, f)))), 1);
        assert_eq!(nearest(16.5, std::iter::once((0, LOW_FLAT_HZ)).chain(steps().map(|(i, f)| (i + 1, f)))), 0);
    }
}
