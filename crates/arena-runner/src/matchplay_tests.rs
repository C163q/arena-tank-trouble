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
            Elimination { tank: 1, owner: 1 },
            Elimination { tank: 0, owner: 3 },
        ],
        RoundOutcome::Running,
    );
    assert_eq!(scores[1], Score::default());
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
        &[Elimination { tank: 3, owner: 0 }],
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
fn default_points_reward_last_survivor_twice_as_much_as_a_kill() {
    let config = MatchConfig::default();
    let mut scores = [Score::default(); 2];
    score_step(
        &mut scores,
        &config,
        &[Elimination { tank: 1, owner: 0 }],
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
