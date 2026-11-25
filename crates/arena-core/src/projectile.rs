use crate::collision::{EPSILON, sweep_maze};
use crate::hull::OrientedBox;
use crate::{Maze, PROJECTILE_RADIUS, Pose, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ProjectileKind {
    Shell,
    Laser,
}

#[derive(Clone, Copy)]
pub(super) struct FlightSegment {
    pub start: Vec2,
    pub end: Vec2,
    pub after_bounce: bool,
}

pub(super) struct Projectile {
    pub position: Vec2,
    pub(super) kind: ProjectileKind,
    direction: Vec2,
    owner: usize,
    contacts: u8,
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
            direction: Vec2::from_angle(pose.angle),
            owner,
            contacts: 0,
        }
    }

    pub fn advance(
        &mut self,
        maze: &Maze,
        tanks: &[Pose; 2],
        mut distance: f64,
        contact_limit: u8,
        mut record: impl FnMut(FlightSegment),
    ) -> Option<Impact> {
        while distance > EPSILON {
            let delta = self.direction.scale(distance);
            let wall = sweep_maze(maze, self.position, delta, PROJECTILE_RADIUS);
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
                self.direction = if hit.corner {
                    self.direction.scale(-1.0)
                } else {
                    self.direction - hit.normal.scale(2.0 * self.direction.dot(hit.normal))
                };
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

    fn first_tank_contact(&self, tanks: &[Pose; 2], delta: Vec2) -> Option<(usize, f64)> {
        tanks
            .iter()
            .enumerate()
            .filter(|&(index, _)| index != self.owner || self.contacts != 0)
            .filter_map(|(index, &tank)| {
                OrientedBox::tank(tank)
                    .disc_contact(self.position, delta, PROJECTILE_RADIUS)
                    .map(|time| (index, time))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
}
