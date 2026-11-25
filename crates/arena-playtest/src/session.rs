use arena_core::{
    Action, LASER_RADIUS, MUZZLE_DISTANCE, PICKUP_SIZE, PROJECTILE_RADIUS, Params, Round,
    RoundOutcome, RoundView, TANK_LENGTH, TANK_WIDTH, WALL_THICKNESS,
};
use rand::{RngCore, SeedableRng, rngs::StdRng};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Command {
    Input {
        round: u64,
        forward: bool,
        reverse: bool,
        turn_left: bool,
        turn_right: bool,
    },
    Fire {
        round: u64,
    },
    Restart {
        round: u64,
        mode: RestartMode,
        params: Params,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum RestartMode {
    Same,
    New,
}

pub struct Session {
    game: Round,
    generation: u64,
    seeds: StdRng,
    seed: u64,
    action: Action,
    outcome: RoundOutcome,
    target_hits: u64,
    self_hits: u64,
}

impl Session {
    pub fn new(initial_seed: u64) -> Self {
        Self {
            game: Round::new(initial_seed, Params::default()).expect("valid default parameters"),
            generation: 0,
            seeds: StdRng::seed_from_u64(initial_seed),
            seed: initial_seed,
            action: Action::default(),
            outcome: RoundOutcome::Running,
            target_hits: 0,
            self_hits: 0,
        }
    }

    pub fn receive(&mut self, text: &str) -> Option<Value> {
        let command = match serde_json::from_str(text) {
            Ok(command) => command,
            Err(_) => return Some(error("无法识别这条操作请求")),
        };
        match command {
            Command::Input {
                round,
                forward,
                reverse,
                turn_left,
                turn_right,
            } if round == self.generation => {
                self.action.forward = forward;
                self.action.reverse = reverse;
                self.action.turn_left = turn_left;
                self.action.turn_right = turn_right;
            }
            Command::Fire { round } if round == self.generation => {
                self.action.fire = true;
            }
            Command::Restart {
                round,
                mode,
                params,
            } if round == self.generation => {
                return Some(self.restart(mode, params));
            }
            _ => {}
        }
        None
    }

    fn restart(&mut self, mode: RestartMode, params: Params) -> Value {
        if let Err(field) = params.validate() {
            let label = match field {
                "forward_secs" => "前进时间",
                "reverse_secs" => "后退时间",
                "turn_secs" => "转向时间",
                "reload_secs" => "补弹间隔",
                "wall_contacts" => "触墙上限",
                "laser_trail_secs" => "激光轨迹保留",
                _ => "参数",
            };
            return error(&format!("{label}超出允许范围"));
        }
        let seed = match mode {
            RestartMode::Same => self.seed,
            RestartMode::New => loop {
                let candidate = self.seeds.next_u64();
                if candidate != self.seed {
                    break candidate;
                }
            },
        };
        self.game = Round::new(seed, params).expect("validated parameters");
        self.seed = seed;
        self.generation += 1;
        self.action = Action::default();
        self.outcome = RoundOutcome::Running;
        if matches!(mode, RestartMode::New) {
            self.target_hits = 0;
            self.self_hits = 0;
        }
        self.state()
    }

    pub fn tick(&mut self) -> Value {
        self.game.step([self.action, Action::default()]);
        self.action.fire = false;
        let view = self.game.view();
        if self.outcome == RoundOutcome::Running {
            match view.outcome {
                RoundOutcome::TargetHit => self.target_hits += 1,
                RoundOutcome::SelfHit => self.self_hits += 1,
                _ => {}
            }
            self.outcome = view.outcome;
        }
        self.snapshot(view)
    }

    pub fn state(&self) -> Value {
        self.snapshot(self.game.view())
    }

    fn snapshot(&self, view: RoundView<'_>) -> Value {
        json!({
            "type": "state",
            "round": self.generation,
            "tick": view.tick,
            "seed": view.seed.to_string(),
            "geometry": {
                "tank_length": TANK_LENGTH,
                "tank_width": TANK_WIDTH,
                "muzzle_distance": MUZZLE_DISTANCE,
                "projectile_radius": PROJECTILE_RADIUS,
                "wall_thickness": WALL_THICKNESS,
                "laser_pickup_size": PICKUP_SIZE,
                "pickup_size": PICKUP_SIZE,
                "laser_radius": LASER_RADIUS,
            },
            "width": view.maze.width,
            "height": view.maze.height,
            "walls": view.maze.walls,
            "tanks": view.tanks,
            "projectiles": view.projectiles,
            "pickups": view.pickups,
            "laser_trails": view.laser_trails,
            "params": view.params,
            "outcome": view.outcome,
            "victory_at": view.victory_at,
            "target_hits": self.target_hits,
            "self_hits": self.self_hits,
        })
    }
}

fn error(message: &str) -> Value {
    json!({"type": "error", "message": message})
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn restart(round: u64, mode: &str, reload_secs: f64) -> String {
        json!({
            "type": "restart",
            "round": round,
            "mode": mode,
            "params": {
                "forward_secs": 0.45,
                "reverse_secs": 0.65,
                "turn_secs": 1.5,
                "reload_secs": reload_secs,
                "wall_contacts": 5,
            }
        })
        .to_string()
    }

    fn input(round: u64, moving: bool) -> String {
        json!({
            "type": "input",
            "round": round,
            "forward": moving,
            "reverse": false,
            "turn_left": false,
            "turn_right": false,
        })
        .to_string()
    }

    fn fire(round: u64) -> String {
        json!({"type": "fire", "round": round}).to_string()
    }

    #[test]
    fn same_and_new_restart() {
        let mut game = Session::new(42);
        let initial = game.state();
        game.receive(&input(0, true));
        game.tick();
        game.receive(&fire(0));
        let before = game.tick();
        assert_ne!(before["tanks"][0]["pose"], initial["tanks"][0]["pose"]);
        assert_eq!(before["tanks"][0]["ammo"], 4);

        let same = game.receive(&restart(0, "same", 2.0)).unwrap();
        for field in ["seed", "walls", "tanks", "target_hits", "self_hits"] {
            assert_eq!(same[field], initial[field]);
        }
        assert_eq!(same["tick"], 0);
        assert_eq!(same["round"], 1);
        assert_eq!(same["params"]["reload_secs"], 2.0);
        assert_eq!(same["projectiles"], json!([]));
        let new = game.receive(&restart(1, "new", 2.0)).unwrap();
        assert_ne!(new["seed"], initial["seed"]);
        assert_eq!(new["target_hits"], 0);
        assert_eq!(new["self_hits"], 0);
    }

    #[test]
    fn scores_once_and_keeps_score_only_on_same_map() {
        let mut game = Session::new(407);
        game.receive(&fire(0));
        let pending = (0..95)
            .map(|_| game.tick())
            .find(|snapshot| !snapshot["victory_at"].is_null())
            .expect("static target destroyed");
        let deadline = pending["tick"].as_u64().unwrap() + 120;
        assert_eq!(pending["victory_at"], deadline);
        assert_eq!(pending["tanks"][1]["alive"], false);
        for _ in 0..119 {
            let state = game.tick();
            assert_eq!(state["outcome"], "running");
            assert_eq!(state["target_hits"], 0);
        }
        let complete = game.tick();
        assert_eq!(complete["tick"], deadline);
        assert!(complete["victory_at"].is_null());
        assert_eq!(complete["outcome"], "target_hit");
        assert_eq!(complete["target_hits"], 1);
        assert_eq!(game.tick(), complete);
        let same = game.receive(&restart(0, "same", 1.0)).unwrap();
        assert_eq!(same["target_hits"], 1);
        assert_eq!(same["outcome"], "running");
        assert!(same["victory_at"].is_null());
        assert_eq!(same["tanks"][1]["alive"], true);
        let new = game.receive(&restart(1, "new", 1.0)).unwrap();
        assert_eq!(new["target_hits"], 0);
    }

    #[test]
    fn restart_validates_and_resets_laser_state() {
        let mut game = Session::new(26532);
        let legacy = game.receive(&restart(0, "same", 3.0)).unwrap();
        assert_eq!(legacy["params"]["laser_trail_secs"], 0.20);
        assert_eq!(legacy["geometry"]["pickup_size"], 0.32);
        for _ in 0..900 {
            game.game.step([Action::default(); 2]);
        }
        let spawned = game.state();
        let pickups = spawned["pickups"].as_array().unwrap();
        assert_eq!(pickups.len(), 2);
        assert!(
            pickups
                .iter()
                .all(|item| item["kind"] == "laser" && item["x"].as_f64().is_some())
        );
        game.receive(&input(1, true));
        for _ in 0..45 {
            game.tick();
        }
        game.receive(&input(1, false));
        assert_eq!(game.state()["tanks"][0]["laser_loaded"], true);
        assert_eq!(game.state()["tanks"][0]["next_shot"], "laser");
        game.receive(&fire(1));
        let before = game.tick();
        assert!(!before["laser_trails"].as_array().unwrap().is_empty());

        let mut request: Value = serde_json::from_str(&restart(1, "same", 3.0)).unwrap();
        request["params"]["laser_trail_secs"] = json!(0.04);
        assert_eq!(game.receive(&request.to_string()).unwrap()["type"], "error");
        assert_eq!(game.state(), before);

        request["params"]["laser_trail_secs"] = json!(0.50);
        let after = game.receive(&request.to_string()).unwrap();
        assert_eq!(after["params"]["laser_trail_secs"], 0.50);
        assert_eq!(after["seed"], before["seed"]);
        assert_eq!(after["tick"], 0);
        assert_eq!(after["pickups"], json!([]));
        assert_eq!(after["laser_trails"], json!([]));
        assert_eq!(after["tanks"][0]["laser_loaded"], false);
        assert_eq!(after["tanks"][1]["laser_loaded"], false);
        for tank in after["tanks"].as_array().unwrap() {
            assert_eq!(tank.get("next_shot"), Some(&Value::Null));
        }
    }

    #[test]
    fn invalid_restart_keeps_current_round() {
        let mut game = Session::new(42);
        game.receive(&fire(0));
        let before = game.tick();
        let error = game.receive(&restart(0, "new", 0.0)).unwrap();
        assert_eq!(error["type"], "error");
        assert_eq!(game.state(), before);
        let malformed = game.receive("{").unwrap();
        assert_eq!(malformed["type"], "error");
        assert_eq!(game.state(), before);
    }

    #[test]
    fn stale_round_input_is_ignored() {
        let mut game = Session::new(0);
        game.receive(&input(0, true));
        game.receive(&fire(0));
        let fresh = game.receive(&restart(0, "same", 1.0)).unwrap();
        for message in [input(0, true), fire(0), restart(0, "new", 1.0)] {
            assert_eq!(game.receive(&message), None);
        }
        let next = game.tick();
        for field in ["round", "seed", "tanks", "projectiles"] {
            assert_eq!(next[field], fresh[field]);
        }
    }

    #[test]
    fn sessions_are_independent() {
        let mut first = Session::new(42);
        let mut second = Session::new(42);
        first.receive(&fire(0));
        assert_eq!(first.tick()["tanks"][0]["ammo"], 4);
        assert_eq!(second.tick()["tanks"][0]["ammo"], 5);
        first.receive(&restart(0, "new", 2.0)).unwrap();
        assert_eq!(second.state()["seed"], "42");
        assert_eq!(second.state()["params"]["reload_secs"], 3.0);
    }
}
