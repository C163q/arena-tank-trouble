use crate::collision::{EPSILON, sweep_maze};
use crate::hull::Hull;
use crate::{Maze, PROJECTILE_RADIUS, Pose, Vec2};

pub(super) struct Projectile {
    pub position: Vec2,
    direction: Vec2,
    owner: usize,
    contacts: u8,
}

pub(super) enum Impact {
    Tank(usize),
    Expired,
}

impl Projectile {
    pub fn new(owner: usize, pose: Pose) -> Self {
        Self {
            position: pose.position,
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
    ) -> Option<Impact> {
        while distance > EPSILON {
            let delta = self.direction.scale(distance);
            let wall = sweep_maze(maze, self.position, delta, PROJECTILE_RADIUS);
            let tank = self.first_tank_contact(tanks, delta);
            if let Some((index, time)) = tank
                && wall.is_none_or(|hit| time < hit.time - EPSILON)
            {
                return Some(Impact::Tank(index));
            }

            if let Some(hit) = wall {
                self.contacts += 1;
                if self.contacts >= contact_limit {
                    return Some(Impact::Expired);
                }
                self.position = self.position + delta.scale(hit.time);
                self.direction = if hit.corner {
                    self.direction.scale(-1.0)
                } else {
                    self.direction - hit.normal.scale(2.0 * self.direction.dot(hit.normal))
                };
                distance *= 1.0 - hit.time;
            } else {
                self.position = self.position + delta;
                break;
            }
        }
        None
    }

    fn first_tank_contact(&self, tanks: &[Pose; 2], delta: Vec2) -> Option<(usize, f64)> {
        tanks
            .iter()
            .enumerate()
            .filter(|&(index, _)| index != self.owner || self.contacts != 0)
            .filter_map(|(index, &tank)| {
                Hull::new(tank)
                    .disc_contact(self.position, delta, PROJECTILE_RADIUS)
                    .map(|time| (index, time))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
}
