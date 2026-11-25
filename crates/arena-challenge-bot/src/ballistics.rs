use super::{Observation, length};
use arena_core::{Action, PickupKind, ProjectileKind, Vec2};
use std::f64::consts::PI;

#[derive(Clone, Copy)]
struct Segment {
    origin: Vec2,
    velocity: Vec2,
    start: f64,
    end: f64,
    contacts: u8,
}

fn trace(
    world: &Observation,
    origin: Vec2,
    velocity: Vec2,
    kind: ProjectileKind,
    contacts: u8,
    horizon: f64,
    mut visit: impl FnMut(Segment) -> bool,
) {
    let limit = if kind == ProjectileKind::Laser {
        5
    } else {
        world.params.wall_contacts
    };
    let (mut position, mut velocity, mut now) = (origin, velocity, 0.0);
    for count in contacts..limit {
        let remaining = horizon - now;
        if remaining < 1e-7 {
            break;
        }
        let bounce = world
            .map
            .first_bounce(position, velocity.scale(remaining), kind.radius());
        let duration = remaining * bounce.map_or(1.0, |b| b.0);
        if !visit(Segment {
            origin: position,
            velocity,
            start: now,
            end: now + duration,
            contacts: count,
        }) {
            break;
        }
        let Some((_, reflected)) = bounce else {
            break;
        };
        position = position + velocity.scale(duration);
        velocity = reflected.scale(1.0 / remaining);
        now += duration;
    }
}

// Return time and distance at the closest point of a relative-motion segment.
fn closest(offset: Vec2, velocity: Vec2, span: f64) -> (f64, f64) {
    let t = (-offset.dot(velocity) / velocity.dot(velocity).max(1e-12)).clamp(0., span);
    (t, length(offset + velocity.scale(t)))
}

pub(super) fn firing_solution(world: &Observation, targets: &[Vec2]) -> Option<(f64, bool)> {
    let own = world.own_tank;
    if targets.is_empty() || (own.next_shot.is_none() && own.ammo == 0) {
        return None;
    }
    let kind = if own.next_shot == Some(PickupKind::Laser) {
        ProjectileKind::Laser
    } else {
        ProjectileKind::Shell
    };
    let range = if kind == ProjectileKind::Laser {
        9.0
    } else {
        4.5
    };
    if targets
        .iter()
        .all(|p| length(*p - own.pose.position) > range)
    {
        return None;
    }
    let mut best = None;
    let mut best_cost = f64::INFINITY;
    let steps = (PI / world.step()).ceil() as i32;
    for turn in -steps..=steps {
        let turn_cost = turn.unsigned_abs() as f64 * 0.065;
        if turn_cost >= best_cost {
            continue;
        }
        let angle = own.pose.angle + f64::from(turn) * world.step();
        trace(
            world,
            own.pose.position,
            Vec2::from_angle(angle),
            kind,
            0,
            range,
            |s| {
                let span = s.end - s.start;
                let self_contact = closest(s.origin - own.pose.position, s.velocity, span);
                let unsafe_return = s.contacts > 0 && self_contact.1 < 0.29;
                for target in targets {
                    let (along, miss) = closest(s.origin - *target, s.velocity, span);
                    if miss > 0.19 || (unsafe_return && self_contact.0 < along) {
                        continue;
                    }
                    let cost =
                        s.start + along + turn_cost + miss * 3. + f64::from(s.contacts) * 0.35;
                    if cost < best_cost {
                        best = Some((angle, turn.abs() <= 1));
                        best_cost = cost;
                    }
                }
                !unsafe_return && s.contacts < 3 && s.end + turn_cost < best_cost
            },
        );
    }
    best
}

pub(super) fn evade(world: &Observation, planned: Action) -> Action {
    let origin = world.own_tank.pose.position;
    let dt = 1. / f64::from(world.ticks_per_second);
    let delay = world.tick.saturating_sub(world.world_tick) as f64 * dt;
    let horizon = 0.72;
    let mut threats = Vec::new();
    for shot in &world.projectiles {
        let reach = length(shot.velocity) * (delay + horizon)
            + horizon / world.params.forward_secs.min(world.params.reverse_secs)
            + 0.32;
        if length(shot.position - origin) > reach {
            continue;
        }
        trace(
            world,
            shot.position,
            shot.velocity,
            shot.kind,
            shot.wall_contacts,
            delay + horizon,
            |mut s| {
                if s.end >= delay && (shot.owner != world.seat || s.contacts > 0) {
                    let skip = (delay - s.start).max(0.);
                    s.origin = s.origin + s.velocity.scale(skip);
                    s.start = (s.start - delay).max(0.);
                    s.end -= delay;
                    threats.push(s);
                }
                true
            },
        );
    }
    if threats.is_empty() {
        return planned;
    }
    let risk = |action: Action| {
        let turn = i8::from(action.turn_right) - i8::from(action.turn_left);
        let angle = world.own_tank.pose.angle + f64::from(turn) * world.step();
        let speed = if action.forward {
            1. / world.params.forward_secs
        } else if action.reverse {
            -1. / world.params.reverse_secs
        } else {
            0.
        };
        let velocity = Vec2::from_angle(angle).scale(speed);
        let stop = if speed == 0. {
            horizon
        } else {
            horizon
                * world
                    .map
                    .first_wall_contact(origin, origin + velocity.scale(horizon), 0.24)
                    .unwrap_or(1.)
        };
        let mut worst: f64 = 0.;
        for s in &threats {
            for (a, b, moving) in [
                (s.start, s.end.min(stop), true),
                (s.start.max(stop), s.end, false),
            ] {
                if b <= a {
                    continue;
                }
                let offset =
                    s.origin + s.velocity.scale(a - s.start) - origin - velocity.scale(a.min(stop));
                let relative = if moving {
                    s.velocity - velocity
                } else {
                    s.velocity
                };
                let (t, gap) = closest(offset, relative, b - a);
                worst = worst.max((0.30 - gap).max(0.) / (0.25 + a + t));
            }
        }
        worst
    };
    let mut best = planned;
    let mut lowest = risk(planned);
    if lowest <= 0. {
        return best;
    }
    for turn in [-1, 0, 1] {
        for movement in [-1, 0, 1] {
            let a = Action {
                turn_left: turn < 0,
                turn_right: turn > 0,
                forward: movement > 0,
                reverse: movement < 0,
                fire: false,
            };
            let score = risk(a);
            if score + 0.005 < lowest {
                best = a;
                lowest = score;
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_core::{Maze, Params, Pose, TankView, Wall};

    #[test]
    fn fires_a_bank_shot_at_a_target_hidden_by_a_wall() {
        let target = Vec2::new(3., 1.5);
        let mut world = Observation {
            map: Maze {
                width: 4,
                height: 3,
                walls: vec![
                    Wall {
                        a: Vec2::new(2., 1.),
                        b: Vec2::new(2., 2.),
                    },
                    Wall {
                        a: Vec2::new(0., 0.),
                        b: Vec2::new(4., 0.),
                    },
                ],
            },
            tick: 0,
            world_tick: 0,
            ticks_per_second: 60,
            seat: 0,
            own_tank: TankView {
                pose: Pose {
                    position: Vec2::new(1., 1.5),
                    angle: 0.,
                },
                ammo: 5,
                alive: true,
                refill_at: None,
                laser_loaded: false,
                next_shot: None,
            },
            other_tanks: vec![],
            projectiles: vec![],
            params: Params::default(),
        };
        let (angle, ready) = firing_solution(&world, &[target]).expect("bank shot");
        assert!(!ready && angle < 0.);
        world.own_tank.pose.angle = angle;
        assert!(firing_solution(&world, &[target]).unwrap().1);
        world.map.walls.pop();
        assert!(firing_solution(&world, &[target]).is_none());
    }
}
