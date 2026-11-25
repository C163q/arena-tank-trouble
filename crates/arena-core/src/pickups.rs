use rand::{Rng, SeedableRng, seq::SliceRandom};
use rand_chacha::ChaCha8Rng;

use crate::collision::Bounds;
use crate::hull::OrientedBox;
use crate::{Maze, Pose, TICKS_PER_SECOND, Vec2};

pub const PICKUP_SIZE: f64 = 0.32;
const FIRST_SPAWN: u64 = 15 * TICKS_PER_SECOND as u64;
const SPAWN_INTERVAL: u64 = 10 * TICKS_PER_SECOND as u64;
const LIMIT: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PickupKind {
    Laser,
    Shotgun,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Pickup {
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub position: Vec2,
    pub kind: PickupKind,
}

pub(super) struct Pickups {
    items: Vec<Pickup>,
    position_rng: ChaCha8Rng,
    kind_rng: ChaCha8Rng,
}

impl Pickups {
    pub fn new(seed: u64) -> Self {
        Self {
            items: Vec::with_capacity(LIMIT),
            position_rng: ChaCha8Rng::seed_from_u64(seed ^ 0x9E37_79B9_7F4A_7C15),
            kind_rng: ChaCha8Rng::seed_from_u64(seed ^ 0xD1B5_4A32_D192_ED03),
        }
    }

    pub fn collect(&mut self, poses: [Pose; 2]) -> [Option<PickupKind>; 2] {
        let hulls = poses.map(OrientedBox::tank);
        let mut collected = [None; 2];
        self.items.retain(|&Pickup { position, kind }| {
            let square = pickup_square(position);
            let distance = |index: usize| {
                let offset = position - poses[index].position;
                offset.dot(offset)
            };
            let winner = (0..2)
                .filter(|&index| hulls[index].overlaps(square))
                .min_by(|&a, &b| distance(a).total_cmp(&distance(b)).then(a.cmp(&b)));
            if let Some(index) = winner {
                collected[index] = Some(kind);
            }
            winner.is_none()
        });
        collected
    }

    pub fn spawn_due(&mut self, tick: u64, maze: &Maze, poses: [Pose; 2]) {
        let due = tick
            .checked_sub(FIRST_SPAWN)
            .is_some_and(|elapsed| elapsed.is_multiple_of(SPAWN_INTERVAL));
        let free_slots = LIMIT - self.items.len();
        if !due || free_slots == 0 {
            return;
        }
        let hulls = poses.map(OrientedBox::tank);
        let available: Vec<_> = (0..maze.height)
            .flat_map(|y| {
                (0..maze.width).map(move |x| Vec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5))
            })
            .filter(|&position| {
                !self.items.iter().any(|item| item.position == position)
                    && hulls
                        .iter()
                        .all(|hull| !hull.overlaps(pickup_square(position)))
            })
            .collect();
        let count = self
            .position_rng
            .gen_range(1..=2)
            .min(free_slots)
            .min(available.len());
        for &position in available.choose_multiple(&mut self.position_rng, count) {
            let kind = if self.kind_rng.gen_bool(0.5) {
                PickupKind::Laser
            } else {
                PickupKind::Shotgun
            };
            self.items.push(Pickup { position, kind });
        }
    }

    pub fn items(&self) -> &[Pickup] {
        &self.items
    }
}

fn pickup_square(center: Vec2) -> OrientedBox {
    let half = Vec2::new(PICKUP_SIZE / 2.0, PICKUP_SIZE / 2.0);
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
        assert!(first.items().is_empty());
        first.spawn_due(900, &maze, poses);
        replay.spawn_due(900, &maze, poses);
        assert!((1..=2).contains(&first.items().len()));
        assert_eq!(first.items(), replay.items());
        for (index, item) in first.items().iter().enumerate() {
            let position = item.position;
            assert_eq!(position.x.fract(), 0.5);
            assert_eq!(position.y.fract(), 0.5);
            assert!(poses.iter().all(|pose| pose.position != position));
            assert!(
                !first.items()[..index]
                    .iter()
                    .any(|other| other.position == position)
            );
        }

        let mut capped = Pickups::new(1);
        capped.items = (0..4)
            .map(|x| Pickup {
                position: Vec2::new(f64::from(x) + 0.5, 0.5),
                kind: PickupKind::Laser,
            })
            .collect();
        capped.spawn_due(900, &maze, poses);
        assert_eq!(capped.items().len(), 5);
        capped.spawn_due(1500, &maze, poses);
        assert_eq!(capped.items().len(), 5);
        capped.items.truncate(3);
        capped.spawn_due(1501, &maze, poses);
        assert_eq!(capped.items().len(), 3);
        capped.spawn_due(2100, &maze, poses);
        assert!((4..=5).contains(&capped.items().len()));
    }

    #[test]
    fn mixed_pickups_share_the_cap() {
        let (maze, poses) = Maze::generate(42);
        let mut items = Pickups::new(42);
        let mut last_count = 0;
        for tick in [900, 1500, 2100, 2700, 3300] {
            items.spawn_due(tick, &maze, poses);
            let total = items.items().len();
            assert!(total <= 5);
            assert!((last_count..=last_count + 2).contains(&total));
            assert!(items.items().iter().enumerate().all(|(index, item)| {
                items.items()[..index]
                    .iter()
                    .all(|older| older.position != item.position)
            }));
            last_count = total;
        }
        assert_eq!(last_count, 5);
        assert!(
            items
                .items()
                .iter()
                .any(|item| item.kind == PickupKind::Laser)
        );
        assert!(
            items
                .items()
                .iter()
                .any(|item| item.kind == PickupKind::Shotgun)
        );
    }

    #[test]
    fn square_pickup_requires_area_overlap() {
        let mut pickups = Pickups::new(0);
        let center = Vec2::new(2.5, 2.5);
        let item = Pickup {
            position: center,
            kind: PickupKind::Shotgun,
        };
        let far = pose(7.5, 7.5, 0.0);
        for (x, angle) in [(2.13, 0.0), (2.34 - (0.21 + 0.17) / SQRT_2, FRAC_PI_4)] {
            pickups.items.push(item);
            assert_eq!(pickups.collect([pose(x, 2.5, angle), far]), [None, None]);
            assert_eq!(pickups.items(), [item]);
            assert_eq!(
                pickups.collect([pose(x + 0.01, 2.5, angle), far]),
                [Some(PickupKind::Shotgun), None]
            );
            assert!(pickups.items().is_empty());
        }
        pickups.items.push(item);
        assert_eq!(
            pickups.collect([pose(2.16, 2.5, 0.0), pose(2.81, 2.5, 0.0)]),
            [None, Some(PickupKind::Shotgun)]
        );
        pickups.items.push(item);
        assert_eq!(
            pickups.collect([pose(2.1875, 2.5, 0.0), pose(2.8125, 2.5, 0.0)]),
            [Some(PickupKind::Shotgun), None]
        );
    }
}
