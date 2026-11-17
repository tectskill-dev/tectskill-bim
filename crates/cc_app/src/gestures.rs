//! Scroll and pinch gestures over the views (issue #174).
//!
//! egui adds the mouse wheel and a trackpad's two-finger scroll into one
//! scroll delta. The views tell them apart by the wheel events of the frame:
//! a wheel reports lines (or pages), a trackpad reports points. A wheel
//! zooms, as in CAD programs; with the *Trackpad scrolling pans* preference
//! (on by default) two fingers move the view and a pinch zooms. The kind of
//! the last wheel event is remembered, since egui spreads a wheel notch over
//! a few frames that have no events of their own.

use egui::{Context, Event, Id, MouseWheelUnit, Ui, Vec2};
use std::sync::atomic::{AtomicBool, Ordering};

/// The preference, set once per frame by `preferences::sync_look`.
static TRACKPAD_PANS: AtomicBool = AtomicBool::new(true);

/// Follow the *Trackpad scrolling pans* preference.
pub fn set_trackpad_pans(on: bool) {
    TRACKPAD_PANS.store(on, Ordering::Relaxed);
}

/// What the scroll and pinch of this frame ask a view to do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gesture {
    /// Scroll that zooms, in points (a wheel; positive: in).
    pub zoom_scroll: f32,
    /// Pinch (or Ctrl+scroll) zoom factor (1: none).
    pub pinch: f32,
    /// How far the content moves on screen, in points (two fingers on a trackpad).
    pub pan: Vec2,
}

impl Gesture {
    /// The zoom factor, with `rate` per point of wheel scroll.
    pub fn zoom(&self, rate: f64) -> f64 {
        let factor = (f64::from(self.zoom_scroll) * rate).exp() * f64::from(self.pinch);
        if factor.is_finite() && factor > 0.0 { factor } else { 1.0 }
    }
}

fn source_id() -> Id {
    Id::new("gesture-scroll-from-trackpad")
}

/// Whether the last scroll came from a trackpad, updated with this frame's events.
fn from_trackpad(ctx: &Context) -> bool {
    let latest = ctx.input(|i| {
        i.events.iter().rev().find_map(|e| match e {
            Event::MouseWheel { unit, .. } => Some(*unit == MouseWheelUnit::Point),
            _ => None,
        })
    });
    match latest {
        Some(trackpad) => {
            ctx.data_mut(|d| d.insert_temp(source_id(), trackpad));
            trackpad
        }
        None => ctx.data(|d| d.get_temp(source_id())).unwrap_or(false),
    }
}

/// The gesture of this frame (call it only while the view is hovered).
pub fn read(ui: &Ui) -> Gesture {
    let trackpad = from_trackpad(ui.ctx());
    let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
    split(scroll, pinch, trackpad && TRACKPAD_PANS.load(Ordering::Relaxed))
}

/// A scroll delta and a pinch as a gesture: `pans` moves the view with the scroll.
fn split(scroll: Vec2, pinch: f32, pans: bool) -> Gesture {
    if pans {
        Gesture { zoom_scroll: 0.0, pinch, pan: scroll }
    } else {
        Gesture { zoom_scroll: scroll.y, pinch, pan: Vec2::ZERO }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wheel_zooms_and_a_trackpad_pans() {
        let wheel = split(egui::vec2(0.0, 40.0), 1.0, false);
        assert_eq!((wheel.zoom_scroll, wheel.pan), (40.0, Vec2::ZERO));
        assert!(wheel.zoom(0.0025) > 1.0);
        let fingers = split(egui::vec2(-12.0, 5.0), 1.0, true);
        assert_eq!((fingers.zoom_scroll, fingers.pan), (0.0, egui::vec2(-12.0, 5.0)));
        assert_eq!(fingers.zoom(0.0025), 1.0);
        // A pinch zooms either way.
        assert!((split(Vec2::ZERO, 1.25, true).zoom(0.0025) - 1.25).abs() < 1e-6);
        assert_eq!(split(Vec2::ZERO, f32::NAN, false).zoom(0.0025), 1.0);
    }

    /// The events of a frame decide the source, and frames without events keep it.
    #[test]
    fn the_source_is_the_last_wheel_event() {
        let ctx = egui::Context::default();
        let frame = |events: Vec<Event>| {
            let input = egui::RawInput { events, ..Default::default() };
            let mut seen = false;
            let mut out = ctx.run_ui(input, |ui| seen = from_trackpad(ui.ctx()));
            out.textures_delta.clear();
            seen
        };
        let wheel = |unit| Event::MouseWheel {
            unit,
            delta: egui::vec2(0.0, 1.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        };
        assert!(!frame(vec![]), "a wheel until told otherwise");
        assert!(frame(vec![wheel(MouseWheelUnit::Point)]));
        assert!(frame(vec![]), "kept while the scroll goes on");
        assert!(!frame(vec![wheel(MouseWheelUnit::Line)]));
    }
}
