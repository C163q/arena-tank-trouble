use std::f64::consts::{PI, TAU};

use crate::collision::Bounds;
use crate::hull::OrientedBox;
use crate::{Maze, Pose, TANK_LENGTH, TANK_WIDTH, TICKS_PER_SECOND, Vec2};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Action {
    pub forward: bool,
    pub reverse: bool,
    pub turn_left: bool,
    pub turn_right: bool,
    pub fire: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Params {
    pub forward_secs: f64,
    pub reverse_secs: f64,
    pub turn_secs: f64,
    pub reload_secs: f64,
    pub wall_contacts: u8,
    #[cfg_attr(feature = "serde", serde(default = "default_laser_trail_secs"))]
    pub laser_trail_secs: f64,
}

fn default_laser_trail_secs() -> f64 {
    0.20
}

impl Default for Params {
    fn default() -> Self {
        Self {
            forward_secs: 0.45,
            reverse_secs: 0.45,
            turn_secs: 1.5,
            reload_secs: 3.0,
            wall_contacts: 10,
            laser_trail_secs: default_laser_trail_secs(),
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
            (self.laser_trail_secs, 0.05..=2.00, "laser_trail_secs"),
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

pub fn drive(maze: &Maze, pose: Pose, other: Pose, action: Action, params: &Params) -> Pose {
    drive_with_other(maze, pose, Some(other), action, params)
}

pub(super) fn drive_with_other(
    maze: &Maze,
    mut pose: Pose,
    other: Option<Pose>,
    action: Action,
    params: &Params,
) -> Pose {
    let turn = f64::from(i8::from(action.turn_right) - i8::from(action.turn_left));
    let speed = match (action.forward, action.reverse) {
        (true, false) => 1.0 / params.forward_secs,
        (false, true) => -1.0 / params.reverse_secs,
        _ => 0.0,
    };
    if turn == 0.0 && speed == 0.0 {
        return pose;
    }
    let travel = speed / f64::from(TICKS_PER_SECOND);
    let hull_radius = TANK_LENGTH.hypot(TANK_WIDTH) / 2.0;
    let reach = hull_radius + travel.abs();
    let mut obstacles: Vec<_> = maze
        .walls
        .iter()
        .copied()
        .map(Bounds::wall)
        .filter(|bounds| bounds.near(pose.position, reach))
        .map(OrientedBox::axis_aligned)
        .collect();
    if let Some(other) = other.filter(|other| {
        let separation = other.position - pose.position;
        separation.dot(separation) <= (reach + hull_radius).powi(2)
    }) {
        obstacles.push(OrientedBox::tank(other));
    }

    if turn != 0.0 {
        let change = turn * TAU / (params.turn_secs * f64::from(TICKS_PER_SECOND));
        let available_turn = OrientedBox::tank(pose).turn_fraction(change, &obstacles);
        let angle = pose.angle + change * available_turn;
        pose.angle = (angle + PI).rem_euclid(TAU) - PI;
    }
    if speed != 0.0 {
        let movement = Vec2::from_angle(pose.angle).scale(travel);
        let hull = OrientedBox::tank(pose);
        let fraction = obstacles
            .into_iter()
            .filter_map(|obstacle| hull.movement_contact(movement, obstacle))
            .fold(1.0, f64::min);
        pose.position = pose.position + movement.scale(fraction);
    }
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
            ((reverse.position.x - 0.5).hypot(reverse.position.y - 0.5) - 1.0 / 27.0).abs() < 1e-9
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
            position: Vec2::new(0.75, 0.5),
            angle: 0.0,
        };
        let forward_right = Action {
            forward: true,
            turn_right: true,
            ..Action::default()
        };
        let stopped = drive(&maze, against_wall, other, forward_right, &params);
        assert!((stopped.position.x - against_wall.position.x).abs() < 1e-8);
        assert!(stopped.angle.abs() < 1e-7);
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
            position: Vec2::new(0.92, 0.5),
            angle: 0.0,
        };
        let forward = Action {
            forward: true,
            ..Action::default()
        };
        let stopped = drive(&maze, pose, touching, forward, &params);
        assert!((stopped.position.x - pose.position.x).abs() < 1e-8);
    }

    #[test]
    fn drive_can_follow_a_wall_when_the_hull_has_clearance() {
        let maze = Maze {
            width: 4,
            height: 4,
            walls: vec![Wall {
                a: Vec2::new(0.0, 1.0),
                b: Vec2::new(2.0, 1.0),
            }],
        };
        let other = Pose {
            position: Vec2::new(3.5, 3.5),
            angle: 0.0,
        };
        let moving = Action {
            forward: true,
            ..Action::default()
        };
        let almost_parallel = Pose {
            position: Vec2::new(0.7, 1.27),
            angle: -0.07,
        };
        let after = drive(&maze, almost_parallel, other, moving, &Params::default());
        assert!(after.position.x > 0.736);

        let tangent = Pose {
            position: Vec2::new(0.7, 1.21),
            angle: 0.0,
        };
        assert!(
            drive(&maze, tangent, other, moving, &Params::default())
                .position
                .x
                > 0.736
        );
    }

    #[test]
    fn square_wall_end_blocks_an_overlapping_tank_corner() {
        let maze = Maze {
            width: 4,
            height: 4,
            walls: vec![Wall {
                a: Vec2::new(1.0, 1.0),
                b: Vec2::new(2.0, 1.0),
            }],
        };
        let pose = Pose {
            position: Vec2::new(2.27, 1.206),
            angle: PI,
        };
        let other = Pose {
            position: Vec2::new(3.5, 3.5),
            angle: 0.0,
        };
        let forward = Action {
            forward: true,
            ..Action::default()
        };
        let after = drive(&maze, pose, other, forward, &Params::default());
        assert!((after.position.x - 2.25).abs() < 1e-8);
    }

    #[test]
    fn rotation_stops_where_a_hull_corner_reaches_the_wall() {
        let maze = Maze {
            width: 4,
            height: 4,
            walls: vec![Wall {
                a: Vec2::new(0.0, 1.0),
                b: Vec2::new(2.0, 1.0),
            }],
        };
        let pose = Pose {
            position: Vec2::new(0.7, 1.213),
            angle: 0.0,
        };
        let other = Pose {
            position: Vec2::new(3.5, 3.5),
            angle: 0.0,
        };
        let turning = Action {
            turn_right: true,
            ..Action::default()
        };
        let after = drive(&maze, pose, other, turning, &Params::default());
        assert_eq!(after.position, pose.position);
        assert!(after.angle > 0.0 && after.angle < TAU / 90.0);
        let extent = 0.21 * after.angle.sin() + 0.17 * after.angle.cos();
        assert!(after.position.y - extent >= 1.04 - 1e-8);
        let further = drive(&maze, after, other, turning, &Params::default());
        assert!((further.angle - after.angle).abs() < 1e-7);
    }

    #[test]
    fn turning_stops_at_the_other_tank_body() {
        let maze = Maze {
            width: 4,
            height: 4,
            walls: vec![],
        };
        let pose = Pose {
            position: Vec2::new(0.7, 1.343),
            angle: 0.0,
        };
        let other = Pose {
            position: Vec2::new(0.7, 1.0),
            angle: 0.0,
        };
        let turn = Action {
            turn_right: true,
            ..Action::default()
        };
        let after = drive(&maze, pose, other, turn, &Params::default());
        assert_eq!(after.position, pose.position);
        assert!((0.013..0.016).contains(&after.angle));
    }

    #[test]
    fn turning_cannot_skip_a_brief_corner_contact() {
        let maze = Maze {
            width: 4,
            height: 4,
            walls: vec![Wall {
                a: Vec2::new(0.0, 1.0),
                b: Vec2::new(2.0, 1.0),
            }],
        };
        // The corner would cross the wall for barely half a degree.
        let pose = Pose {
            position: Vec2::new(0.7, 1.310182121722126),
            angle: 50.5090059575_f64.to_radians(),
        };
        let other = Pose {
            position: Vec2::new(3.5, 3.5),
            angle: 0.0,
        };
        let turn = Action {
            turn_right: true,
            ..Action::default()
        };
        let after = drive(&maze, pose, other, turn, &Params::default());
        assert!((50.73..50.75).contains(&after.angle.to_degrees()));
    }

    #[test]
    fn driving_after_a_blocked_turn_cannot_enter_the_wall() {
        let maze = Maze {
            width: 4,
            height: 4,
            walls: vec![Wall {
                a: Vec2::new(0.0, 1.0),
                b: Vec2::new(2.0, 1.0),
            }],
        };
        let other = Pose {
            position: Vec2::new(3.5, 3.5),
            angle: 0.0,
        };
        let mut pose = Pose {
            position: Vec2::new(0.7, 1.213),
            angle: 0.0,
        };
        let action = Action {
            turn_left: true,
            forward: true,
            ..Action::default()
        };
        for _ in 0..5 {
            pose = drive(&maze, pose, other, action, &Params::default());
            let extent = 0.21 * pose.angle.sin().abs() + 0.17 * pose.angle.cos().abs();
            assert!(pose.position.y - extent >= 1.04 - 1e-8, "{pose:?}");
        }
    }

    #[test]
    fn playtest_defaults_use_matching_drive_speeds_and_slower_reload() {
        let params = Params::default();
        assert_eq!(params.forward_secs, 0.45);
        assert_eq!(params.reverse_secs, 0.45);
        assert_eq!(params.turn_secs, 1.5);
        assert_eq!(params.reload_secs, 3.0);
        assert_eq!(params.wall_contacts, 10);
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
                laser_trail_secs: 0.05,
            },
            Params {
                forward_secs: 3.0,
                reverse_secs: 3.0,
                turn_secs: 6.0,
                reload_secs: 10.0,
                wall_contacts: 20,
                laser_trail_secs: 2.00,
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
                laser_trail_secs: 0.04,
                ..Params::default()
            },
            Params {
                laser_trail_secs: 2.01,
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
