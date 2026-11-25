use super::{Observation, difference, length};
use arena_core::{Action, Vec2};
use std::collections::VecDeque;
use std::f64::consts::{FRAC_PI_2, PI};

pub(super) fn hold_center(world: &Observation, targets: &[Vec2]) -> Action {
    let map = &world.map;
    let width = usize::from(map.width);
    let height = usize::from(map.height);
    let count = width * height;
    let pose = world.own_tank.pose;
    let cell = |p: Vec2| (p.y as usize).min(height - 1) * width + (p.x as usize).min(width - 1);
    let middle = |c: usize| Vec2::new((c % width) as f64 + 0.5, (c / width) as f64 + 0.5);
    let start = cell(pose.position);
    let mut first = vec![usize::MAX; count];
    first[start] = start;
    let mut queue = VecDeque::from([start]);
    let center = Vec2::new(width as f64 * 0.5, height as f64 * 0.5);
    let mut best = start;
    let mut best_distance = f64::INFINITY;
    while let Some(c) = queue.pop_front() {
        // BFS tie-breaks equally central cells by travel distance.
        let distance = length(middle(c) - center);
        if distance < best_distance {
            best_distance = distance;
            best = c;
        }
        for to in [
            (c % width > 0).then(|| c - 1),
            (c % width + 1 < width).then_some(c + 1),
            c.checked_sub(width),
            (c + width < count).then_some(c + width),
        ]
        .into_iter()
        .flatten()
        {
            if first[to] == usize::MAX
                && map.first_wall_contact(middle(c), middle(to), 0.).is_none()
            {
                first[to] = if c == start { to } else { first[c] };
                queue.push_back(to);
            }
        }
    }
    let endgame = world.tick > u64::from(world.ticks_per_second) * 65;
    let mut waypoint = middle(first[best]);
    if best == start {
        // Leave room to rotate even if the center is a wall intersection.
        let towards = center - waypoint;
        waypoint = waypoint + Vec2::new(towards.x.clamp(-0.18, 0.18), towards.y.clamp(-0.18, 0.18));
        if endgame {
            let anchor = middle(start);
            let mut score = f64::INFINITY;
            // A central wall may block one axis but leave the other open.
            // Minimize the fraction of the shrinking rectangle needed to fit.
            for x in [0.0_f64, 0.18, 0.32, 0.48] {
                for y in [0.0_f64, 0.18, 0.32, 0.48] {
                    let candidate = anchor
                        + Vec2::new(
                            towards.x.signum() * x.min(towards.x.abs()),
                            towards.y.signum() * y.min(towards.y.abs()),
                        );
                    let gap = candidate - center;
                    let cost = ((gap.x.abs() + 0.21) / width as f64)
                        .max((gap.y.abs() + 0.17) / height as f64)
                        + 0.001 * length(gap);
                    if cost < score && map.first_wall_contact(anchor, candidate, 0.27).is_none() {
                        waypoint = candidate;
                        score = cost;
                    }
                }
            }
        }
    } else if map
        .first_wall_contact(pose.position, waypoint, 0.28)
        .is_some()
    {
        waypoint = middle(start);
    }
    let offset = waypoint - pose.position;
    if length(offset) < 0.045 {
        if endgame && best == start {
            return world.steer(if pose.angle.cos() < 0.0 { PI } else { 0.0 });
        }
        return targets
            .iter()
            .min_by(|a, b| length(**a - pose.position).total_cmp(&length(**b - pose.position)))
            .map_or_else(Action::default, |p| {
                let next = first[cell(*p)];
                let aim = if next == start || next == usize::MAX {
                    *p
                } else {
                    middle(next)
                };
                let delta = aim - pose.position;
                world.steer(delta.y.atan2(delta.x))
            });
    }
    let mut angle = offset.y.atan2(offset.x);
    let reverse = difference(angle, pose.angle).abs() > FRAC_PI_2;
    if reverse {
        angle += PI;
    }
    let mut action = world.steer(angle);
    if difference(angle, pose.angle).abs() < 0.16 {
        action.forward = !reverse;
        action.reverse = reverse;
    }
    action
}
