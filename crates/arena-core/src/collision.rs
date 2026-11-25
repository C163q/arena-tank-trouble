use crate::{Maze, Vec2, WALL_THICKNESS, Wall};

/// Tolerance for contacts sharing the same point along one sweep.
pub(super) const EPSILON: f64 = 1e-9;

#[derive(Clone, Copy)]
pub(super) struct Contact {
    pub time: f64,
    pub normal: Vec2,
    pub corner: bool,
}

/// Return the first contact made by a point moving along delta.
pub(super) fn sweep_circle(start: Vec2, delta: Vec2, center: Vec2, radius: f64) -> Option<Contact> {
    let offset = start - center;
    let along = offset.dot(delta);
    let length_squared = delta.dot(delta);
    if along >= -EPSILON || length_squared == 0.0 {
        return None;
    }

    let gap = offset.dot(offset) - radius * radius;
    let discriminant = along * along - length_squared * gap;
    if discriminant < 0.0 {
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
        let Some(hit) = sweep_wall(wall, start, delta, radius + WALL_THICKNESS / 2.0) else {
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

fn sweep_wall(wall: Wall, start: Vec2, delta: Vec2, radius: f64) -> Option<Contact> {
    // A wall with round ends is the union of its side strip and endpoint discs.
    let vertical = wall.a.x == wall.b.x;
    let coordinates = |v: Vec2| if vertical { (v.x, v.y) } else { (v.y, v.x) };
    let (axis, end_a) = coordinates(wall.a);
    let (_, end_b) = coordinates(wall.b);
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
        sweep_circle(start, delta, wall.a, radius),
        sweep_circle(start, delta, wall.b, radius),
    ]
    .into_iter()
    .flatten()
    .min_by(|a, b| a.time.total_cmp(&b.time))
}
