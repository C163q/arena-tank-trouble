use serde_json::{Value, json};

use crate::{arena_bot_abi_version, arena_bot_decide};

fn observation() -> Value {
    json!({
        "tick": 24, "world_tick": 12, "seat": 0, "ticks_per_second": 60,
        "params": arena_core::Params::default(),
        "map": {"width": 4, "height": 4, "walls": []},
        "own_tank": {
            "pose": {"position": {"x": 1.5, "y": 1.5}, "angle": 0.0},
            "alive": true, "ammo": 2, "refill_at": null,
            "laser_loaded": false, "next_shot": null
        },
        "other_tanks": [{
            "seat": 1,
            "pose": {"position": {"x": 3.5, "y": 1.5}, "angle": 0.0},
            "alive": true, "ammo": 2, "refill_at": null,
            "laser_loaded": false, "next_shot": null
        }],
        "pickups": [], "projectiles": [], "laser_trails": []
    })
}

fn decide(world: &Value) -> u32 {
    let bytes = serde_json::to_vec(world).unwrap();
    unsafe { arena_bot_decide(bytes.as_ptr(), bytes.len()) }
}

#[test]
fn exports_abi_and_waits_on_invalid_json() {
    assert_eq!(arena_bot_abi_version(), 1);
    assert_eq!(decide(&json!({})), 0);
    assert_eq!(unsafe { arena_bot_decide(std::ptr::null(), 0) }, 0);
}

#[test]
fn fires_at_a_visible_enemy_with_a_loaded_pickup_and_an_empty_magazine() {
    let mut world = observation();
    world["own_tank"]["ammo"] = json!(0);
    world["own_tank"]["next_shot"] = json!("laser");
    assert_eq!(decide(&world), 16);
    world["seat"] = json!(3);
    world["other_tanks"][0]["seat"] = json!(2);
    assert_eq!(decide(&world), 16);
}

#[test]
fn follows_the_open_route_around_a_wall() {
    let mut world = observation();
    world["map"]["walls"] = json!([{
        "a": {"x": 2.0, "y": 0.0}, "b": {"x": 2.0, "y": 2.0}
    }]);
    assert_eq!(decide(&world), 8);
}

#[test]
fn heads_for_a_nearby_pickup_when_the_target_is_out_of_sight() {
    let mut world = observation();
    world["map"]["walls"] = json!([{
        "a": {"x": 2.0, "y": 0.0}, "b": {"x": 2.0, "y": 2.0}
    }]);
    world["pickups"] = json!([{"x": 0.5, "y": 1.5, "kind": "laser"}]);
    world["own_tank"]["pose"]["angle"] = json!(std::f64::consts::PI);
    assert_eq!(decide(&world), 1);
}

#[test]
fn moves_out_of_a_delayed_shell_path_even_without_a_live_target() {
    let mut world = observation();
    world["other_tanks"][0]["alive"] = json!(false);
    world["projectiles"] = json!([{
        "position": {"x": 1.5, "y": 0.5},
        "velocity": {"x": 0.0, "y": 2.5},
        "kind": "shell", "owner": 0, "wall_contacts": 0
    }]);
    assert_eq!(decide(&world), 1);
}
