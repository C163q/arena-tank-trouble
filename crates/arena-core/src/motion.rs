use std::f64::consts::{PI, TAU};

use crate::collision::{sweep_circle, sweep_maze};
use crate::{Maze, Pose, TANK_RADIUS, TICKS_PER_SECOND, Vec2};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Action {
    pub forward: bool,
    pub reverse: bool,
    pub turn_left: bool,
    pub turn_right: bool,
    pub fire: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    pub forward_secs: f64,
    pub reverse_secs: f64,
    pub turn_secs: f64,
    pub reload_secs: f64,
    pub wall_contacts: u8,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            forward_secs: 0.45,
            reverse_secs: 0.65,
            turn_secs: 1.5,
            reload_secs: 1.0,
            wall_contacts: 5,
        }
    }
}

impl Params {
    pub fn validate(&self) -> Result<(), &'static str> {
        for (value, bounds, field) in [
            (self.forward_secs, 0.15..=3.0, "forward_secs"),
            (self.reverse_secs, 0.15..=3.0, "reverse_secs"),
            (self.turn_secs, 0.25..=6.0, "turn_secs"),
            (self.reload_secs, 0.2..=10.0, "reload_secs"),
        ] {
            if !bounds.contains(&value) {
                return Err(field);
            }
        }
        if !(1..=20).contains(&self.wall_contacts) {
            return Err("wall_contacts");
        }
        Ok(())
    }
}

pub fn drive(maze: &Maze, mut pose: Pose, other: Pose, action: Action, params: &Params) -> Pose {
    let turn = f64::from(i8::from(action.turn_right) - i8::from(action.turn_left));
    if turn != 0.0 {
        let angle = pose.angle + turn * TAU / (params.turn_secs * f64::from(TICKS_PER_SECOND));
        pose.angle = (angle + PI).rem_euclid(TAU) - PI;
    }
    let speed = match (action.forward, action.reverse) {
        (true, false) => 1.0 / params.forward_secs,
        (false, true) => -1.0 / params.reverse_secs,
        _ => return pose,
    };
    let delta = Vec2::from_angle(pose.angle).scale(speed / f64::from(TICKS_PER_SECOND));
    let wall = sweep_maze(maze, pose.position, delta, TANK_RADIUS);
    let tank = sweep_circle(pose.position, delta, other.position, 2.0 * TANK_RADIUS);
    let fraction = wall
        .into_iter()
        .chain(tank)
        .map(|hit| hit.time)
        .fold(1.0, f64::min);
    pose.position = pose.position + delta.scale(fraction);
    pose
}

#[cfg(test)]
mod tests {
    use std::f64::consts::TAU;

    use super::*;
    use crate::{Vec2, Wall};

    #[test]
    fn drive_turn_and_block() {
        let maze = Maze {
            width: 4,
            height: 4,
            walls: vec![Wall {
                a: Vec2::new(1.0, 0.0),
                b: Vec2::new(1.0, 1.0),
            }],
        };
        let pose = Pose {
            position: Vec2::new(0.5, 0.5),
            angle: 0.0,
        };
        let other = Pose {
            position: Vec2::new(3.5, 3.5),
            angle: 0.0,
        };
        let params = Params::default();
        let after = drive(
            &maze,
            pose,
            other,
            Action {
                forward: true,
                turn_right: true,
                ..Action::default()
            },
            &params,
        );
        let length = (after.position.x - 0.5).hypot(after.position.y - 0.5);
        assert!((length - 1.0 / 27.0).abs() < 1e-9);
        assert!((after.angle - TAU / 90.0).abs() < 1e-9);
        assert!(after.position.y > 0.5);

        let reverse = drive(
            &maze,
            pose,
            other,
            Action {
                reverse: true,
                turn_left: true,
                ..Action::default()
            },
            &params,
        );
        assert!(reverse.angle < 0.0);
        assert!(
            ((reverse.position.x - 0.5).hypot(reverse.position.y - 0.5) - 1.0 / 39.0).abs() < 1e-9
        );

        let opposite = Action {
            forward: true,
            reverse: true,
            turn_left: true,
            turn_right: true,
            fire: false,
        };
        assert_eq!(drive(&maze, pose, other, opposite, &params), pose);

        let against_wall = Pose {
            position: Vec2::new(0.73, 0.5),
            angle: 0.0,
        };
        let forward_right = Action {
            forward: true,
            turn_right: true,
            ..Action::default()
        };
        let stopped = drive(&maze, against_wall, other, forward_right, &params);
        assert_eq!(stopped.position, against_wall.position);
        assert!(stopped.angle > 0.0);
        let away = drive(
            &maze,
            against_wall,
            other,
            Action {
                reverse: true,
                ..Action::default()
            },
            &params,
        );
        assert!(away.position.x < against_wall.position.x);

        let touching = Pose {
            position: Vec2::new(0.96, 0.5),
            angle: 0.0,
        };
        let stopped = drive(&maze, pose, touching, forward_right, &params);
        assert_eq!(stopped.position, pose.position);
        assert!(stopped.angle > 0.0);
    }

    #[test]
    fn parameter_ranges() {
        for params in [
            Params::default(),
            Params {
                forward_secs: 0.15,
                reverse_secs: 0.15,
                turn_secs: 0.25,
                reload_secs: 0.2,
                wall_contacts: 1,
            },
            Params {
                forward_secs: 3.0,
                reverse_secs: 3.0,
                turn_secs: 6.0,
                reload_secs: 10.0,
                wall_contacts: 20,
            },
        ] {
            assert!(params.validate().is_ok());
        }
        for params in [
            Params {
                forward_secs: 0.0,
                ..Params::default()
            },
            Params {
                reverse_secs: f64::NAN,
                ..Params::default()
            },
            Params {
                turn_secs: f64::INFINITY,
                ..Params::default()
            },
            Params {
                reload_secs: 10.1,
                ..Params::default()
            },
            Params {
                wall_contacts: 21,
                ..Params::default()
            },
        ] {
            assert!(params.validate().is_err());
        }
    }
}
