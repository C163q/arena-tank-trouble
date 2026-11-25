mod combat;
mod navigation;

#[cfg(test)]
mod tests;

use std::f64::consts::{PI, TAU};

use arena_core::{Action, Maze, Params, Pickup, ProjectileView, TANK_LENGTH, TANK_WIDTH, TankView};
use serde::Deserialize;

#[derive(Deserialize)]
struct Observation {
    tick: u64,
    world_tick: u64,
    ticks_per_second: u32,
    params: Params,
    map: Maze,
    own_tank: TankView,
    other_tanks: Vec<TankView>,
    projectiles: Vec<ProjectileView>,
    pickups: Vec<Pickup>,
}

impl Observation {
    fn decide(&self) -> Action {
        let action = combat::aim(self)
            .or_else(|| navigation::navigate(self))
            .unwrap_or_default();
        combat::dodge(self, action)
    }

    fn turn_step(&self) -> f64 {
        TAU / (self.params.turn_secs * f64::from(self.ticks_per_second))
    }

    fn steer(&self, angle: f64) -> (Action, f64) {
        let offset = (angle - self.own_tank.pose.angle + PI).rem_euclid(TAU) - PI;
        let step = self.turn_step();
        let action = Action {
            turn_left: offset < -step / 2.0,
            turn_right: offset > step / 2.0,
            ..Action::default()
        };
        (action, offset - self.turn_delta(action))
    }

    fn turn_delta(&self, action: Action) -> f64 {
        self.turn_step() * f64::from(i8::from(action.turn_right) - i8::from(action.turn_left))
    }
}

fn tank_radius() -> f64 {
    TANK_LENGTH.hypot(TANK_WIDTH) / 2.0
}

#[unsafe(no_mangle)]
pub extern "C" fn arena_bot_abi_version() -> u32 {
    1
}

/// # Safety
/// A non-null `observation` must point to `len` initialized bytes in one allocation.
/// The buffer must remain readable and unmodified during this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn arena_bot_decide(observation: *const u8, len: usize) -> u32 {
    if observation.is_null() || len > isize::MAX as usize {
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(observation, len) };
    let Ok(world) = serde_json::from_slice::<Observation>(bytes) else {
        return 0;
    };
    if !world.own_tank.alive || world.ticks_per_second == 0 || world.params.validate().is_err() {
        return 0;
    }
    let action = world.decide();
    // Keep the bit order in sync with include/arena_bot.h.
    u32::from(action.forward)
        | (u32::from(action.reverse) << 1)
        | (u32::from(action.turn_left) << 2)
        | (u32::from(action.turn_right) << 3)
        | (u32::from(action.fire) << 4)
}
