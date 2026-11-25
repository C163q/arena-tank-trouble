use crate::{Maze, Vec2, WALL_THICKNESS, Wall};

/// Tolerance for contacts sharing the same point along one sweep.
pub(super) const EPSILON: f64 = 1e-9;

#[derive(Clone, Copy)]
pub(super) struct Contact {
    pub time: f64,
    pub normal: Vec2,
    pub corner: bool,
}

impl Contact {
    pub fn reflect(self, incoming: Vec2) -> Vec2 {
        if self.corner {
            incoming.scale(-1.0)
        } else {
            incoming - self.normal.scale(2.0 * incoming.dot(self.normal))
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Bounds {
    pub min: Vec2,
    pub max: Vec2,
}

impl Bounds {
    pub fn wall(wall: Wall) -> Self {
        let half = WALL_THICKNESS / 2.0;
        Self {
            min: Vec2::new(wall.a.x.min(wall.b.x) - half, wall.a.y.min(wall.b.y) - half),
            max: Vec2::new(wall.a.x.max(wall.b.x) + half, wall.a.y.max(wall.b.y) + half),
        }
    }

    pub fn centered(half: Vec2) -> Self {
        Self {
            min: half.scale(-1.0),
            max: half,
        }
    }

    pub fn corners(self) -> [Vec2; 4] {
        [
            self.min,
            Vec2::new(self.max.x, self.min.y),
            self.max,
            Vec2::new(self.min.x, self.max.y),
        ]
    }

    pub fn near(self, point: Vec2, reach: f64) -> bool {
        (self.min.x - reach..=self.max.x + reach).contains(&point.x)
            && (self.min.y - reach..=self.max.y + reach).contains(&point.y)
    }

    pub fn sweep_disc(
        self,
        start: Vec2,
        delta: Vec2,
        radius: f64,
        count_tangencies: bool,
    ) -> Option<Contact> {
        let end = start + delta;
        if start.x.min(end.x) > self.max.x + radius
            || start.x.max(end.x) < self.min.x - radius
            || start.y.min(end.y) > self.max.y + radius
            || start.y.max(end.y) < self.min.y - radius
        {
            return None;
        }

        // A moving disc can first meet a rectangle on a face or at a vertex.
        // A vertex contact uses the normal from the vertex to the disc center.
        let corners = self.corners();
        corners
            .into_iter()
            .zip(corners.into_iter().cycle().skip(1))
            .filter_map(|(a, b)| sweep_segment(a, b, start, delta, radius, count_tangencies))
            .min_by(|a, b| a.time.total_cmp(&b.time))
    }
}

/// Return the first contact made by a point moving along delta.
fn sweep_circle(
    start: Vec2,
    delta: Vec2,
    center: Vec2,
    radius: f64,
    count_tangencies: bool,
) -> Option<Contact> {
    let offset = start - center;
    let along = offset.dot(delta);
    let length_squared = delta.dot(delta);
    if along >= -EPSILON || length_squared == 0.0 {
        return None;
    }

    let gap = offset.dot(offset) - radius * radius;
    let discriminant = along * along - length_squared * gap;
    // A wall vertex grazed along the tangent is not a bounce. Scale the
    // tolerance to the quadratic to absorb cancellation at exact tangency.
    let minimum = if count_tangencies {
        0.0
    } else {
        EPSILON * length_squared * radius * radius
    };
    if discriminant < minimum {
        return None;
    }
    let time = ((-along - discriminant.sqrt()) / length_squared).max(0.0);
    if time > 1.0 + EPSILON {
        return None;
    }
    let normal = (offset + delta.scale(time)).scale(1.0 / radius);
    Some(Contact {
        time: time.min(1.0),
        normal,
        corner: false,
    })
}

pub(super) fn sweep_maze(maze: &Maze, start: Vec2, delta: Vec2, radius: f64) -> Option<Contact> {
    let mut nearest: Option<Contact> = None;
    for &wall in &maze.walls {
        let Some(hit) = Bounds::wall(wall).sweep_disc(start, delta, radius, false) else {
            continue;
        };
        match nearest.as_mut() {
            Some(current) if (current.time - hit.time).abs() <= EPSILON => {
                current.corner |= current.normal.dot(hit.normal).abs() < 1.0 - EPSILON;
            }
            Some(current) if current.time < hit.time => {}
            _ => nearest = Some(hit),
        }
    }
    nearest
}

fn sweep_segment(
    a: Vec2,
    b: Vec2,
    start: Vec2,
    delta: Vec2,
    radius: f64,
    count_tangencies: bool,
) -> Option<Contact> {
    let vertical = a.x == b.x;
    let coordinates = |v: Vec2| if vertical { (v.x, v.y) } else { (v.y, v.x) };
    let (axis, end_a) = coordinates(a);
    let (_, end_b) = coordinates(b);
    let (across, along) = coordinates(start);
    let (across_delta, along_delta) = coordinates(delta);
    let side = if across_delta.abs() > EPSILON {
        let sign = -across_delta.signum();
        let time = (axis + sign * radius - across) / across_delta;
        let contact_at = along + time * along_delta;
        let normal = if vertical {
            Vec2::new(sign, 0.0)
        } else {
            Vec2::new(0.0, sign)
        };
        ((-EPSILON..=1.0 + EPSILON).contains(&time)
            && (end_a.min(end_b)..=end_a.max(end_b)).contains(&contact_at))
        .then(|| Contact {
            time: time.clamp(0.0, 1.0),
            normal,
            corner: false,
        })
    } else {
        None
    };

    [
        side,
        sweep_circle(start, delta, a, radius, count_tangencies),
        sweep_circle(start, delta, b, radius, count_tangencies),
    ]
    .into_iter()
    .flatten()
    .min_by(|a, b| a.time.total_cmp(&b.time))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn short_wall() -> Maze {
        Maze {
            width: 3,
            height: 3,
            walls: vec![Wall {
                a: Vec2::new(1.0, 1.0),
                b: Vec2::new(2.0, 1.0),
            }],
        }
    }

    #[test]
    fn projectile_meets_a_flat_wall_end_head_on() {
        let start = Vec2::new(2.14, 1.02);
        let movement = Vec2::new(-0.15, 0.0);
        let hit = sweep_maze(&short_wall(), start, movement, 0.042).unwrap();
        assert_eq!(hit.normal, Vec2::new(1.0, 0.0));
        let position = start + movement.scale(hit.time);
        assert!((position.x - 2.082).abs() < 1e-9);
    }

    #[test]
    fn tangential_projectile_does_not_hit_a_straight_wall_seam() {
        let mut maze = short_wall();
        maze.walls.push(Wall {
            a: Vec2::new(2.0, 1.0),
            b: Vec2::new(3.0, 1.0),
        });
        let start = Vec2::new(1.83, 1.082);
        assert!(sweep_maze(&maze, start, Vec2::new(0.15, 0.0), 0.042).is_none());

        let approaching = Vec2::new(1.83, 1.082000001);
        let hit = sweep_maze(&maze, approaching, Vec2::new(0.15, -2e-9), 0.042).unwrap();
        assert_eq!(hit.normal, Vec2::new(0.0, 1.0));
    }

    #[test]
    fn projectile_can_glance_off_a_wall_corner() {
        let start = Vec2::new(2.075, 1.12);
        let movement = Vec2::new(0.0, -0.08);
        let hit = sweep_maze(&short_wall(), start, movement, 0.042).unwrap();
        let position = start + movement.scale(hit.time);
        assert!((1.06..1.07).contains(&position.y));
        assert!(hit.normal.x > 0.0 && hit.normal.y > 0.0);
    }
}
