use eframe::egui::{self, Vec2};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub struct Display {
    path: Option<PathBuf>,
    last: Option<[u32; 5]>,
    changed: Instant,
    reported: bool,
}

fn dimensions(points: Vec2, scale: f32) -> Option<[u32; 5]> {
    if !scale.is_finite()
        || !(0.5..=4.0).contains(&scale)
        || !points.is_finite()
    {
        return None;
    }
    let width = points.x.round() as u32;
    let height = points.y.round() as u32;
    let pixel_width = (points.x * scale).round() as u32;
    let pixel_height = (points.y * scale).round() as u32;
    if width < 320
        || height < 200
        || pixel_width > 8192
        || pixel_height > 8192
        || u64::from(pixel_width) * u64::from(pixel_height) > 16_777_216
    {
        return None;
    }
    Some([
        width,
        height,
        pixel_width,
        pixel_height,
        (160.0 * scale).round() as u32,
    ])
}

impl Display {
    pub fn new() -> Self {
        Self {
            path: std::env::var_os("DROIDPC_DISPLAY_CONFIG").map(PathBuf::from),
            last: None,
            changed: Instant::now(),
            reported: false,
        }
    }

    pub fn report(&mut self, ctx: &egui::Context) {
        let Some(path) = &self.path else {
            return;
        };
        // shortcut: calibrate at startup; live monitor changes need guest display mode support.
        if self.reported {
            return;
        }
        let Some(size) =
            dimensions(ctx.screen_rect().size(), ctx.pixels_per_point())
        else {
            return;
        };
        if self.last != Some(size) {
            self.last = Some(size);
            self.changed = Instant::now();
        }
        if self.changed.elapsed() < Duration::from_millis(200) {
            ctx.request_repaint_after(Duration::from_millis(200));
            return;
        }
        let [w, h, pw, ph, density] = size;
        match std::fs::write(path, format!("{w} {h} {pw} {ph} {density}\n")) {
            Ok(()) => eprintln!(
                "Android display request: {pw}x{ph} pixels; {density} dpi"
            ),
            Err(error) => {
                eprintln!("Could not report Android display size: {error}")
            }
        }
        self.reported = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn display_size_uses_physical_pixels_and_matching_density() {
        assert_eq!(
            dimensions(Vec2::new(960.0, 540.0), 2.0),
            Some([960, 540, 1920, 1080, 320])
        );
        assert_eq!(
            dimensions(Vec2::new(960.0, 540.0), 1.25),
            Some([960, 540, 1200, 675, 200])
        );
        assert!(dimensions(Vec2::splat(8192.0), 4.0).is_none());
        assert!(dimensions(Vec2::splat(1000.0), f32::NAN).is_none());
    }
}
