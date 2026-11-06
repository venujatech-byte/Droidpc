use eframe::egui::{MouseWheelUnit, Vec2};
use std::time::{Duration, Instant};

const INTERVAL: Duration = Duration::from_millis(16);

#[derive(Default)]
pub struct Wheel {
    pending: Vec2,
    point: [u32; 2],
    sent_at: Option<Instant>,
}

impl Wheel {
    pub fn add(
        &mut self,
        unit: MouseWheelUnit,
        delta: Vec2,
        point: [u32; 2],
        viewport: Vec2,
    ) {
        // Android's default wheel factor is 64 dp; a mouse notch moves about three text lines.
        let axes = match unit {
            MouseWheelUnit::Point => delta / 64.0,
            MouseWheelUnit::Line => delta * 0.75,
            MouseWheelUnit::Page => delta * viewport / 64.0,
        };
        if axes.is_finite() {
            self.pending += axes;
            self.point = point;
        }
    }

    pub fn take(&mut self, now: Instant) -> Option<String> {
        if self
            .sent_at
            .is_some_and(|at| now.duration_since(at) < INTERVAL)
            || self.pending.length() < 0.001
        {
            return None;
        }
        let delta = self.pending.clamp(Vec2::splat(-4.0), Vec2::splat(4.0));
        self.pending = Vec2::ZERO;
        self.sent_at = Some(now);
        Some(format!(
            "scroll {} {} {:.3} {:.3}",
            self.point[0], self.point[1], delta.x, delta.y
        ))
    }

    pub fn wait(&self, now: Instant) -> Option<Duration> {
        (self.pending.length() >= 0.001).then(|| {
            self.sent_at.map_or(Duration::ZERO, |at| {
                INTERVAL.saturating_sub(now.duration_since(at))
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_notches_and_touchpad_pixels_have_equivalent_travel() {
        for (unit, delta) in
            [(MouseWheelUnit::Line, 1.0), (MouseWheelUnit::Point, 48.0)]
        {
            let mut wheel = Wheel::default();
            wheel.add(
                unit,
                Vec2::new(0.0, delta),
                [500, 500],
                Vec2::splat(1000.0),
            );
            assert_eq!(
                wheel.take(Instant::now()).as_deref(),
                Some("scroll 500 500 0.000 0.750")
            );
        }
    }

    #[test]
    fn touchpad_preserves_small_diagonal_deltas_and_coalesces_without_backlog()
    {
        let mut wheel = Wheel::default();
        let start = Instant::now();
        wheel.add(
            MouseWheelUnit::Point,
            Vec2::new(1.0, -2.0),
            [50, 60],
            Vec2::splat(1000.0),
        );
        assert_eq!(
            wheel.take(start).as_deref(),
            Some("scroll 50 60 0.016 -0.031")
        );
        for _ in 0..100 {
            wheel.add(
                MouseWheelUnit::Line,
                Vec2::new(0.0, -3.0),
                [50, 60],
                Vec2::splat(1000.0),
            );
        }
        assert!(wheel.take(start + Duration::from_millis(1)).is_none());
        assert_eq!(
            wheel.take(start + INTERVAL).as_deref(),
            Some("scroll 50 60 0.000 -4.000")
        );
        assert!(wheel.wait(start + INTERVAL).is_none());
    }
}
