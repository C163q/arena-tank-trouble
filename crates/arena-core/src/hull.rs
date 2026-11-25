//! Collision geometry for the oriented tank body.

use std::f64::consts::{PI, TAU};

use crate::collision::{Bounds, EPSILON};
use crate::{Pose, TANK_LENGTH, TANK_WIDTH, Vec2};

#[derive(Clone, Copy)]
pub(super) struct OrientedBox {
    center: Vec2,
    forward: Vec2,
    side: Vec2,
    half: Vec2,
}

impl OrientedBox {
    pub fn tank(pose: Pose) -> Self {
        let forward = Vec2::from_angle(pose.angle);
        Self {
            center: pose.position,
            forward,
            side: perpendicular(forward),
            half: Vec2::new(TANK_LENGTH / 2.0, TANK_WIDTH / 2.0),
        }
    }

    pub fn axis_aligned(bounds: Bounds) -> Self {
        Self {
            center: (bounds.max + bounds.min).scale(0.5),
            forward: Vec2::new(1.0, 0.0),
            side: Vec2::new(0.0, 1.0),
            half: (bounds.max - bounds.min).scale(0.5),
        }
    }

    fn world_offset(self, offset: Vec2) -> Vec2 {
        self.forward.scale(offset.x) + self.side.scale(offset.y)
    }

    fn rotated(self, angle: f64) -> Self {
        let forward = self.world_offset(Vec2::from_angle(angle));
        Self {
            forward,
            side: perpendicular(forward),
            ..self
        }
    }

    fn local_vector(self, vector: Vec2) -> Vec2 {
        Vec2::new(vector.dot(self.forward), vector.dot(self.side))
    }

    fn local_point(self, point: Vec2) -> Vec2 {
        self.local_vector(point - self.center)
    }

    fn extent(self, axis: Vec2) -> f64 {
        self.half.x * self.forward.dot(axis).abs() + self.half.y * self.side.dot(axis).abs()
    }

    pub fn within(self, bounds: Bounds) -> bool {
        let half = Vec2::new(
            self.extent(Vec2::new(1.0, 0.0)),
            self.extent(Vec2::new(0.0, 1.0)),
        );
        let min = self.center - half;
        let max = self.center + half;
        bounds.min.x - min.x <= EPSILON
            && bounds.min.y - min.y <= EPSILON
            && max.x - bounds.max.x <= EPSILON
            && max.y - bounds.max.y <= EPSILON
    }

    fn axes(self, other: Self) -> [Vec2; 4] {
        [self.forward, self.side, other.forward, other.side]
    }

    pub fn overlaps(self, other: Self) -> bool {
        let offset = other.center - self.center;
        self.axes(other)
            .into_iter()
            .all(|axis| offset.dot(axis).abs() < self.extent(axis) + other.extent(axis) - EPSILON)
    }

    /// Find a circular projectile's first contact with the hull.
    pub fn disc_contact(self, start: Vec2, movement: Vec2, radius: f64) -> Option<f64> {
        let start = self.local_point(start);
        let delta = self.local_vector(movement);
        let dx = (start.x.abs() - self.half.x).max(0.0);
        let dy = (start.y.abs() - self.half.y).max(0.0);
        if dx * dx + dy * dy <= radius * radius {
            return Some(0.0);
        }
        Bounds::centered(self.half)
            .sweep_disc(start, delta, radius, true)
            .map(|hit| hit.time)
    }

    pub fn turn_fraction(self, turn: f64, obstacles: &[Self]) -> f64 {
        if obstacles.is_empty() {
            return 1.0;
        }
        let mut times = vec![0.0, 1.0];
        let moving_corners = Bounds::centered(self.half).corners();

        // Overlap can change only where a corner crosses an obstacle edge.
        // Split the turn at those angles to catch even very short contacts.
        for &obstacle in obstacles {
            let relative = obstacle.local_point(self.center);
            for corner in moving_corners {
                let moving = obstacle.local_vector(self.world_offset(corner));
                box_face_events(&mut times, turn, relative, moving, obstacle.half);
            }
            for corner in Bounds::centered(obstacle.half).corners() {
                let fixed = self.local_point(obstacle.center + obstacle.world_offset(corner));
                box_face_events(&mut times, -turn, Vec2::new(0.0, 0.0), fixed, self.half);
            }
        }

        times.sort_by(f64::total_cmp);
        for window in times.windows(2) {
            let [start, end] = [window[0], window[1]];
            if end - start <= EPSILON {
                continue;
            }
            let middle = (start + end) / 2.0;
            let hull = self.rotated(turn * middle);
            if obstacles.iter().any(|&obstacle| hull.overlaps(obstacle)) {
                return (start - EPSILON).max(0.0);
            }
        }
        1.0
    }

    pub fn movement_contact(self, movement: Vec2, other: Self) -> Option<f64> {
        // Intersect the time intervals with an overlapping projection on
        // each separating axis.
        let offset = self.center - other.center;
        let mut enter: f64 = 0.0;
        let mut exit: f64 = 1.0;
        for axis in self.axes(other) {
            let extent = self.extent(axis) + other.extent(axis) - EPSILON;
            let gap = offset.dot(axis);
            let speed = movement.dot(axis);
            if speed.abs() <= EPSILON {
                if gap.abs() >= extent {
                    return None;
                }
            } else {
                let a = (-extent - gap) / speed;
                let b = (extent - gap) / speed;
                enter = enter.max(a.min(b));
                exit = exit.min(a.max(b));
                if enter + EPSILON >= exit {
                    return None;
                }
            }
        }
        Some(enter)
    }
}

fn box_face_events(times: &mut Vec<f64>, turn: f64, center: Vec2, offset: Vec2, half: Vec2) {
    for sign in [-1.0, 1.0] {
        projection_events(times, turn, offset.x, -offset.y, sign * half.x - center.x);
        projection_events(times, turn, offset.y, offset.x, sign * half.y - center.y);
    }
}

fn projection_events(times: &mut Vec<f64>, turn: f64, a: f64, b: f64, threshold: f64) {
    // Solve a*cos(angle) + b*sin(angle) = threshold.
    let amplitude = a.hypot(b);
    if amplitude == 0.0 || threshold.abs() > amplitude {
        return;
    }
    let phase = b.atan2(a);
    let relative = (threshold / amplitude).acos();
    for solution in [phase - relative, phase + relative] {
        let angle = (solution + PI).rem_euclid(TAU) - PI;
        let time = angle / turn;
        if 0.0 < time && time < 1.0 {
            times.push(time);
        }
    }
}

fn perpendicular(vector: Vec2) -> Vec2 {
    Vec2::new(-vector.y, vector.x)
}
