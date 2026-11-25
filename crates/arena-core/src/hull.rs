//! Collision geometry for the oriented tank body.

use std::f64::consts::{PI, TAU};

use crate::collision::{EPSILON, sweep_wall};
use crate::{Pose, TANK_LENGTH, TANK_WIDTH, Vec2, WALL_THICKNESS, Wall};

const HALF_LENGTH: f64 = TANK_LENGTH / 2.0;
const HALF_WIDTH: f64 = TANK_WIDTH / 2.0;
// A shared tolerance lets both turning and driving accept the same tangency.
const WALL_RADIUS: f64 = WALL_THICKNESS / 2.0 - EPSILON;
const CORNERS: [Vec2; 4] = [
    Vec2::new(-HALF_LENGTH, -HALF_WIDTH),
    Vec2::new(HALF_LENGTH, -HALF_WIDTH),
    Vec2::new(HALF_LENGTH, HALF_WIDTH),
    Vec2::new(-HALF_LENGTH, HALF_WIDTH),
];

#[derive(Clone, Copy)]
pub(super) struct Hull {
    center: Vec2,
    forward: Vec2,
    side: Vec2,
}

impl Hull {
    pub fn new(pose: Pose) -> Self {
        let forward = Vec2::from_angle(pose.angle);
        let side = perpendicular(forward);
        Self {
            center: pose.position,
            forward,
            side,
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
        HALF_LENGTH * self.forward.dot(axis).abs() + HALF_WIDTH * self.side.dot(axis).abs()
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

    pub fn overlaps_wall(self, wall: Wall) -> bool {
        // Allow exact tangency. Testing the wall center line against the
        // radius-expanded rectangle accounts for both the sides and end caps.
        self.disc_contact(wall.a, wall.b - wall.a, WALL_RADIUS)
            .is_some()
    }

    /// Find a circular projectile's first contact with the hull.
    pub fn disc_contact(self, start: Vec2, movement: Vec2, radius: f64) -> Option<f64> {
        let start = self.local_point(start);
        let delta = self.local_vector(movement);
        let dx = (start.x.abs() - HALF_LENGTH).max(0.0);
        let dy = (start.y.abs() - HALF_WIDTH).max(0.0);
        if dx * dx + dy * dy <= radius * radius {
            return Some(0.0);
        }
        sweep_local_box(start, delta, radius, false)
    }

    pub fn wall_contact(self, movement: Vec2, wall: Wall) -> Option<f64> {
        // During translation a first contact is either a moving hull corner
        // against the wall capsule or a wall end against a moving hull edge.
        let corners = CORNERS.into_iter().filter_map(|corner| {
            let start = self.center + self.world_offset(corner);
            let contact = sweep_wall(wall, start, movement, WALL_RADIUS)?;
            (movement.dot(contact.normal) < -EPSILON).then_some(contact.time)
        });
        let delta = self.local_vector(movement.scale(-1.0));
        let ends = [wall.a, wall.b]
            .into_iter()
            .filter_map(|end| sweep_local_box(self.local_point(end), delta, WALL_RADIUS, true));
        corners.chain(ends).min_by(f64::total_cmp)
    }

    pub fn turn_fraction(self, turn: f64, walls: &[Wall], other: Self) -> f64 {
        let mut arcs = ArcContacts {
            turn,
            times: vec![0.0, 1.0],
        };
        let offset = other.center - self.center;
        let near_other = offset.dot(offset) <= TANK_LENGTH * TANK_LENGTH + TANK_WIDTH * TANK_WIDTH;
        if walls.is_empty() && !near_other {
            return 1.0;
        }

        // Overlap can change only at a corner/edge or corner/end-cap event.
        // Split there to catch even a contact that ends within this step.
        for &wall in walls {
            self.wall_arc_contacts(wall, &mut arcs);
        }
        if near_other {
            for corner in CORNERS {
                let relative = other.local_point(self.center);
                let moving = other.local_vector(self.world_offset(corner));
                arcs.box_faces(relative, moving, 0.0, false);
                let fixed = self.local_point(other.center + other.world_offset(corner));
                arcs.box_faces(Vec2::new(0.0, 0.0), fixed, 0.0, true);
            }
        }

        arcs.times.sort_by(f64::total_cmp);
        for window in arcs.times.windows(2) {
            let [start, end] = [window[0], window[1]];
            if end - start <= EPSILON {
                continue;
            }
            let center = (start + end) / 2.0;
            let hull = self.rotated(turn * center);
            if hull.overlaps(other) || walls.iter().any(|&wall| hull.overlaps_wall(wall)) {
                return (start - EPSILON).max(0.0);
            }
        }
        1.0
    }

    fn wall_arc_contacts(self, wall: Wall, arcs: &mut ArcContacts) {
        let vertical = wall.a.x == wall.b.x;
        let normal = if vertical {
            Vec2::new(1.0, 0.0)
        } else {
            Vec2::new(0.0, 1.0)
        };
        let face = (wall.a - self.center).dot(normal);
        for corner in CORNERS {
            let moving = self.world_offset(corner);
            for sign in [-1.0, 1.0] {
                arcs.projection(moving, normal, face + sign * WALL_RADIUS, false);
            }
            for end in [wall.a, wall.b] {
                arcs.circle(moving, end - self.center, WALL_RADIUS, false);
            }
        }
        // In the rotating hull's frame, each wall end rotates the other way.
        for end in [wall.a, wall.b] {
            let moving = self.local_point(end);
            arcs.box_faces(Vec2::new(0.0, 0.0), moving, WALL_RADIUS, true);
            for corner in CORNERS {
                arcs.circle(moving, corner, WALL_RADIUS, true);
            }
        }
    }

    pub fn hull_contact(self, movement: Vec2, other: Self) -> Option<f64> {
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

fn sweep_local_box(start: Vec2, delta: Vec2, radius: f64, approaching_only: bool) -> Option<f64> {
    CORNERS
        .into_iter()
        .zip(CORNERS.into_iter().cycle().skip(1))
        .filter_map(|(a, b)| sweep_wall(Wall { a, b }, start, delta, radius))
        .filter(|hit| !approaching_only || delta.dot(hit.normal) < -EPSILON)
        .map(|hit| hit.time)
        .min_by(f64::total_cmp)
}

struct ArcContacts {
    turn: f64,
    times: Vec<f64>,
}

impl ArcContacts {
    fn box_faces(&mut self, center: Vec2, offset: Vec2, radius: f64, inverse: bool) {
        for sign in [-1.0, 1.0] {
            self.projection(
                offset,
                Vec2::new(1.0, 0.0),
                sign * (HALF_LENGTH + radius) - center.x,
                inverse,
            );
            self.projection(
                offset,
                Vec2::new(0.0, 1.0),
                sign * (HALF_WIDTH + radius) - center.y,
                inverse,
            );
        }
    }

    fn circle(&mut self, offset: Vec2, center: Vec2, radius: f64, inverse: bool) {
        // Squared distance to the stationary cap is linear in the rotated offset.
        let threshold = (offset.dot(offset) + center.dot(center) - radius * radius) / 2.0;
        self.projection(offset, center, threshold, inverse);
    }

    fn projection(&mut self, offset: Vec2, axis: Vec2, threshold: f64, inverse: bool) {
        // Solve a*cos(angle) + b*sin(angle) = threshold.
        let a = offset.dot(axis);
        let b = perpendicular(offset).dot(axis);
        let amplitude = a.hypot(b);
        if amplitude == 0.0 || threshold.abs() > amplitude {
            return;
        }
        let phase = b.atan2(a);
        let relative = (threshold / amplitude).acos();
        let turn = if inverse { -self.turn } else { self.turn };
        for solution in [phase - relative, phase + relative] {
            let angle = (solution + PI).rem_euclid(TAU) - PI;
            let time = angle / turn;
            if 0.0 < time && time < 1.0 {
                self.times.push(time);
            }
        }
    }
}

fn perpendicular(vector: Vec2) -> Vec2 {
    Vec2::new(-vector.y, vector.x)
}
