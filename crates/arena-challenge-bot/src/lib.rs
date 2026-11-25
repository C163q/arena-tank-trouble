mod ballistics;
mod navigation;

use arena_core::{Action, Maze, Params, ProjectileView, TankView, Vec2};
use serde::Deserialize;
use std::cell::RefCell;
use std::f64::consts::{PI, TAU};

#[derive(Deserialize)]
struct Observation {
    tick: u64,
    world_tick: u64,
    ticks_per_second: u32,
    seat: usize,
    own_tank: TankView,
    other_tanks: Vec<TankView>,
    projectiles: Vec<ProjectileView>,
    map: Maze,
    params: Params,
}

fn length(v: Vec2) -> f64 {
    v.x.hypot(v.y)
}

fn difference(target: f64, facing: f64) -> f64 {
    (target - facing + PI).rem_euclid(TAU) - PI
}

impl Observation {
    fn step(&self) -> f64 {
        TAU / (self.params.turn_secs * f64::from(self.ticks_per_second))
    }

    fn steer(&self, target: f64) -> Action {
        let error = difference(target, self.own_tank.pose.angle);
        Action {
            turn_left: error < -self.step() * 0.5,
            turn_right: error > self.step() * 0.5,
            ..Action::default()
        }
    }
}

#[derive(Default)]
struct Memory {
    previous_tick: Option<u64>,
    enemies: Vec<Vec2>,
    last_fire: Option<u64>,
}

impl Memory {
    fn decide(&mut self, world: &Observation) -> Action {
        let age = world.tick.saturating_sub(world.world_tick);
        let track = self
            .previous_tick
            .is_some_and(|t| t.checked_add(1) == Some(world.world_tick));
        if world.tick == 0 {
            *self = Self::default();
        }
        let targets: Vec<_> = world
            .other_tanks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.alive)
            .map(|(i, tank)| {
                let position = tank.pose.position;
                let advance = self
                    .enemies
                    .get(i)
                    .filter(|_| track)
                    .map_or(Vec2::new(0., 0.), |old| {
                        (position - *old).scale(age as f64 + 5.0)
                    });
                let fraction = world
                    .map
                    .first_wall_contact(position, position + advance, 0.2)
                    .unwrap_or(1.);
                position + advance.scale(fraction)
            })
            .collect();
        self.enemies = world.other_tanks.iter().map(|t| t.pose.position).collect();
        self.previous_tick = Some(world.world_tick);

        let mut planned = navigation::hold_center(world, &targets);
        if let Some((angle, ready)) = ballistics::firing_solution(world, &targets) {
            planned = world.steer(angle);
            planned.fire = ready
                && self
                    .last_fire
                    .is_none_or(|t| world.tick.saturating_sub(t) >= 8);
        }
        let action = ballistics::evade(world, planned);
        if action.fire {
            self.last_fire = Some(world.tick);
        }
        action
    }
}

thread_local! { static MEMORY: RefCell<Memory> = RefCell::default(); }

#[unsafe(no_mangle)]
pub extern "C" fn arena_bot_abi_version() -> u32 {
    1
}

/// # Safety
/// The caller supplies a readable buffer of `len` bytes for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn arena_bot_decide(observation: *const u8, len: usize) -> u32 {
    if observation.is_null() || len > isize::MAX as usize {
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(observation, len) };
    let Ok(world) = serde_json::from_slice::<Observation>(bytes) else {
        return 0;
    };
    if !world.own_tank.alive
        || world.ticks_per_second == 0
        || world.params.validate().is_err()
        || world.map.width == 0
        || world.map.height == 0
    {
        return 0;
    }
    let a = MEMORY.with_borrow_mut(|m| m.decide(&world));
    u32::from(a.forward)
        | u32::from(a.reverse) << 1
        | u32::from(a.turn_left) << 2
        | u32::from(a.turn_right) << 3
        | u32::from(a.fire) << 4
}
