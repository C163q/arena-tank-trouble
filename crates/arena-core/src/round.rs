use crate::hull::OrientedBox;
use crate::pickups::Pickups;
use crate::projectile::{Impact, Projectile};
use crate::{
    Action, MUZZLE_DISTANCE, Maze, PROJECTILE_RADIUS, Params, Pose, ProjectileKind,
    TICKS_PER_SECOND, Vec2, drive,
};

const MAGAZINE_CAPACITY: u8 = 5;
const PROJECTILE_SPEED_RATIO: f64 = 1.125;
const LASER_SPEED_MULTIPLIER: f64 = 4.0;
const LASER_WALL_CONTACTS: u8 = 5;

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
    pub pickups: &'a [Vec2],
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
    laser_trails: Vec<LaserTrail>,
    outcome: RoundOutcome,
    reload_ticks: u64,
    laser_trail_ticks: u64,
}

struct Tank {
    pose: Pose,
    ammo: u8,
    refill_at: Option<u64>,
    laser_loaded: bool,
}

impl Tank {
    fn refill(&mut self, tick: u64, interval: u64) {
        if self.refill_at == Some(tick) {
            self.ammo += 1;
            self.refill_at = (self.ammo < MAGAZINE_CAPACITY).then_some(tick + interval);
        }
    }

    fn fire(&mut self, owner: usize, tick: u64, interval: u64) -> Option<Projectile> {
        if std::mem::take(&mut self.laser_loaded) {
            return Some(Projectile::new(owner, self.pose, ProjectileKind::Laser));
        }
        if self.ammo == 0 {
            return None;
        }
        self.ammo -= 1;
        self.refill_at.get_or_insert(tick + interval);
        Some(Projectile::new(owner, self.pose, ProjectileKind::Shell))
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
                laser_loaded: false,
            }),
            projectiles: Vec::new(),
            pickups: Pickups::new(seed),
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
            tank.laser_loaded |= collected;
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
            if action.fire
                && let Some(mut bullet) =
                    self.tanks[index].fire(index, self.tick, self.reload_ticks)
                && advance(&mut bullet, MUZZLE_DISTANCE, true)
            {
                self.projectiles.push(bullet);
            }
        }
        let projectile_step =
            PROJECTILE_SPEED_RATIO / self.params.forward_secs / f64::from(TICKS_PER_SECOND);
        self.projectiles.retain_mut(|bullet| {
            let distance = if bullet.kind == ProjectileKind::Laser {
                projectile_step * LASER_SPEED_MULTIPLIER
            } else {
                projectile_step
            };
            advance(bullet, distance, false)
        });
        // Include new segments: their final point can touch both tanks at once.
        for (hit, pose) in hits.iter_mut().zip(poses) {
            let hull = OrientedBox::tank(pose);
            *hit = *hit
                || self.laser_trails.iter().any(|trail| {
                    hull.disc_contact(trail.start, trail.end - trail.start, PROJECTILE_RADIUS)
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
                    laser_loaded: tank.laser_loaded,
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
            pickups: self.pickups.positions(),
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
    fn laser_uses_only_one_pending_shot() {
        let mut round = fixture(
            vec![],
            pose(1.0, 1.0, 0.0),
            pose(1.0, 7.0, 0.0),
            Params::default(),
        );
        round.tick = 899;
        idle(&mut round, 1);
        if round.view().pickups.len() == 1 {
            round.tick = 1499;
            idle(&mut round, 1);
        }
        let positions = round.view().pickups[..2].to_vec();
        round.tanks[0].ammo = 0;
        let refill_at = round.tick + 6;
        round.tanks[0].refill_at = Some(refill_at);
        for position in positions {
            round.tanks[0].pose.position = position;
            idle(&mut round, 1);
            assert!(round.view().tanks[0].laser_loaded);
            assert!(!round.view().pickups.contains(&position));
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
        round.tanks[0].laser_loaded = true;
        fire(&mut round);
        let view = round.view();
        assert_eq!(view.laser_trails.len(), 1);
        let trail = view.laser_trails[0];
        assert_eq!(trail.start, Vec2::new(1.32, 1.0));
        assert_eq!(trail.expires_at, 13);
        let head = view.projectiles[0].position;
        idle(&mut round, 1);
        assert!((round.view().projectiles[0].position.x - head.x - 10.0 / 60.0).abs() < 1e-9);
        assert_eq!(round.view().tanks[0].ammo, 5);
        assert_eq!(round.view().outcome, RoundOutcome::Running);

        let mut against_wall = fixture(
            vec![wall((1.0, 0.0), (1.0, 2.0))],
            pose(0.63, 0.5, 0.0),
            pose(4.0, 4.0, 0.0),
            Params::default(),
        );
        against_wall.tanks[0].laser_loaded = true;
        fire(&mut against_wall);
        let view = against_wall.view();
        assert_eq!(view.outcome, RoundOutcome::SelfHit);
        assert!((view.laser_trails[0].start.x - 0.918).abs() < 1e-9);
        assert!(view.laser_trails[0].end.x < view.laser_trails[0].start.x);
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
        idle(&mut ricochets, 7);
        assert_eq!(ricochets.view().projectiles.len(), 1);
        idle(&mut ricochets, 1);
        let complete = ricochets.view();
        assert!(complete.projectiles.is_empty());
        assert!(!complete.laser_trails.is_empty());
        let last = complete.laser_trails.last().unwrap();
        assert!((last.end.x - 1.918).abs() < 1e-9);
        assert_eq!(last.expires_at, 20);

        let mut hit = fixture(
            vec![],
            pose(1.0, 1.0, 0.0),
            pose(1.86, 1.0, 0.0),
            Params::default(),
        );
        hit.tanks[0].laser_loaded = true;
        fire(&mut hit);
        idle(&mut hit, 1);
        let view = hit.view();
        assert_eq!(view.outcome, RoundOutcome::TargetHit);
        assert!(view.projectiles.is_empty());
        assert!((view.laser_trails.last().unwrap().end.x - 1.608).abs() < 1e-9);
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
        idle(&mut round, 5);
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
        drives_into_own_trail.tanks[0].laser_loaded = true;
        fire(&mut drives_into_own_trail);
        drives_into_own_trail.projectiles.clear();
        drives_into_own_trail.step([forward, Action::default()]);
        assert_eq!(drives_into_own_trail.view().outcome, RoundOutcome::Running);
        drives_into_own_trail.step([forward, Action::default()]);
        assert_eq!(drives_into_own_trail.view().outcome, RoundOutcome::SelfHit);

        for (seconds, ticks) in [(0.2, 12), (0.5, 30)] {
            let mut expires = make_round(seconds);
            expires.tick = 99;
            expires.tanks[0].laser_loaded = true;
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
