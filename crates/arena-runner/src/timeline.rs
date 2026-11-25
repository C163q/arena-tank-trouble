use std::io::{self, Write};

use arena_core::Action;
use serde_json::{Value, json};

use crate::matchplay::{Frame, Replay};

pub fn write(mut output: impl Write, replay: &Replay) -> io::Result<()> {
    write_line(
        &mut output,
        &json!({
            "type": "arena_timeline",
            "format_version": replay.format_version,
            "seed": replay.seed,
            "ticks_per_second": replay.ticks_per_second,
            "observation_delay_ticks": replay.observation_delay_ticks,
            "config": replay.config,
            "geometry": replay.geometry,
            "map": replay.map,
            "seats": replay.seats,
            "keys": {"W": "forward", "S": "reverse", "A": "turn_left", "D": "turn_right", "F": "fire", "-": "wait"},
            "columns": {
                "tanks": ["x", "y", "angle", "alive", "ammo", "refill_at", "next_shot"],
                "projectiles": ["owner", "kind", "x", "y", "vx", "vy", "wall_contacts"],
                "pickups": ["kind", "x", "y"],
                "laser_trails": ["owner", "start_x", "start_y", "end_x", "end_y", "expires_at"],
                "eliminations": ["tank", "owner"],
                "decisions": ["keys", "status", "diagnostic"]
            }
        }),
    )?;
    for frame in &replay.frames {
        write_line(&mut output, &frame_record(frame))?;
    }
    write_line(
        &mut output,
        &json!({
            "type": "result",
            "tick": replay.frames.last().expect("initial frame").tick,
            "result": replay.result,
            "scores": replay.scores
        }),
    )?;
    output.flush()
}

fn write_line(output: &mut impl Write, record: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *output, record)?;
    output.write_all(b"\n")
}

fn frame_record(frame: &Frame) -> Value {
    let tanks: Vec<_> = frame
        .tanks
        .iter()
        .map(|tank| {
            (
                tank.pose.position.x,
                tank.pose.position.y,
                tank.pose.angle,
                tank.alive,
                tank.ammo,
                tank.refill_at,
                tank.next_shot,
            )
        })
        .collect();
    let projectiles: Vec<_> = frame
        .projectiles
        .iter()
        .map(|shot| {
            (
                shot.owner,
                shot.kind,
                shot.position.x,
                shot.position.y,
                shot.velocity.x,
                shot.velocity.y,
                shot.wall_contacts,
            )
        })
        .collect();
    let pickups: Vec<_> = frame
        .pickups
        .iter()
        .map(|pickup| (pickup.kind, pickup.position.x, pickup.position.y))
        .collect();
    let laser_trails: Vec<_> = frame
        .laser_trails
        .iter()
        .map(|trail| {
            (
                trail.owner,
                trail.start.x,
                trail.start.y,
                trail.end.x,
                trail.end.y,
                trail.expires_at,
            )
        })
        .collect();
    let eliminations: Vec<_> = frame
        .eliminations
        .iter()
        .map(|event| (event.tank, event.owner))
        .collect();
    let decisions: Vec<_> = frame
        .decisions
        .iter()
        .map(|decision| {
            (
                keys(decision.action),
                &decision.status,
                &decision.diagnostic,
            )
        })
        .collect();
    json!({
        "tick": frame.tick,
        "tanks": tanks,
        "projectiles": projectiles,
        "pickups": pickups,
        "laser_trails": laser_trails,
        "eliminations": eliminations,
        "decisions": decisions,
        "victory_at": frame.victory_at
    })
}

fn keys(action: Action) -> String {
    let keys: String = [
        (action.forward, 'W'),
        (action.reverse, 'S'),
        (action.turn_left, 'A'),
        (action.turn_right, 'D'),
        (action.fire, 'F'),
    ]
    .into_iter()
    .filter_map(|(pressed, key)| pressed.then_some(key))
    .collect();
    if keys.is_empty() { "-".into() } else { keys }
}
