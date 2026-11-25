use crate::projectile::{Impact, Projectile};
use crate::{Action, Maze, Params, Pose, TICKS_PER_SECOND, Vec2, drive};

const MAGAZINE_CAPACITY: u8 = 5;
const PROJECTILE_STEP: f64 = 9.0 / TICKS_PER_SECOND as f64;
const MUZZLE_DISTANCE: f64 = 0.28;

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
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ProjectileView {
    pub position: Vec2,
}

#[derive(Debug, PartialEq)]
pub struct RoundView<'a> {
    pub seed: u64,
    pub tick: u64,
    pub maze: &'a Maze,
    pub params: &'a Params,
    pub tanks: [TankView; 2],
    pub projectiles: Vec<ProjectileView>,
    pub outcome: RoundOutcome,
}

pub struct Round {
    seed: u64,
    tick: u64,
    maze: Maze,
    params: Params,
    tanks: [Tank; 2],
    projectiles: Vec<Projectile>,
    outcome: RoundOutcome,
    reload_ticks: u64,
}

struct Tank {
    pose: Pose,
    ammo: u8,
    refill_at: Option<u64>,
}

impl Tank {
    fn refill(&mut self, tick: u64, interval: u64) {
        if self.refill_at == Some(tick) {
            self.ammo += 1;
            self.refill_at = (self.ammo < MAGAZINE_CAPACITY).then_some(tick + interval);
        }
    }

    fn fire(&mut self, owner: usize, tick: u64, interval: u64) -> Option<Projectile> {
        if self.ammo == 0 {
            return None;
        }
        self.ammo -= 1;
        self.refill_at.get_or_insert(tick + interval);
        Some(Projectile::new(owner, self.pose))
    }
}

impl Round {
    pub fn new(seed: u64, params: Params) -> Result<Self, &'static str> {
        params.validate()?;
        let (maze, spawns) = Maze::generate(seed);
        Ok(Self::from_maze(seed, params, maze, spawns))
    }

    fn from_maze(seed: u64, params: Params, maze: Maze, spawns: [Pose; 2]) -> Self {
        let exact_ticks = params.reload_secs * f64::from(TICKS_PER_SECOND);
        // Keep a whole tick whole when a decimal setting rounds slightly up.
        let reload_ticks = (exact_ticks - exact_ticks * f64::EPSILON).ceil() as u64;
        Self {
            seed,
            tick: 0,
            maze,
            params,
            tanks: spawns.map(|pose| Tank {
                pose,
                ammo: MAGAZINE_CAPACITY,
                refill_at: None,
            }),
            projectiles: Vec::new(),
            outcome: RoundOutcome::Running,
            reload_ticks,
        }
    }

    pub fn step(&mut self, actions: [Action; 2]) {
        if self.outcome != RoundOutcome::Running {
            return;
        }
        self.tick += 1;
        for (index, action) in actions.into_iter().enumerate() {
            let other = self.tanks[1 - index].pose;
            let tank = &mut self.tanks[index];
            tank.refill(self.tick, self.reload_ticks);
            tank.pose = drive(&self.maze, tank.pose, other, action, &self.params);
        }
        let poses = [self.tanks[0].pose, self.tanks[1].pose];
        let mut hits = [false; 2];
        let mut advance = |bullet: &mut Projectile, distance| match bullet.advance(
            &self.maze,
            &poses,
            distance,
            self.params.wall_contacts,
        ) {
            None => true,
            Some(Impact::Tank(index)) => {
                hits[index] = true;
                false
            }
            Some(Impact::Expired) => false,
        };
        for (index, action) in actions.into_iter().enumerate() {
            if action.fire
                && let Some(mut bullet) =
                    self.tanks[index].fire(index, self.tick, self.reload_ticks)
                && advance(&mut bullet, MUZZLE_DISTANCE)
            {
                self.projectiles.push(bullet);
            }
        }
        self.projectiles
            .retain_mut(|bullet| advance(bullet, PROJECTILE_STEP));
        self.outcome = match hits {
            [false, false] => RoundOutcome::Running,
            [false, true] => RoundOutcome::TargetHit,
            [true, false] => RoundOutcome::SelfHit,
            [true, true] => RoundOutcome::Draw,
        };
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
                }
            }),
            projectiles: self
                .projectiles
                .iter()
                .map(|bullet| ProjectileView {
                    position: bullet.position,
                })
                .collect(),
            outcome: self.outcome,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_4, PI};

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
        idle(&mut round, 54);
        assert_eq!(round.view().tanks[0].ammo, 0);
        idle(&mut round, 1);
        assert_eq!(round.view().tanks[0].ammo, 1);
        idle(&mut round, 60);
        assert_eq!(round.view().tanks[0].ammo, 2);
        idle(&mut round, 240);
        assert_eq!(round.view().tanks[0].ammo, 5);
        fire(&mut round);
        idle(&mut round, 59);
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
        idle(&mut round, 1);
        let at_corner = round.view().projectiles[0].position;
        idle(&mut round, 1);
        let after = round.view().projectiles[0].position;
        assert!(after.x < at_corner.x && after.y < at_corner.y);

        // The trajectory clips the target between two consecutive samples.
        let mut round = fixture(
            vec![],
            pose(1.0, 1.0, 0.0),
            pose(1.355, 1.26, 0.0),
            Params::default(),
        );
        fire(&mut round);
        assert_eq!(round.view().outcome, RoundOutcome::TargetHit);
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
