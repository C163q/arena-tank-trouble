use std::collections::VecDeque;
use std::f64::consts::FRAC_PI_2;

use rand::{Rng, SeedableRng, seq::SliceRandom};
use rand_chacha::ChaCha8Rng;

use crate::Vec2;

const WIDTH: usize = 12;
const HEIGHT: usize = 8;
const CELLS: usize = WIDTH * HEIGHT;
type Edge = ((usize, usize), (usize, usize));

fn neighbors(cell: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (cell % WIDTH, cell / WIDTH);
    [
        x.checked_sub(1).map(|x| y * WIDTH + x),
        (x + 1 < WIDTH).then_some(cell + 1),
        y.checked_sub(1).map(|y| y * WIDTH + x),
        (y + 1 < HEIGHT).then_some(cell + WIDTH),
    ]
    .into_iter()
    .flatten()
}

fn edge_between(a: usize, b: usize) -> Edge {
    let (a, b) = (a.min(b), a.max(b));
    let (x, y) = (b % WIDTH, b / WIDTH);
    if b - a == 1 {
        ((x, y), (x, y + 1))
    } else {
        ((x, y), (x + 1, y))
    }
}

fn cell_distances(start: usize, is_open: impl Fn(Edge) -> bool) -> [usize; CELLS] {
    let mut distances = [usize::MAX; CELLS];
    distances[start] = 0;
    let mut queue = VecDeque::from([start]);
    while let Some(cell) = queue.pop_front() {
        for next in neighbors(cell) {
            if distances[next] == usize::MAX && is_open(edge_between(cell, next)) {
                distances[next] = distances[cell] + 1;
                queue.push_back(next);
            }
        }
    }
    distances
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Pose {
    pub position: Vec2,
    pub angle: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Wall {
    pub a: Vec2,
    pub b: Vec2,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Maze {
    pub width: u8,
    pub height: u8,
    pub walls: Vec<Wall>,
}

impl Maze {
    pub fn generate(seed: u64) -> (Self, [Pose; 2]) {
        let (maze, spawns) = Self::generate_for_players(seed, 2);
        (maze, spawns.try_into().expect("two spawns"))
    }

    pub(super) fn generate_for_players(seed: u64, players: usize) -> (Self, Vec<Pose>) {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut open = std::collections::BTreeSet::new();
        let mut visited = [false; CELLS];
        let mut stack = vec![0];
        visited[0] = true;
        while let Some(&cell) = stack.last() {
            let candidates: Vec<_> = neighbors(cell).filter(|&next| !visited[next]).collect();
            if let Some(&next) = candidates.choose(&mut rng) {
                open.insert(edge_between(cell, next));
                visited[next] = true;
                stack.push(next);
            } else {
                stack.pop();
            }
        }

        let vertical = (0..HEIGHT).flat_map(|y| (0..=WIDTH).map(move |x| ((x, y), (x, y + 1))));
        let horizontal = (0..=HEIGHT).flat_map(|y| (0..WIDTH).map(move |x| ((x, y), (x + 1, y))));
        let all_edges: Vec<_> = vertical.chain(horizontal).collect();
        let mut closed_inner: Vec<_> = (0..CELLS)
            .flat_map(|cell| {
                neighbors(cell)
                    .filter(move |&next| next > cell)
                    .map(move |next| edge_between(cell, next))
            })
            .filter(|edge| !open.contains(edge))
            .collect();
        closed_inner.shuffle(&mut rng);
        open.extend(closed_inner.into_iter().take(10));

        let start = rng.gen_range(0..CELLS);
        let distances = cell_distances(start, |edge| open.contains(&edge));
        let targets: Vec<_> = (0..CELLS).filter(|&cell| distances[cell] >= 4).collect();
        let target = *targets.choose(&mut rng).expect("maze has distant cells");
        let spawn = |cell, rng: &mut ChaCha8Rng| Pose {
            position: Vec2::new((cell % WIDTH) as f64 + 0.5, (cell / WIDTH) as f64 + 0.5),
            angle: f64::from(rng.gen_range(0..4)) * FRAC_PI_2,
        };
        let mut spawns = vec![spawn(start, &mut rng), spawn(target, &mut rng)];
        let mut distances_from_spawns = vec![
            distances,
            cell_distances(target, |edge| open.contains(&edge)),
        ];
        while spawns.len() < players {
            let candidates: Vec<_> = (0..CELLS)
                .filter(|&cell| distances_from_spawns.iter().all(|d| d[cell] >= 4))
                .collect();
            let cell = *candidates.choose(&mut rng).expect("maze has distant cells");
            spawns.push(spawn(cell, &mut rng));
            distances_from_spawns.push(cell_distances(cell, |edge| open.contains(&edge)));
        }
        let walls = all_edges
            .into_iter()
            .filter(|edge| !open.contains(edge))
            .map(|(a, b)| Wall {
                a: Vec2::new(a.0 as f64, a.1 as f64),
                b: Vec2::new(b.0 as f64, b.1 as f64),
            })
            .collect();
        (
            Self {
                width: WIDTH as u8,
                height: HEIGHT as u8,
                walls,
            },
            spawns,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, VecDeque};

    use super::*;

    #[test]
    fn extra_seats_preserve_the_map_and_existing_spawns() {
        for seed in 0..32 {
            let (first, pair) = Maze::generate(seed);
            let (second, trio) = Maze::generate_for_players(seed, 3);
            let (third, full) = Maze::generate_for_players(seed, 4);
            assert_eq!(first, second);
            assert_eq!(second, third);
            assert_eq!(pair, trio[..2]);
            assert_eq!(trio, full[..3]);
            assert_eq!(full, Maze::generate_for_players(seed, 4).1);
            for (index, spawn) in full.iter().enumerate() {
                assert!(
                    full[..index]
                        .iter()
                        .all(|other| other.position != spawn.position)
                );
            }
        }
    }

    #[test]
    fn seed_reproduces_connected_maze() {
        for seed in [0, 1, 42] {
            let (maze, spawns) = Maze::generate(seed);
            assert_eq!((maze.clone(), spawns), Maze::generate(seed));
            assert_ne!(maze, Maze::generate(seed + 1).0);
            assert_eq!((maze.width, maze.height), (12, 8));

            let cell = |pose: Pose| {
                assert_eq!(pose.position.x.fract(), 0.5);
                assert_eq!(pose.position.y.fract(), 0.5);
                assert!((0.0..12.0).contains(&pose.position.x));
                assert!((0.0..8.0).contains(&pose.position.y));
                (pose.position.x as i32, pose.position.y as i32)
            };
            let start = cell(spawns[0]);
            let target = cell(spawns[1]);
            let mut visited = HashMap::from([(start, 0)]);
            let mut queue = VecDeque::from([start]);

            while let Some((x, y)) = queue.pop_front() {
                let distance = visited[&(x, y)];
                for (next, a, b) in [
                    ((x - 1, y), (x, y), (x, y + 1)),
                    ((x + 1, y), (x + 1, y), (x + 1, y + 1)),
                    ((x, y - 1), (x, y), (x + 1, y)),
                    ((x, y + 1), (x, y + 1), (x + 1, y + 1)),
                ] {
                    let wall = Wall {
                        a: Vec2::new(f64::from(a.0), f64::from(a.1)),
                        b: Vec2::new(f64::from(b.0), f64::from(b.1)),
                    };
                    if !maze.walls.contains(&wall) && !visited.contains_key(&next) {
                        assert!((0..12).contains(&next.0) && (0..8).contains(&next.1));
                        visited.insert(next, distance + 1);
                        queue.push_back(next);
                    }
                }
            }
            assert_eq!(visited.len(), 96);
            assert!(visited[&target] >= 4);
        }
    }
}
