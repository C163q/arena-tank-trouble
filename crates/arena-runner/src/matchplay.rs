use arena_core::{
    Elimination, LASER_RADIUS, LaserTrail, MUZZLE_DISTANCE, PICKUP_SIZE, PROJECTILE_RADIUS, Params,
    Pickup, ProjectileView, Round, RoundOutcome, RoundView, TANK_LENGTH, TANK_WIDTH,
    TICKS_PER_SECOND, TankView, WALL_THICKNESS, Wall,
};
use serde::Serialize;

use crate::controller::{Controller, Decision};

pub const DELAY_TICKS: u64 = 12;

#[derive(Serialize)]
pub struct MatchConfig {
    #[serde(skip)]
    pub seed: u64,
    pub max_ticks: u64,
    pub kill_points: u32,
    pub survival_points: u32,
    pub params: Params,
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self {
            seed: 407,
            max_ticks: 60 * u64::from(TICKS_PER_SECOND),
            kill_points: 1,
            survival_points: 1,
            params: Params::default(),
        }
    }
}

#[derive(Serialize)]
pub struct Map {
    pub width: u8,
    pub height: u8,
    pub walls: Vec<Wall>,
}

impl Map {
    fn from_view(view: &RoundView<'_>) -> Self {
        Self {
            width: view.maze.width,
            height: view.maze.height,
            walls: view.maze.walls.clone(),
        }
    }
}

#[derive(Serialize)]
pub struct Geometry {
    tank_length: f64,
    tank_width: f64,
    muzzle_distance: f64,
    projectile_radius: f64,
    wall_thickness: f64,
    pickup_size: f64,
    laser_radius: f64,
}

pub const GEOMETRY: Geometry = Geometry {
    tank_length: TANK_LENGTH,
    tank_width: TANK_WIDTH,
    muzzle_distance: MUZZLE_DISTANCE,
    projectile_radius: PROJECTILE_RADIUS,
    wall_thickness: WALL_THICKNESS,
    pickup_size: PICKUP_SIZE,
    laser_radius: LASER_RADIUS,
};

#[derive(Serialize)]
pub struct Frame {
    pub tick: u64,
    pub tanks: Vec<TankView>,
    pub projectiles: Vec<ProjectileView>,
    pub pickups: Vec<Pickup>,
    pub laser_trails: Vec<LaserTrail>,
    pub eliminations: Vec<Elimination>,
    pub victory_at: Option<u64>,
    pub decisions: Vec<Decision>,
}

impl Frame {
    fn from_view(view: RoundView<'_>, decisions: Vec<Decision>) -> Self {
        Self {
            tick: view.tick,
            tanks: view.tanks,
            projectiles: view.projectiles,
            pickups: view.pickups.to_vec(),
            laser_trails: view.laser_trails.to_vec(),
            eliminations: view.eliminations.to_vec(),
            victory_at: view.victory_at,
            decisions,
        }
    }
}

#[derive(Serialize)]
pub struct ObservedTank<'a> {
    pub seat: usize,
    #[serde(flatten)]
    pub tank: &'a TankView,
}

#[derive(Serialize)]
pub struct Observation<'a> {
    pub tick: u64,
    pub world_tick: u64,
    pub seat: usize,
    pub decision_budget_ms: u32,
    pub ticks_per_second: u32,
    pub max_ticks: u64,
    pub kill_points: u32,
    pub survival_points: u32,
    pub map: &'a Map,
    pub geometry: &'a Geometry,
    pub params: &'a Params,
    pub own_tank: &'a TankView,
    pub other_tanks: Vec<ObservedTank<'a>>,
    pub projectiles: &'a [ProjectileView],
    pub laser_trails: &'a [LaserTrail],
    pub pickups: &'a [Pickup],
}

fn observe<'a>(
    config: &'a MatchConfig,
    map: &'a Map,
    frames: &'a [Frame],
    seat: usize,
) -> Observation<'a> {
    let now = frames.last().expect("initial frame");
    let past = &frames[now.tick.saturating_sub(DELAY_TICKS) as usize];
    Observation {
        tick: now.tick,
        world_tick: past.tick,
        seat,
        decision_budget_ms: if now.tick == 0 { 100 } else { 30 },
        ticks_per_second: TICKS_PER_SECOND,
        max_ticks: config.max_ticks,
        kill_points: config.kill_points,
        survival_points: config.survival_points,
        map,
        geometry: &GEOMETRY,
        params: &config.params,
        own_tank: &now.tanks[seat],
        other_tanks: past
            .tanks
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != seat)
            .map(|(seat, tank)| ObservedTank { seat, tank })
            .collect(),
        projectiles: &past.projectiles,
        laser_trails: &past.laser_trails,
        pickups: &past.pickups,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Score {
    pub kill_points: u64,
    pub survival_points: u64,
}

fn score_step(
    scores: &mut [Score],
    config: &MatchConfig,
    eliminations: &[Elimination],
    outcome: RoundOutcome,
) {
    for event in eliminations {
        if event.tank != event.owner {
            scores[event.owner].kill_points += u64::from(config.kill_points);
        }
    }
    if let RoundOutcome::Winner(seat) = outcome {
        scores[seat].survival_points = u64::from(config.survival_points);
    }
}

#[derive(Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum ResultKind {
    Winner { seat: usize },
    AllEliminated,
    TimeLimit,
}

#[derive(Serialize)]
pub struct Replay {
    pub format_version: u32,
    pub seed: String,
    pub ticks_per_second: u32,
    pub observation_delay_ticks: u64,
    pub config: MatchConfig,
    pub geometry: Geometry,
    pub map: Map,
    pub seats: Vec<String>,
    pub frames: Vec<Frame>,
    pub scores: Vec<Score>,
    pub result: ResultKind,
}

pub fn run(config: MatchConfig, controllers: &mut [Box<dyn Controller>]) -> Result<Replay, String> {
    let mut game = Round::with_players(config.seed, config.params, controllers.len())
        .map_err(|field| format!("invalid match parameter: {field}"))?;
    let initial = game.view();
    let map = Map::from_view(&initial);
    let seats = controllers
        .iter()
        .map(|bot| bot.name().to_owned())
        .collect();
    let mut frames = vec![Frame::from_view(initial, vec![])];
    let mut scores: Vec<_> = (0..controllers.len()).map(|_| Score::default()).collect();

    let result = loop {
        let now = frames.last().expect("initial frame exists");
        if now.tick >= config.max_ticks && now.victory_at.is_none() {
            break ResultKind::TimeLimit;
        }
        let decisions: Vec<_> = controllers
            .iter_mut()
            .enumerate()
            .map(|(seat, bot)| {
                if now.tanks[seat].alive {
                    bot.decide(&observe(&config, &map, &frames, seat))
                } else {
                    Decision::eliminated()
                }
            })
            .collect();
        let actions: Vec<_> = decisions.iter().map(|decision| decision.action).collect();
        game.step(&actions);
        let view = game.view();
        for event in view.eliminations {
            controllers[event.tank].eliminate();
        }
        let outcome = view.outcome;
        score_step(&mut scores, &config, view.eliminations, outcome);
        frames.push(Frame::from_view(view, decisions));
        match outcome {
            RoundOutcome::Running => {}
            RoundOutcome::Winner(seat) => break ResultKind::Winner { seat },
            RoundOutcome::Draw => break ResultKind::AllEliminated,
        }
    };

    Ok(Replay {
        format_version: 1,
        seed: config.seed.to_string(),
        ticks_per_second: TICKS_PER_SECOND,
        observation_delay_ticks: DELAY_TICKS,
        config,
        geometry: GEOMETRY,
        map,
        seats,
        frames,
        scores,
        result,
    })
}

#[cfg(test)]
#[path = "matchplay_tests.rs"]
mod tests;
