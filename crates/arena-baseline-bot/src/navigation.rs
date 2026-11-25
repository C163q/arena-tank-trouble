use std::collections::VecDeque;

use arena_core::{Action, Maze, Vec2};

use crate::{Observation, tank_radius};

#[derive(Clone, Copy)]
struct Route {
    steps: usize,
    next: usize,
}

pub(crate) fn navigate(world: &Observation) -> Option<Action> {
    let map = &world.map;
    let own = world.own_tank.pose.position;
    let start = cell(map, own)?;
    let routes = routes_from(map, start);
    let route_to = |position| {
        let route = routes[cell(map, position)?]?;
        Some((route, position))
    };

    // A short detour can earn a free shot. Drop collected pickups immediately
    // using the current own tank, even while they are still in the delayed view.
    let pickup = world
        .pickups
        .iter()
        .filter(|_| world.own_tank.next_shot.is_none())
        .filter_map(|pickup| route_to(pickup.position))
        .filter(|(route, _)| route.steps <= 3)
        .min_by_key(|(route, _)| route.steps);
    let (route, goal) = pickup.or_else(|| {
        world
            .other_tanks
            .iter()
            .filter(|tank| tank.alive)
            .filter_map(|tank| route_to(tank.pose.position))
            .min_by_key(|(route, _)| route.steps)
    })?;
    let mut waypoint = if route.steps == 0 {
        goal
    } else {
        center(map, route.next)
    };
    // Move toward the current cell center before cutting a tight corner.
    if map
        .first_wall_contact(own, waypoint, tank_radius() + 0.02)
        .is_some()
    {
        waypoint = center(map, start);
    }
    let offset = waypoint - own;
    let (mut action, remaining_turn) = world.steer(offset.y.atan2(offset.x));
    if offset.dot(offset) > 0.05_f64.powi(2) {
        action.forward = remaining_turn.abs() < 0.10;
    } else {
        action = Action::default();
    }
    Some(action)
}

fn cell(map: &Maze, position: Vec2) -> Option<usize> {
    (position.x >= 0.0
        && position.x < f64::from(map.width)
        && position.y >= 0.0
        && position.y < f64::from(map.height))
    .then(|| position.y as usize * usize::from(map.width) + position.x as usize)
}

fn center(map: &Maze, cell: usize) -> Vec2 {
    let width = usize::from(map.width);
    Vec2::new((cell % width) as f64 + 0.5, (cell / width) as f64 + 0.5)
}

fn routes_from(map: &Maze, start: usize) -> Vec<Option<Route>> {
    let width = usize::from(map.width);
    let count = width * usize::from(map.height);
    let mut routes = vec![None; count];
    routes[start] = Some(Route {
        steps: 0,
        next: start,
    });
    let mut queue = VecDeque::from([start]);
    while let Some(from) = queue.pop_front() {
        let route = routes[from].expect("queued cells have routes");
        let neighbors = [
            (from % width > 0).then(|| from - 1),
            (from % width + 1 < width).then_some(from + 1),
            from.checked_sub(width),
            (from + width < count).then_some(from + width),
        ];
        for to in neighbors.into_iter().flatten() {
            if routes[to].is_none() && edge_is_open(map, from, to) {
                routes[to] = Some(Route {
                    steps: route.steps + 1,
                    next: if from == start { to } else { route.next },
                });
                queue.push_back(to);
            }
        }
    }
    routes
}

fn edge_is_open(map: &Maze, a: usize, b: usize) -> bool {
    let midpoint = (center(map, a) + center(map, b)).scale(0.5);
    !map.walls.iter().any(|wall| {
        (wall.a.x.min(wall.b.x)..=wall.a.x.max(wall.b.x)).contains(&midpoint.x)
            && (wall.a.y.min(wall.b.y)..=wall.a.y.max(wall.b.y)).contains(&midpoint.y)
    })
}
