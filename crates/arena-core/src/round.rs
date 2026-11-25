use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::hull::OrientedBox;
use crate::pickups::Pickups;
use crate::projectile::{Impact, Projectile};
use crate::{
    Action, LASER_RADIUS, MUZZLE_DISTANCE, Maze, Params, Pickup, PickupKind, Pose, ProjectileKind,
    TICKS_PER_SECOND, Vec2, drive,
};

const MAGAZINE_CAPACITY: u8 = 5;
const PROJECTILE_SPEED_RATIO: f64 = 1.125;
const LASER_SPEED_MULTIPLIER: f64 = 6.0;
const LASER_WALL_CONTACTS: u8 = 5;
const SHOTGUN_PELLETS: usize = 3;
const SHOTGUN_HALF_ANGLE: f64 = 20.0_f64.to_radians();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum RoundOutcome {
    Running,
    TargetHit,
    SelfHit,
    Draw,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TankView {
    pub pose: Pose,
    pub ammo: u8,
    pub laser_loaded: bool,
    pub next_shot: Option<PickupKind>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ProjectileView {
    pub position: Vec2,
    pub kind: ProjectileKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LaserTrail {
    pub start: Vec2,
    pub end: Vec2,
    pub expires_at: u64,
}

#[derive(Debug, PartialEq)]
pub struct RoundView<'a> {
    pub seed: u64,
    pub tick: u64,
    pub maze: &'a Maze,
    pub params: &'a Params,
    pub tanks: [TankView; 2],
    pub projectiles: Vec<ProjectileView>,
    pub pickups: &'a [Pickup],
    pub laser_trails: &'a [LaserTrail],
    pub outcome: RoundOutcome,
}

pub struct Round {
    seed: u64,
    tick: u64,
    maze: Maze,
    params: Params,
    tanks: [Tank; 2],
    projectiles: Vec<Projectile>,
    pickups: Pickups,
    shotgun_rng: ChaCha8Rng,
    laser_trails: Vec<LaserTrail>,
    outcome: RoundOutcome,
    reload_ticks: u64,
    laser_trail_ticks: u64,
}

struct Tank {
    pose: Pose,
    ammo: u8,
    refill_at: Option<u64>,
    next_shot: Option<PickupKind>,
}

enum Shot {
    Shell,
    Laser,
    Shotgun,
}

impl Tank {
    fn refill(&mut self, tick: u64, interval: u64) {
        if self.refill_at == Some(tick) {
            self.ammo += 1;
            self.refill_at = (self.ammo < MAGAZINE_CAPACITY).then_some(tick + interval);
        }
    }

    fn take_shot(&mut self, tick: u64, reload_ticks: u64) -> Option<Shot> {
        if let Some(kind) = self.next_shot.take() {
            return Some(match kind {
                PickupKind::Laser => Shot::Laser,
                PickupKind::Shotgun => Shot::Shotgun,
            });
        }
        if self.ammo == 0 {
            return None;
        }
        self.ammo -= 1;
        self.refill_at.get_or_insert(tick + reload_ticks);
        Some(Shot::Shell)
    }
}

impl Round {
    pub fn new(seed: u64, params: Params) -> Result<Self, &'static str> {
        params.validate()?;
        let (maze, spawns) = Maze::generate(seed);
        Ok(Self::from_maze(seed, params, maze, spawns))
    }

    fn from_maze(seed: u64, params: Params, maze: Maze, spawns: [Pose; 2]) -> Self {
        Self {
            seed,
            tick: 0,
            maze,
            params,
            tanks: spawns.map(|pose| Tank {
                pose,
                ammo: MAGAZINE_CAPACITY,
                refill_at: None,
                next_shot: None,
            }),
            projectiles: Vec::new(),
            pickups: Pickups::new(seed),
            shotgun_rng: ChaCha8Rng::seed_from_u64(seed ^ 0x94D0_49BB_1331_11EB),
            laser_trails: Vec::new(),
            outcome: RoundOutcome::Running,
            reload_ticks: interval_ticks(params.reload_secs),
            laser_trail_ticks: interval_ticks(params.laser_trail_secs),
        }
    }

    pub fn step(&mut self, actions: [Action; 2]) {
        if self.outcome != RoundOutcome::Running {
            return;
        }
        self.tick += 1;
        self.laser_trails
            .retain(|trail| trail.expires_at > self.tick);
        for (index, action) in actions.into_iter().enumerate() {
            let other = self.tanks[1 - index].pose;
            let tank = &mut self.tanks[index];
            tank.refill(self.tick, self.reload_ticks);
            tank.pose = drive(&self.maze, tank.pose, other, action, &self.params);
        }
        let poses = [self.tanks[0].pose, self.tanks[1].pose];
        for (tank, collected) in self.tanks.iter_mut().zip(self.pickups.collect(poses)) {
            if let Some(kind) = collected {
                tank.next_shot = Some(kind);
            }
        }
        let mut hits = [false; 2];
        let expires_at = self.tick + self.laser_trail_ticks;
        let mut advance = |bullet: &mut Projectile, distance, in_barrel: bool| {
            let laser = bullet.kind == ProjectileKind::Laser;
            let contact_limit = if laser {
                LASER_WALL_CONTACTS
            } else {
                self.params.wall_contacts
            };
            let impact = bullet.advance(&self.maze, &poses, distance, contact_limit, |segment| {
                if laser && (!in_barrel || segment.after_bounce) {
                    self.laser_trails.push(LaserTrail {
                        start: segment.start,
                        end: segment.end,
                        expires_at,
                    });
                }
            });
            match impact {
                None => true,
                Some(Impact::Tank(index)) => {
                    hits[index] = true;
                    false
                }
                Some(Impact::Expired) => false,
            }
        };
        for (index, action) in actions.into_iter().enumerate() {
            if !action.fire {
                continue;
            }
            let tank = &mut self.tanks[index];
            let Some(shot) = tank.take_shot(self.tick, self.reload_ticks) else {
                continue;
            };
            let kind = match shot {
                Shot::Laser => ProjectileKind::Laser,
                Shot::Shell | Shot::Shotgun => ProjectileKind::Shell,
            };
            let spread = matches!(shot, Shot::Shotgun).then(|| {
                [(); SHOTGUN_PELLETS].map(|()| {
                    (
                        self.shotgun_rng
                            .gen_range(-SHOTGUN_HALF_ANGLE..=SHOTGUN_HALF_ANGLE),
                        self.shotgun_rng.gen_range(0.85..=1.15),
                    )
                })
            });
            let mut bullet = Projectile::new(index, tank.pose, kind);
            if !advance(&mut bullet, MUZZLE_DISTANCE, true) {
                continue;
            }
            if let Some(spread) = spread {
                self.projectiles
                    .extend(spread.map(|(angle, speed)| bullet.scattered(angle, speed)));
            } else {
                self.projectiles.push(bullet);
            }
        }
        let projectile_step =
            PROJECTILE_SPEED_RATIO / self.params.forward_secs / f64::from(TICKS_PER_SECOND);
        self.projectiles.retain_mut(|bullet| {
            let distance = if bullet.kind == ProjectileKind::Laser {
                projectile_step * LASER_SPEED_MULTIPLIER
            } else {
                projectile_step * bullet.speed_scale
            };
            advance(bullet, distance, false)
        });
        // Include new segments: their final point can touch both tanks at once.
        for (hit, pose) in hits.iter_mut().zip(poses) {
            let hull = OrientedBox::tank(pose);
            *hit = *hit
                || self.laser_trails.iter().any(|trail| {
                    hull.disc_contact(trail.start, trail.end - trail.start, LASER_RADIUS)
                        .is_some()
                });
        }
        self.outcome = match hits {
            [false, false] => RoundOutcome::Running,
            [false, true] => RoundOutcome::TargetHit,
            [true, false] => RoundOutcome::SelfHit,
            [true, true] => RoundOutcome::Draw,
        };
        if self.outcome == RoundOutcome::Running {
            self.pickups.spawn_due(self.tick, &self.maze, poses);
        }
    }

    pub fn view(&self) -> RoundView<'_> {
        RoundView {
            seed: self.seed,
            tick: self.tick,
            maze: &self.maze,
            params: &self.params,
            tanks: std::array::from_fn(|index| {
                let tank = &self.tanks[index];
                TankView {
                    pose: tank.pose,
                    ammo: tank.ammo,
                    laser_loaded: tank.next_shot == Some(PickupKind::Laser),
                    next_shot: tank.next_shot,
                }
            }),
            projectiles: self
                .projectiles
                .iter()
                .map(|bullet| ProjectileView {
                    position: bullet.position,
                    kind: bullet.kind,
                })
                .collect(),
            pickups: self.pickups.items(),
            laser_trails: &self.laser_trails,
            outcome: self.outcome,
        }
    }
}

fn interval_ticks(seconds: f64) -> u64 {
    let exact = seconds * f64::from(TICKS_PER_SECOND);
    // Keep a whole tick whole when a decimal setting rounds slightly up.
    (exact - exact * f64::EPSILON).ceil() as u64
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

    use crate::Wall;

    use super::*;

    fn pose(x: f64, y: f64, angle: f64) -> Pose {
        Pose {
            position: Vec2::new(x, y),
            angle,
        }
    }

    fn wall(a: (f64, f64), b: (f64, f64)) -> Wall {
        Wall {
            a: Vec2::new(a.0, a.1),
            b: Vec2::new(b.0, b.1),
        }
    }

    fn fixture(walls: Vec<Wall>, player: Pose, target: Pose, params: Params) -> Round {
        Round::from_maze(
            0,
            params,
            Maze {
                width: 12,
                height: 8,
                walls,
            },
            [player, target],
        )
    }

    fn fire(round: &mut Round) {
        round.step([
            Action {
                fire: true,
                ..Action::default()
            },
            Action::default(),
        ]);
    }

    fn idle(round: &mut Round, ticks: usize) {
        for _ in 0..ticks {
            round.step([Action::default(); 2]);
        }
    }

    #[test]
    fn ordinary_projectile_is_slightly_faster_than_forward_drive() {
        for (forward_secs, expected_speed) in [(0.45, 2.5), (0.90, 1.25)] {
            let params = Params {
                forward_secs,
                ..Params::default()
            };
            let mut round = fixture(vec![], pose(1.0, 1.0, 0.0), pose(1.0, 7.0, 0.0), params);
            fire(&mut round);
            let first = round.view().projectiles[0].position;
            idle(&mut round, 1);
            let second = round.view().projectiles[0].position;
            let speed = (second.x - first.x) * f64::from(TICKS_PER_SECOND);
            assert!(
                (speed - expected_speed).abs() < 1e-9,
                "{forward_secs}s: {speed}"
            );
        }
    }

    #[test]
    fn turning_can_dodge_a_shot_without_moving_the_tank_center() {
        let start = pose(4.0, 1.23, FRAC_PI_2);
        let make_round = || fixture(vec![], pose(1.0, 1.0, 0.0), start, Params::default());
        let mut stationary = make_round();
        fire(&mut stationary);
        idle(&mut stationary, 70);
        assert_eq!(stationary.view().outcome, RoundOutcome::TargetHit);

        let mut turning = make_round();
        fire(&mut turning);
        for _ in 0..22 {
            turning.step([
                Action::default(),
                Action {
                    turn_left: true,
                    ..Action::default()
                },
            ]);
        }
        idle(&mut turning, 60);
        let view = turning.view();
        assert_eq!(view.tanks[1].pose.position, start.position);
        assert_eq!(view.outcome, RoundOutcome::Running);
        assert!(view.projectiles[0].position.x > start.position.x + 0.3);
    }

    #[test]
    fn latest_pickup_replaces_the_pending_shot() {
        for order in [
            [PickupKind::Laser, PickupKind::Shotgun],
            [PickupKind::Shotgun, PickupKind::Laser],
        ] {
            let mut round = Round::new(42, Params::default()).unwrap();
            for tick in [900, 1500, 2100, 2700, 3300] {
                round.tick = tick - 1;
                idle(&mut round, 1);
            }
            for kind in order {
                let item = round
                    .view()
                    .pickups
                    .iter()
                    .find(|item| item.kind == kind)
                    .unwrap();
                let position = item.position;
                round.tanks[0].pose.position = position;
                idle(&mut round, 1);
                assert_eq!(round.view().tanks[0].next_shot, Some(kind));
                assert!(
                    !round
                        .view()
                        .pickups
                        .iter()
                        .any(|item| item.position == position)
                );
            }
        }
    }

    #[test]
    fn shotgun_fire_uses_no_ammo() {
        let mut round = fixture(
            vec![],
            pose(1.0, 1.0, 0.0),
            pose(1.0, 7.0, 0.0),
            Params::default(),
        );
        round.tanks[0].ammo = 0;
        round.tanks[0].refill_at = Some(4);
        round.tanks[0].next_shot = Some(PickupKind::Shotgun);

        fire(&mut round);
        let volley = round.view();
        assert_eq!(volley.projectiles.len(), 3);
        assert!(
            volley
                .projectiles
                .iter()
                .all(|shot| shot.kind == ProjectileKind::Shell)
        );
        assert_eq!(volley.tanks[0].ammo, 0);
        assert_eq!(volley.tanks[0].next_shot, None);
        assert_eq!(round.tanks[0].refill_at, Some(4));
        fire(&mut round);
        assert_eq!(round.view().projectiles.len(), 3);
        idle(&mut round, 1);
        assert_eq!(round.view().tanks[0].ammo, 0);
        fire(&mut round);
        assert_eq!(round.view().projectiles.len(), 4);
        assert_eq!(round.view().tanks[0].ammo, 0);
    }

    #[test]
    fn shotgun_randomness_is_replayable_and_independent() {
        let make_round = |seed| {
            Round::from_maze(
                seed,
                Params::default(),
                Maze {
                    width: 12,
                    height: 8,
                    walls: vec![],
                },
                [pose(1.0, 1.0, 0.0), pose(1.0, 7.0, 0.0)],
            )
        };
        let mut has_one_sided_volley = false;
        let mut has_different_speeds = false;
        let mut has_distinct_angles = false;
        for seed in 0..24 {
            let mut first = make_round(seed);
            let mut replay = make_round(seed);
            first.tanks[0].next_shot = Some(PickupKind::Shotgun);
            replay.tanks[0].next_shot = Some(PickupKind::Shotgun);
            fire(&mut first);
            fire(&mut replay);
            assert_eq!(first.view(), replay.view());
            let positions = first.view().projectiles;
            assert_eq!(positions.len(), 3);
            idle(&mut first, 1);
            idle(&mut replay, 1);
            assert_eq!(first.view(), replay.view());
            let samples: Vec<_> = first
                .view()
                .projectiles
                .iter()
                .zip(&positions)
                .map(|(after, before)| {
                    let delta = after.position - before.position;
                    let angle = delta.y.atan2(delta.x).to_degrees();
                    let speed = delta.x.hypot(delta.y) * 60.0 / 2.5;
                    assert!((-20.0..=20.0).contains(&angle));
                    assert!((0.85..=1.15).contains(&speed));
                    (angle, speed)
                })
                .collect();
            has_one_sided_volley |= samples.iter().all(|(angle, _)| *angle > 0.0)
                || samples.iter().all(|(angle, _)| *angle < 0.0);
            has_different_speeds |= samples
                .windows(2)
                .any(|pair| (pair[0].1 - pair[1].1).abs() > 0.001);
            has_distinct_angles |= samples.iter().enumerate().all(|(index, sample)| {
                samples[..index]
                    .iter()
                    .all(|older| (older.0 - sample.0).abs() > 0.001)
            });
        }
        assert!(has_one_sided_volley);
        assert!(has_different_speeds);
        assert!(has_distinct_angles);

        let mut active = make_round(42);
        let mut idle_round = make_round(42);
        for round in [&mut active, &mut idle_round] {
            round.tick = 899;
            idle(round, 1);
        }
        active.tanks[0].next_shot = Some(PickupKind::Shotgun);
        fire(&mut active);
        assert_eq!(active.view().projectiles.len(), 3);
        for round in [&mut active, &mut idle_round] {
            round.projectiles.clear();
            round.tick = 1499;
            idle(round, 1);
        }
        assert_eq!(active.view().pickups, idle_round.view().pickups);
    }

    #[test]
    fn shotgun_shares_one_muzzle_sweep() {
        let in_front_of_wall = |wall_contacts| {
            fixture(
                vec![wall((1.0, 0.0), (1.0, 2.0))],
                pose(0.62, 0.5, 0.0),
                pose(3.5, 4.0, 0.0),
                Params {
                    wall_contacts,
                    forward_secs: 3.0,
                    ..Params::default()
                },
            )
        };
        let mut round = in_front_of_wall(10);
        round.tanks[0].next_shot = Some(PickupKind::Shotgun);
        fire(&mut round);
        let first = round.view().projectiles;
        assert_eq!(first.len(), 3);
        idle(&mut round, 1);
        for (next, old) in round.view().projectiles.iter().zip(&first) {
            let delta = next.position - old.position;
            assert!(delta.x < 0.0);
            assert!((delta.y.atan2(-delta.x).to_degrees().abs()) <= 20.0);
            let origin = old.position - delta;
            assert!((origin.x - 0.896).abs() < 1e-9);
            assert!((origin.y - 0.5).abs() < 1e-9);
        }
        assert_eq!(round.view().projectiles.len(), 3);

        let mut blocked = in_front_of_wall(1);
        blocked.tanks[0].next_shot = Some(PickupKind::Shotgun);
        fire(&mut blocked);
        assert!(blocked.view().projectiles.is_empty());
        assert_eq!(blocked.view().tanks[0].next_shot, None);
        assert_eq!(blocked.view().tanks[0].ammo, 5);

        let mut target_in_muzzle = fixture(
            vec![],
            pose(1.0, 1.0, 0.0),
            pose(1.44, 1.0, 0.0),
            Params::default(),
        );
        target_in_muzzle.tanks[0].next_shot = Some(PickupKind::Shotgun);
        fire(&mut target_in_muzzle);
        assert_eq!(target_in_muzzle.view().outcome, RoundOutcome::TargetHit);
        assert!(target_in_muzzle.view().projectiles.is_empty());
        assert_eq!(target_in_muzzle.view().tanks[0].ammo, 5);
    }

    #[test]
    fn laser_uses_only_one_pending_shot() {
        let mut round = fixture(
            vec![],
            pose(1.0, 1.0, 0.0),
            pose(1.0, 7.0, 0.0),
            Params::default(),
        );
        for tick in [900, 1500, 2100, 2700, 3300] {
            round.tick = tick - 1;
            idle(&mut round, 1);
        }
        let positions: Vec<_> = round
            .view()
            .pickups
            .iter()
            .filter(|item| item.kind == PickupKind::Laser)
            .take(2)
            .map(|item| item.position)
            .collect();
        assert_eq!(positions.len(), 2);
        round.tanks[0].ammo = 0;
        let refill_at = round.tick + 6;
        round.tanks[0].refill_at = Some(refill_at);
        for position in positions {
            round.tanks[0].pose.position = position;
            idle(&mut round, 1);
            assert!(round.view().tanks[0].laser_loaded);
            assert!(
                !round
                    .view()
                    .pickups
                    .iter()
                    .any(|item| item.position == position)
            );
        }
        round.tanks[0].pose = pose(1.0, 1.0, 0.0);
        fire(&mut round);
        assert_eq!(round.view().projectiles[0].kind, ProjectileKind::Laser);
        assert_eq!(round.view().tanks[0].ammo, 0);
        assert!(!round.view().tanks[0].laser_loaded);
        fire(&mut round);
        assert_eq!(round.view().projectiles.len(), 1);
        assert_eq!(round.tanks[0].refill_at, Some(refill_at));
        idle(&mut round, 2);
        assert_eq!(round.view().tanks[0].ammo, 1);
        fire(&mut round);
        assert!(
            round
                .view()
                .projectiles
                .iter()
                .any(|shot| shot.kind == ProjectileKind::Shell)
        );
        assert_eq!(round.view().tanks[0].ammo, 0);
    }

    #[test]
    fn laser_trail_starts_outside_the_barrel() {
        let mut round = fixture(
            vec![],
            pose(1.0, 1.0, 0.0),
            pose(1.0, 7.0, 0.0),
            Params::default(),
        );
        round.tanks[0].next_shot = Some(PickupKind::Laser);
        fire(&mut round);
        let view = round.view();
        assert_eq!(view.laser_trails.len(), 1);
        let trail = view.laser_trails[0];
        assert_eq!(trail.start, Vec2::new(1.32, 1.0));
        assert_eq!(trail.expires_at, 13);
        let head = view.projectiles[0].position;
        idle(&mut round, 1);
        assert!((round.view().projectiles[0].position.x - head.x - 15.0 / 60.0).abs() < 1e-9);
        assert_eq!(round.view().tanks[0].ammo, 5);
        assert_eq!(round.view().outcome, RoundOutcome::Running);

        let mut against_wall = fixture(
            vec![wall((1.0, 0.0), (1.0, 2.0))],
            pose(0.63, 0.5, 0.0),
            pose(4.0, 4.0, 0.0),
            Params::default(),
        );
        against_wall.tanks[0].next_shot = Some(PickupKind::Laser);
        fire(&mut against_wall);
        let view = against_wall.view();
        assert_eq!(view.outcome, RoundOutcome::SelfHit);
        assert!((view.laser_trails[0].start.x - 0.93).abs() < 1e-9);
        assert!(view.laser_trails[0].end.x < view.laser_trails[0].start.x);
    }

    #[test]
    fn narrower_laser_and_trail_miss_a_shell_graze() {
        let make_round = || {
            fixture(
                vec![],
                pose(1.0, 1.0, 0.0),
                pose(2.0, 1.205, 0.0),
                Params::default(),
            )
        };
        let mut ordinary = make_round();
        fire(&mut ordinary);
        idle(&mut ordinary, 24);
        assert_eq!(ordinary.view().outcome, RoundOutcome::TargetHit);

        let mut laser = make_round();
        laser.tanks[0].next_shot = Some(PickupKind::Laser);
        fire(&mut laser);
        idle(&mut laser, 24);
        assert_eq!(laser.view().outcome, RoundOutcome::Running);
        assert!(laser.view().projectiles[0].position.x > 2.0);
    }

    #[test]
    fn laser_records_contacts_and_expires_by_segment() {
        let mut ricochets = fixture(
            vec![wall((1.0, 1.0), (1.0, 2.0)), wall((2.0, 1.0), (2.0, 2.0))],
            pose(1.0, 4.0, 0.0),
            pose(2.0, 4.0, 0.0),
            Params {
                forward_secs: 0.15,
                wall_contacts: 10,
                ..Params::default()
            },
        );
        ricochets.projectiles.push(Projectile::new(
            0,
            pose(1.5, 1.5, 0.0),
            ProjectileKind::Laser,
        ));
        idle(&mut ricochets, 5);
        assert_eq!(ricochets.view().projectiles.len(), 1);
        idle(&mut ricochets, 1);
        let complete = ricochets.view();
        assert!(complete.projectiles.is_empty());
        assert!(!complete.laser_trails.is_empty());
        let last = complete.laser_trails.last().unwrap();
        assert!((last.end.x - 1.93).abs() < 1e-9);
        assert_eq!(last.expires_at, 18);

        let mut hit = fixture(
            vec![],
            pose(1.0, 1.0, 0.0),
            pose(1.86, 1.0, 0.0),
            Params::default(),
        );
        hit.tanks[0].next_shot = Some(PickupKind::Laser);
        fire(&mut hit);
        idle(&mut hit, 1);
        let view = hit.view();
        assert_eq!(view.outcome, RoundOutcome::TargetHit);
        assert!(view.projectiles.is_empty());
        assert!((view.laser_trails.last().unwrap().end.x - 1.62).abs() < 1e-9);
    }

    #[test]
    fn laser_contact_can_end_in_a_draw() {
        let mut round = fixture(
            vec![wall((1.0, 0.0), (1.0, 3.0))],
            pose(2.0, 1.32, 0.0),
            pose(2.0, 1.68, 0.0),
            Params::default(),
        );
        round.projectiles.push(Projectile::new(
            0,
            pose(1.4, 1.5, PI),
            ProjectileKind::Laser,
        ));
        idle(&mut round, 4);
        assert_eq!(round.view().outcome, RoundOutcome::Running);
        idle(&mut round, 1);
        assert_eq!(round.view().outcome, RoundOutcome::Draw);
        assert!(round.view().projectiles.is_empty());
    }

    #[test]
    fn lingering_trail_hits_until_its_own_expiry() {
        let forward = Action {
            forward: true,
            ..Action::default()
        };
        let make_round = |trail_secs| {
            fixture(
                vec![],
                pose(1.0, 1.0, 0.0),
                pose(1.0, 7.0, 0.0),
                Params {
                    laser_trail_secs: trail_secs,
                    ..Params::default()
                },
            )
        };
        let mut drives_into_own_trail = make_round(0.2);
        drives_into_own_trail.tanks[0].next_shot = Some(PickupKind::Laser);
        fire(&mut drives_into_own_trail);
        drives_into_own_trail.projectiles.clear();
        for _ in 0..2 {
            drives_into_own_trail.step([forward, Action::default()]);
            assert_eq!(drives_into_own_trail.view().outcome, RoundOutcome::Running);
        }
        drives_into_own_trail.step([forward, Action::default()]);
        assert_eq!(drives_into_own_trail.view().outcome, RoundOutcome::SelfHit);

        for (seconds, ticks) in [(0.2, 12), (0.5, 30)] {
            let mut expires = make_round(seconds);
            expires.tick = 99;
            expires.tanks[0].next_shot = Some(PickupKind::Laser);
            fire(&mut expires);
            idle(&mut expires, 1);
            let [first, second]: [LaserTrail; 2] = expires.view().laser_trails.try_into().unwrap();
            assert_eq!(first.expires_at, 100 + ticks as u64);
            assert_eq!(second.expires_at, 101 + ticks as u64);
            expires.projectiles.clear();
            idle(&mut expires, ticks - 1);
            assert_eq!(expires.view().laser_trails, [second]);
            idle(&mut expires, 1);
            assert!(expires.view().laser_trails.is_empty());
            for _ in 0..3 {
                expires.step([forward, Action::default()]);
            }
            assert_eq!(expires.view().outcome, RoundOutcome::Running);
        }
    }

    #[test]
    fn magazine_refills_on_due_step() {
        let mut round = fixture(
            vec![],
            pose(1.0, 1.0, 0.0),
            pose(1.0, 7.0, 0.0),
            Params::default(),
        );
        for ammo in [4, 3, 2, 1, 0] {
            fire(&mut round);
            assert_eq!(round.view().tanks[0].ammo, ammo);
        }
        fire(&mut round);
        assert_eq!(round.view().projectiles.len(), 5);
        idle(&mut round, 174);
        assert_eq!(round.view().tanks[0].ammo, 0);
        idle(&mut round, 1);
        assert_eq!(round.view().tanks[0].ammo, 1);
        idle(&mut round, 180);
        assert_eq!(round.view().tanks[0].ammo, 2);
        idle(&mut round, 540);
        assert_eq!(round.view().tanks[0].ammo, 5);
        fire(&mut round);
        idle(&mut round, 179);
        assert_eq!(round.view().tanks[0].ammo, 4);
        idle(&mut round, 1);
        assert_eq!(round.view().tanks[0].ammo, 5);
    }

    #[test]
    fn reload_rounds_fractional_ticks_and_keeps_exact_boundaries() {
        for (seconds, ticks) in [(4.15, 249), (0.21, 13)] {
            let mut round = fixture(
                vec![],
                pose(1.0, 1.0, 0.0),
                pose(1.0, 7.0, 0.0),
                Params {
                    reload_secs: seconds,
                    ..Params::default()
                },
            );
            fire(&mut round);
            idle(&mut round, ticks - 1);
            assert_eq!(round.view().tanks[0].ammo, 4, "before {seconds}s");
            idle(&mut round, 1);
            assert_eq!(round.view().tanks[0].ammo, 5, "at {seconds}s");
        }
    }

    #[test]
    fn wall_at_muzzle_counts_as_first_contact() {
        for (limit, expected) in [(5, RoundOutcome::SelfHit), (1, RoundOutcome::Running)] {
            let mut round = fixture(
                vec![wall((1.0, 0.0), (1.0, 1.0))],
                pose(0.73, 0.5, 0.0),
                pose(3.5, 0.5, 0.0),
                Params {
                    wall_contacts: limit,
                    ..Params::default()
                },
            );
            fire(&mut round);
            let view = round.view();
            assert_eq!(view.tanks[0].ammo, 4);
            assert!(view.projectiles.is_empty());
            assert_eq!(view.outcome, expected);
        }
    }

    #[test]
    fn corner_and_swept_hit() {
        let mut round = fixture(
            vec![wall((2.0, 1.0), (2.0, 2.0)), wall((1.0, 2.0), (2.0, 2.0))],
            pose(1.5, 1.5, FRAC_PI_4),
            pose(4.0, 3.0, 0.0),
            Params {
                wall_contacts: 2,
                ..Params::default()
            },
        );
        fire(&mut round);
        idle(&mut round, 6);
        let at_corner = round.view().projectiles[0].position;
        idle(&mut round, 1);
        let after = round.view().projectiles[0].position;
        assert!(after.x < at_corner.x && after.y < at_corner.y);

        // A long sweep crosses the whole hull, with both endpoints outside.
        let mut shot = Projectile::new(0, pose(1.0, 1.0, 0.0), ProjectileKind::Shell);
        let arena = Maze {
            width: 4,
            height: 4,
            walls: vec![],
        };
        let tanks = [pose(1.0, 1.0, 0.0), pose(2.0, 1.0, FRAC_PI_4)];
        assert!(matches!(
            shot.advance(&arena, &tanks, 2.0, 2, |_| {}),
            Some(Impact::Tank(1))
        ));
    }

    #[test]
    fn round_freezes_and_tracks_result() {
        let mut round = fixture(
            vec![],
            pose(1.0, 1.0, 0.0),
            pose(1.58, 1.0, PI),
            Params::default(),
        );
        let shooting = Action {
            fire: true,
            ..Action::default()
        };
        round.step([shooting; 2]);
        let complete = round.view();
        assert_eq!(complete.outcome, RoundOutcome::Draw);
        let tick = complete.tick;
        round.step([shooting; 2]);
        assert_eq!(round.view().tick, tick);
        assert_eq!(round.view().tanks[0].ammo, 4);

        let mut first = Round::new(42, Params::default()).unwrap();
        let mut replay = Round::new(42, Params::default()).unwrap();
        for turn in 0..30 {
            let actions = [
                Action {
                    fire: turn % 6 == 0,
                    turn_right: true,
                    ..Action::default()
                },
                Action::default(),
            ];
            first.step(actions);
            replay.step(actions);
        }
        assert_eq!(first.view(), replay.view());
    }
}
