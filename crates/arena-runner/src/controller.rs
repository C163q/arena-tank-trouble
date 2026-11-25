use std::f64::consts::{PI, TAU};

use arena_core::{Action, Vec2, WALL_THICKNESS, Wall};
use serde::Serialize;

use crate::matchplay::Observation;

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    #[cfg(target_os = "linux")]
    Timeout,
    #[cfg(target_os = "linux")]
    Error,
    Eliminated,
}

#[derive(Serialize)]
pub struct Decision {
    pub action: Action,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

impl Decision {
    pub fn ok(action: Action) -> Self {
        Self {
            action,
            status: Status::Ok,
            diagnostic: None,
        }
    }

    pub fn eliminated() -> Self {
        Self {
            action: Action::default(),
            status: Status::Eliminated,
            diagnostic: None,
        }
    }

    #[cfg(target_os = "linux")]
    pub fn failed(status: Status, error: std::io::Error) -> Self {
        Self {
            action: Action::default(),
            status,
            diagnostic: Some(error.to_string()),
        }
    }
}

pub trait Controller {
    fn name(&self) -> &str;
    fn decide(&mut self, observation: &Observation<'_>) -> Decision;
    fn eliminate(&mut self) {}
}

pub struct Demo {
    pub name: String,
}

impl Controller for Demo {
    fn name(&self) -> &str {
        &self.name
    }

    fn decide(&mut self, obs: &Observation<'_>) -> Decision {
        let own = obs.own_tank.pose;
        let cautious = obs.seat == 0;
        let target = obs
            .other_tanks
            .iter()
            .filter(|other| other.tank.alive)
            .map(|other| {
                let pos = other.tank.pose.position;
                let (x, y) = (pos.x - own.position.x, pos.y - own.position.y);
                (pos, x.hypot(y), y.atan2(x))
            })
            .filter(|(pos, _, _)| !cautious || clear_line(own.position, *pos, &obs.map.walls))
            .min_by(|(_, a, _), (_, b, _)| a.total_cmp(b));
        let phase = obs.tick + (19 * obs.seat as u64);
        if let Some((_, distance, angle)) = target
            && distance <= if cautious { 6.0 } else { 3.5 }
            && phase % 180 < 125
        {
            let offset = (angle - own.angle + PI).rem_euclid(TAU) - PI;
            return Decision::ok(Action {
                forward: !cautious && offset.abs() < 0.4 && distance > 0.75,
                turn_left: offset < -0.05,
                turn_right: offset > 0.05,
                fire: offset.abs() < 0.12 && phase.is_multiple_of(if cautious { 120 } else { 24 }),
                ..Action::default()
            });
        }
        Decision::ok(Action {
            forward: phase % 137 < 107,
            turn_right: phase % 137 >= 107,
            fire: !cautious && phase.is_multiple_of(87),
            ..Action::default()
        })
    }
}

fn clear_line(from: Vec2, to: Vec2, walls: &[Wall]) -> bool {
    walls.iter().all(|wall| {
        let mut enter: f64 = 0.0;
        let mut exit: f64 = 1.0;
        for (start, end, a, b) in [
            (from.x, to.x, wall.a.x, wall.b.x),
            (from.y, to.y, wall.a.y, wall.b.y),
        ] {
            let min = a.min(b) - WALL_THICKNESS / 2.0;
            let max = a.max(b) + WALL_THICKNESS / 2.0;
            let delta = end - start;
            if delta.abs() < 1e-9 {
                if start < min || start > max {
                    return true;
                }
            } else {
                let first = (min - start) / delta;
                let second = (max - start) / delta;
                enter = enter.max(first.min(second));
                exit = exit.min(first.max(second));
            }
        }
        enter > exit
    })
}
