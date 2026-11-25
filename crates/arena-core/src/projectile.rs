use crate::collision::{EPSILON, sweep_maze};
use crate::hull::OrientedBox;
use crate::{LASER_RADIUS, Maze, PROJECTILE_RADIUS, Pose, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ProjectileKind {
    Shell,
    Laser,
}

impl ProjectileKind {
    pub fn radius(self) -> f64 {
        match self {
            Self::Shell => PROJECTILE_RADIUS,
            Self::Laser => LASER_RADIUS,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct FlightSegment {
    pub start: Vec2,
    pub end: Vec2,
    pub after_bounce: bool,
}

#[derive(Clone)]
pub(super) struct Projectile {
    pub position: Vec2,
    pub(super) kind: ProjectileKind,
    pub(super) speed_scale: f64,
    direction: Vec2,
    pub(super) owner: usize,
    pub(super) contacts: u8,
}

pub(super) enum Impact {
    Tank(usize),
    Expired,
}

impl Projectile {
    pub fn new(owner: usize, pose: Pose, kind: ProjectileKind) -> Self {
        Self {
            position: pose.position,
            kind,
            speed_scale: 1.0,
            direction: Vec2::from_angle(pose.angle),
            owner,
            contacts: 0,
        }
    }

    pub fn velocity(&self, speed: f64) -> Vec2 {
        self.direction.scale(speed * self.speed_scale)
    }

    pub fn scattered(&self, angle_offset: f64, speed_scale: f64) -> Self {
        let (sin, cos) = angle_offset.sin_cos();
        Self {
            direction: Vec2::new(
                self.direction.x * cos - self.direction.y * sin,
                self.direction.x * sin + self.direction.y * cos,
            ),
            speed_scale,
            ..self.clone()
        }
    }

    pub fn advance(
        &mut self,
        maze: &Maze,
        tanks: &[Option<Pose>],
        mut distance: f64,
        contact_limit: u8,
        mut record: impl FnMut(FlightSegment),
    ) -> Option<Impact> {
        while distance > EPSILON {
            let delta = self.direction.scale(distance);
            let wall = sweep_maze(maze, self.position, delta, self.kind.radius());
            let tank = self.first_tank_contact(tanks, delta);
            if let Some((index, time)) = tank
                && wall.is_none_or(|hit| time < hit.time - EPSILON)
            {
                self.record_move(delta.scale(time), &mut record);
                return Some(Impact::Tank(index));
            }

            if let Some(hit) = wall {
                self.record_move(delta.scale(hit.time), &mut record);
                self.contacts += 1;
                if self.contacts >= contact_limit {
                    return Some(Impact::Expired);
                }
                self.direction = hit.reflect(self.direction);
                distance *= 1.0 - hit.time;
            } else {
                self.record_move(delta, &mut record);
                break;
            }
        }
        None
    }

    fn record_move(&mut self, delta: Vec2, record: &mut impl FnMut(FlightSegment)) {
        let start = self.position;
        self.position = start + delta;
        if start != self.position {
            record(FlightSegment {
                start,
                end: self.position,
                after_bounce: self.contacts != 0,
            });
        }
    }

    fn first_tank_contact(&self, tanks: &[Option<Pose>], delta: Vec2) -> Option<(usize, f64)> {
        tanks
            .iter()
            .enumerate()
            .filter(|&(index, _)| index != self.owner || self.contacts != 0)
            .filter_map(|(index, &tank)| {
                OrientedBox::tank(tank?)
                    .disc_contact(self.position, delta, self.kind.radius())
                    .map(|time| (index, time))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
}

#[cfg(test)]
mod tests {
    use crate::Wall;

    use super::*;

    #[test]
    fn scattered_shell_inherits_muzzle_wall_contact() {
        let poses = [
            Pose {
                position: Vec2::new(0.62, 0.5),
                angle: 0.0,
            },
            Pose {
                position: Vec2::new(3.5, 3.5),
                angle: 0.0,
            },
        ];
        let maze = Maze {
            width: 4,
            height: 4,
            walls: vec![Wall {
                a: Vec2::new(1.0, 0.0),
                b: Vec2::new(1.0, 2.0),
            }],
        };
        let mut barrel = Projectile::new(0, poses[0], ProjectileKind::Shell);
        assert!(
            barrel
                .advance(&maze, &poses.map(Some), crate::MUZZLE_DISTANCE, 10, |_| {})
                .is_none()
        );
        let pellet = barrel.scattered(0.2, 1.1);
        assert_eq!(pellet.contacts, 1);
        assert_eq!(pellet.owner, 0);
        assert_eq!(pellet.position, barrel.position);
        assert_eq!(pellet.speed_scale, 1.1);
    }
}
