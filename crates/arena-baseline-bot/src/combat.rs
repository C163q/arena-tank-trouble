use arena_core::{Action, PROJECTILE_RADIUS, PickupKind, ProjectileKind, TANK_WIDTH, Vec2};

use crate::{Observation, tank_radius};

const AIM_RANGE: f64 = 4.0;
const LASER_AIM_RANGE: f64 = 8.0;
const SHOT_INTERVAL_TICKS: u64 = 12;
const DODGE_SECONDS: f64 = 0.28;

pub(crate) fn aim(world: &Observation) -> Option<Action> {
    let own = world.own_tank.pose;
    if world.own_tank.ammo == 0 && world.own_tank.next_shot.is_none() {
        return None;
    }
    let kind = if world.own_tank.next_shot == Some(PickupKind::Laser) {
        ProjectileKind::Laser
    } else {
        ProjectileKind::Shell
    };
    let range = if kind == ProjectileKind::Laser {
        LASER_AIM_RANGE
    } else {
        AIM_RANGE
    };
    let (offset, distance) = world
        .other_tanks
        .iter()
        .filter(|tank| tank.alive)
        .map(|tank| tank.pose.position - own.position)
        .map(|offset| (offset, offset.x.hypot(offset.y)))
        .filter(|&(offset, distance)| {
            distance <= range
                && world
                    .map
                    .first_wall_contact(own.position, own.position + offset, kind.radius())
                    .is_none()
        })
        .min_by(|(_, a), (_, b)| a.total_cmp(b))?;
    let (mut action, error) = world.steer(offset.y.atan2(offset.x));
    let ray = Vec2::from_angle(own.angle + world.turn_delta(action)).scale(distance);
    action.fire = world.tick.is_multiple_of(SHOT_INTERVAL_TICKS)
        && error.abs() < 0.25
        && distance * error.sin().abs() < TANK_WIDTH / 2.0
        && world
            .map
            .first_wall_contact(own.position, own.position + ray, kind.radius())
            .is_none();
    Some(action)
}

struct Flight {
    position: Vec2,
    velocity: Vec2,
    seconds: f64,
    radius: f64,
}

pub(crate) fn dodge(world: &Observation, planned: Action) -> Action {
    let own = world.own_tank.pose;
    let delay =
        world.tick.saturating_sub(world.world_tick) as f64 / f64::from(world.ticks_per_second);
    let span = delay + DODGE_SECONDS;
    let tank_speed = 1.0 / world.params.forward_secs.min(world.params.reverse_secs);
    let flights: Vec<_> = world
        .projectiles
        .iter()
        .filter(|shot| shot.owner != world.seat || shot.wall_contacts > 0)
        .filter_map(|shot| {
            let offset = shot.position - own.position;
            let reach = tank_speed * DODGE_SECONDS
                + shot.velocity.x.hypot(shot.velocity.y) * span
                + tank_radius()
                + PROJECTILE_RADIUS;
            if offset.dot(offset) > reach * reach {
                return None;
            }
            let radius = shot.kind.radius();
            let end = shot.position + shot.velocity.scale(span);
            let until = span
                * world
                    .map
                    .first_wall_contact(shot.position, end, radius)
                    .unwrap_or(1.0);
            (until > delay).then(|| Flight {
                position: shot.position + shot.velocity.scale(delay),
                velocity: shot.velocity,
                seconds: until - delay,
                radius,
            })
        })
        .collect();
    if flights.is_empty() {
        return planned;
    }

    let clearance = |velocity: Vec2| {
        let end = own.position + velocity.scale(DODGE_SECONDS);
        let stop = DODGE_SECONDS
            * world
                .map
                .first_wall_contact(own.position, end, tank_radius())
                .unwrap_or(1.0);
        flights
            .iter()
            .map(|flight| {
                let from = flight.position - own.position;
                let delta = flight.velocity - velocity;
                let mut gap = closest_distance(from, delta, flight.seconds.min(stop));
                if flight.seconds > stop {
                    let stopped = from + delta.scale(stop);
                    gap = gap.min(closest_distance(
                        stopped,
                        flight.velocity,
                        flight.seconds - stop,
                    ));
                }
                gap - flight.radius
            })
            .fold(f64::INFINITY, f64::min)
    };
    let velocity = |action: Action| {
        let speed = match (action.forward, action.reverse) {
            (true, false) => 1.0 / world.params.forward_secs,
            (false, true) => -1.0 / world.params.reverse_secs,
            _ => 0.0,
        };
        Vec2::from_angle(own.angle + world.turn_delta(action)).scale(speed)
    };
    let mut best = planned;
    let mut best_clearance = clearance(velocity(planned));
    if best_clearance > tank_radius() {
        return planned;
    }
    for action in [
        Action {
            forward: true,
            ..Action::default()
        },
        Action {
            reverse: true,
            ..Action::default()
        },
        Action::default(),
    ] {
        let gap = clearance(velocity(action));
        if gap > best_clearance + 0.01 {
            best = action;
            best_clearance = gap;
        }
    }
    best
}

fn closest_distance(from: Vec2, velocity: Vec2, seconds: f64) -> f64 {
    let time = (-from.dot(velocity) / velocity.dot(velocity).max(f64::EPSILON)).clamp(0.0, seconds);
    let nearest = from + velocity.scale(time);
    nearest.x.hypot(nearest.y)
}
