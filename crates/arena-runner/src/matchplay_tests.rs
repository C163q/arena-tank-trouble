use arena_core::{Action, Elimination, Params, Round, RoundOutcome};

use super::*;

#[test]
fn observation_delays_every_dynamic_object_except_own_tank() {
    let mut game = Round::with_players(407, Params::default(), 3).unwrap();
    let initial = game.view();
    let map = Map::from_view(&initial);
    let config = MatchConfig::default();
    let mut history = vec![Frame::from_view(initial, vec![])];
    for tick in 1_u64..=13 {
        game.step([
            Action {
                forward: true,
                fire: tick == 1,
                ..Action::default()
            },
            Action {
                turn_left: true,
                ..Action::default()
            },
            Action::default(),
        ]);
        history.push(Frame::from_view(game.view(), vec![]));
        let view = observe(&config, &map, &history, 0);
        let past = tick.saturating_sub(12);
        assert_eq!(view.tick, tick);
        assert_eq!(view.world_tick, past);
        assert_eq!(view.max_ticks, config.max_ticks);
        assert_eq!(view.kill_points, config.kill_points);
        assert_eq!(view.survival_points, config.survival_points);
        assert_eq!(view.own_tank, &history[tick as usize].tanks[0]);
        assert_eq!(view.other_tanks[0].tank, &history[past as usize].tanks[1]);
        assert_eq!(view.projectiles, history[past as usize].projectiles);
        assert_eq!(view.pickups, history[past as usize].pickups);
        assert_eq!(view.laser_trails, history[past as usize].laser_trails);
        if tick == 1 {
            assert_eq!(view.own_tank.ammo, 4);
            assert!(view.projectiles.is_empty());
        }
        if tick == 13 {
            assert_eq!(view.projectiles[0].owner, 0);
        }
    }
}

#[test]
fn kills_and_final_survival_are_scored_separately() {
    let config = MatchConfig {
        kill_points: 2,
        survival_points: 3,
        ..MatchConfig::default()
    };
    let mut scores = vec![Score::default(); 4];
    score_step(
        &mut scores,
        &config,
        &[
            Elimination {
                tank: 1,
                owner: Some(1),
            },
            Elimination {
                tank: 0,
                owner: Some(3),
            },
            Elimination {
                tank: 2,
                owner: None,
            },
        ],
        RoundOutcome::Running,
    );
    assert_eq!(scores[1], Score::default());
    assert_eq!(scores[2], Score::default());
    assert_eq!(
        scores[3],
        Score {
            kill_points: 2,
            survival_points: 0
        }
    );
    score_step(
        &mut scores,
        &config,
        &[Elimination {
            tank: 3,
            owner: Some(0),
        }],
        RoundOutcome::Draw,
    );
    assert_eq!(scores[0].kill_points, 2);
    assert!(scores.iter().all(|score| score.survival_points == 0));
    score_step(&mut scores, &config, &[], RoundOutcome::Winner(2));
    assert_eq!(
        scores[2],
        Score {
            kill_points: 0,
            survival_points: 3
        }
    );
}

#[test]
fn the_zone_ends_an_idle_match_and_matches_the_delayed_observation() {
    use arena_core::ZONE_SCHEDULE;

    struct Idle;

    impl Controller for Idle {
        fn name(&self) -> &str {
            "idle"
        }

        fn decide(&mut self, observation: &Observation<'_>) -> Decision {
            assert_eq!(observation.zone_schedule, ZONE_SCHEDULE);
            if observation.world_tick < ZONE_SCHEDULE.start_tick {
                assert_eq!(observation.safe_zone, None);
            } else {
                let zone = observation.safe_zone.unwrap();
                let duration = ZONE_SCHEDULE.end_tick - ZONE_SCHEDULE.start_tick;
                let elapsed = observation.world_tick - ZONE_SCHEDULE.start_tick;
                let expected_x =
                    f64::from(observation.map.width) * elapsed as f64 / (2.0 * duration as f64);
                assert!((zone.min.x - expected_x).abs() < 1e-9);
            }
            Decision::ok(Action::default())
        }
    }

    let config = MatchConfig::default();
    assert_eq!(config.max_ticks, ZONE_SCHEDULE.end_tick);
    let replay = run(config, &mut [Box::new(Idle), Box::new(Idle)]).unwrap();
    assert!(!matches!(replay.result, ResultKind::TimeLimit));
    assert!(replay.frames.last().unwrap().tick > ZONE_SCHEDULE.start_tick + DELAY_TICKS);
    let deaths: Vec<_> = replay
        .frames
        .iter()
        .flat_map(|frame| &frame.eliminations)
        .collect();
    assert!(!deaths.is_empty());
    assert!(deaths.iter().all(|event| event.owner.is_none()));
    assert!(replay.scores.iter().all(|score| score.kill_points == 0));
}

#[test]
fn default_points_reward_last_survivor_twice_as_much_as_a_kill() {
    let config = MatchConfig::default();
    let mut scores = [Score::default(); 2];
    score_step(
        &mut scores,
        &config,
        &[Elimination {
            tank: 1,
            owner: Some(0),
        }],
        RoundOutcome::Winner(0),
    );
    assert_eq!(
        scores[0],
        Score {
            kill_points: 1,
            survival_points: 2
        }
    );
    assert_eq!(scores[1], Score::default());
}
