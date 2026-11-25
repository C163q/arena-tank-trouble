use rand::{Rng, SeedableRng, seq::SliceRandom};
use rand_chacha::ChaCha8Rng;

use crate::collision::Bounds;
use crate::hull::OrientedBox;
use crate::{Maze, Pose, TICKS_PER_SECOND, Vec2};

pub const LASER_PICKUP_SIZE: f64 = 0.32;
const FIRST_SPAWN: u64 = 15 * TICKS_PER_SECOND as u64;
const SPAWN_INTERVAL: u64 = 10 * TICKS_PER_SECOND as u64;
const LIMIT: usize = 5;

pub(super) struct Pickups {
    positions: Vec<Vec2>,
    rng: ChaCha8Rng,
}

impl Pickups {
    pub fn new(seed: u64) -> Self {
        Self {
            positions: Vec::with_capacity(LIMIT),
            rng: ChaCha8Rng::seed_from_u64(seed ^ 0x9E37_79B9_7F4A_7C15),
        }
    }

    pub fn collect(&mut self, poses: [Pose; 2]) -> [bool; 2] {
        let hulls = poses.map(OrientedBox::tank);
        let mut collected = [false; 2];
        self.positions.retain(|&position| {
            let square = pickup_square(position);
            let distance = |index: usize| {
                let offset = position - poses[index].position;
                offset.dot(offset)
            };
            let winner = (0..2)
                .filter(|&index| hulls[index].overlaps(square))
                .min_by(|&a, &b| distance(a).total_cmp(&distance(b)).then(a.cmp(&b)));
            if let Some(index) = winner {
                collected[index] = true;
            }
            winner.is_none()
        });
        collected
    }

    pub fn spawn_due(&mut self, tick: u64, maze: &Maze, poses: [Pose; 2]) {
        let due = tick
            .checked_sub(FIRST_SPAWN)
            .is_some_and(|elapsed| elapsed.is_multiple_of(SPAWN_INTERVAL));
        let free_slots = LIMIT - self.positions.len();
        if !due || free_slots == 0 {
            return;
        }
        let hulls = poses.map(OrientedBox::tank);
        let available: Vec<_> = (0..maze.height)
            .flat_map(|y| {
                (0..maze.width).map(move |x| Vec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5))
            })
            .filter(|&position| {
                !self.positions.contains(&position)
                    && hulls
                        .iter()
                        .all(|hull| !hull.overlaps(pickup_square(position)))
            })
            .collect();
        let count = self
            .rng
            .gen_range(1..=2)
            .min(free_slots)
            .min(available.len());
        self.positions
            .extend(available.choose_multiple(&mut self.rng, count).copied());
    }

    pub fn positions(&self) -> &[Vec2] {
        &self.positions
    }
}

fn pickup_square(center: Vec2) -> OrientedBox {
    let half = Vec2::new(LASER_PICKUP_SIZE / 2.0, LASER_PICKUP_SIZE / 2.0);
    OrientedBox::axis_aligned(Bounds {
        min: center - half,
        max: center + half,
    })
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_4, SQRT_2};

    use super::*;

    fn pose(x: f64, y: f64, angle: f64) -> Pose {
        Pose {
            position: Vec2::new(x, y),
            angle,
        }
    }

    #[test]
    fn spawn_caps_batches_and_preserves_cadence() {
        let (maze, poses) = Maze::generate(42);
        let mut first = Pickups::new(42);
        let mut replay = Pickups::new(42);
        first.spawn_due(899, &maze, poses);
        assert!(first.positions().is_empty());
        first.spawn_due(900, &maze, poses);
        replay.spawn_due(900, &maze, poses);
        assert!((1..=2).contains(&first.positions().len()));
        assert_eq!(first.positions(), replay.positions());
        for (index, position) in first.positions().iter().enumerate() {
            assert_eq!(position.x.fract(), 0.5);
            assert_eq!(position.y.fract(), 0.5);
            assert!(poses.iter().all(|pose| pose.position != *position));
            assert!(!first.positions()[..index].contains(position));
        }

        let mut capped = Pickups::new(1);
        capped.positions = (0..4).map(|x| Vec2::new(f64::from(x) + 0.5, 0.5)).collect();
        capped.spawn_due(900, &maze, poses);
        assert_eq!(capped.positions().len(), 5);
        capped.spawn_due(1500, &maze, poses);
        assert_eq!(capped.positions().len(), 5);
        capped.positions.truncate(3);
        capped.spawn_due(1501, &maze, poses);
        assert_eq!(capped.positions().len(), 3);
        capped.spawn_due(2100, &maze, poses);
        assert!((4..=5).contains(&capped.positions().len()));
    }

    #[test]
    fn square_pickup_requires_area_overlap() {
        let mut pickups = Pickups::new(0);
        let center = Vec2::new(2.5, 2.5);
        let far = pose(7.5, 7.5, 0.0);
        for (x, angle) in [(2.13, 0.0), (2.34 - (0.21 + 0.17) / SQRT_2, FRAC_PI_4)] {
            pickups.positions.push(center);
            assert_eq!(pickups.collect([pose(x, 2.5, angle), far]), [false, false]);
            assert_eq!(pickups.positions(), [center]);
            assert_eq!(
                pickups.collect([pose(x + 0.01, 2.5, angle), far]),
                [true, false]
            );
            assert!(pickups.positions().is_empty());
        }
        pickups.positions.push(center);
        assert_eq!(
            pickups.collect([pose(2.16, 2.5, 0.0), pose(2.81, 2.5, 0.0)]),
            [false, true]
        );
        pickups.positions.push(center);
        assert_eq!(
            pickups.collect([pose(2.1875, 2.5, 0.0), pose(2.8125, 2.5, 0.0)]),
            [true, false]
        );
    }
}
