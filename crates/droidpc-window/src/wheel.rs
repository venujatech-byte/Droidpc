use eframe::egui::{MouseWheelUnit, Vec2};
use std::time::{Duration, Instant};

const INTERVAL: Duration = Duration::from_millis(200);

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
        size: [usize; 2],
    ) {
        let points = match unit {
            MouseWheelUnit::Point => delta,
            MouseWheelUnit::Line => delta * 32.0,
            MouseWheelUnit::Page => delta * viewport * 0.5,
        };
        self.pending += points
            * Vec2::new(
                size[0] as f32 / viewport.x.max(1.0),
                size[1] as f32 / viewport.y.max(1.0),
            );
        self.point = point;
    }

    pub fn take(&mut self, now: Instant, size: [usize; 2]) -> Option<String> {
        if self
            .sent_at
            .is_some_and(|at| now.duration_since(at) < INTERVAL)
            || self.pending.length() < 1.0
        {
            return None;
        }
        let delta = self.pending;
        self.pending = Vec2::ZERO;
        let [x, y] = self.point;
        let (end_x, end_y) = if delta.y.abs() >= delta.x.abs() {
            let limit = (size[1] as f32 * 0.10).clamp(1.0, 120.0);
            (
                x,
                (y as f32 + delta.y.clamp(-limit, limit))
                    .clamp(0.0, size[1].saturating_sub(1) as f32)
                    as u32,
            )
        } else {
            let limit = (size[0] as f32 * 0.10).clamp(1.0, 120.0);
            (
                (x as f32 + delta.x.clamp(-limit, limit))
                    .clamp(0.0, size[0].saturating_sub(1) as f32)
                    as u32,
                y,
            )
        };
        if (end_x, end_y) == (x, y) {
            return None;
        }
        self.sent_at = Some(now);
        Some(format!("swipe {x} {y} {end_x} {end_y} 100"))
    }

    pub fn wait(&self, now: Instant) -> Option<Duration> {
        (self.pending.length() >= 1.0).then(|| {
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
    fn scroll_uses_units_and_scales_to_android_pixels() {
        let mut wheel = Wheel::default();
        wheel.add(
            MouseWheelUnit::Line,
            Vec2::new(0.0, 1.0),
            [500, 500],
            Vec2::new(500.0, 500.0),
            [1000, 1000],
        );
        assert_eq!(
            wheel.take(Instant::now(), [1000, 1000]).as_deref(),
            Some("swipe 500 500 500 564 100")
        );
        let mut wheel = Wheel::default();
        wheel.add(
            MouseWheelUnit::Point,
            Vec2::new(0.0, 1.0),
            [500, 500],
            Vec2::new(500.0, 500.0),
            [1000, 1000],
        );
        assert_eq!(
            wheel.take(Instant::now(), [1000, 1000]).as_deref(),
            Some("swipe 500 500 500 502 100")
        );
    }

    #[test]
    fn fast_wheel_bursts_are_bounded_and_do_not_build_a_gesture_backlog() {
        let mut wheel = Wheel::default();
        let start = Instant::now();
        for _ in 0..100 {
            wheel.add(
                MouseWheelUnit::Line,
                Vec2::new(0.0, -3.0),
                [500, 500],
                Vec2::splat(1000.0),
                [1000, 1000],
            );
        }
        assert_eq!(
            wheel.take(start, [1000, 1000]).as_deref(),
            Some("swipe 500 500 500 400 100")
        );
        assert!(wheel.wait(start).is_none());
        wheel.add(
            MouseWheelUnit::Line,
            Vec2::new(1.0, 0.0),
            [500, 500],
            Vec2::splat(1000.0),
            [1000, 1000],
        );
        assert!(
            wheel
                .take(start + Duration::from_millis(10), [1000, 1000])
                .is_none()
        );
        assert_eq!(
            wheel.take(start + INTERVAL, [1000, 1000]).as_deref(),
            Some("swipe 500 500 532 500 100")
        );
    }
}
