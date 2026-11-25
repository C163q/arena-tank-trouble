use crate::{Maze, TICKS_PER_SECOND, Vec2, collision::Bounds, hull::OrientedBox};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ZoneSchedule {
    pub start_tick: u64,
    pub end_tick: u64,
}

pub const ZONE_SCHEDULE: ZoneSchedule = ZoneSchedule {
    start_tick: 30 * TICKS_PER_SECOND as u64,
    end_tick: 90 * TICKS_PER_SECOND as u64,
};

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SafeZone {
    pub min: Vec2,
    pub max: Vec2,
}

impl SafeZone {
    pub(super) fn at_tick(maze: &Maze, tick: u64) -> Option<Self> {
        let elapsed = tick.checked_sub(ZONE_SCHEDULE.start_tick)?;
        let duration = ZONE_SCHEDULE.end_tick - ZONE_SCHEDULE.start_tick;
        let center = Vec2::new(f64::from(maze.width), f64::from(maze.height)).scale(0.5);
        let half = center.scale(duration.saturating_sub(elapsed) as f64 / duration as f64);
        Some(Self {
            min: center - half,
            max: center + half,
        })
    }

    pub(super) fn contains(self, hull: OrientedBox) -> bool {
        hull.within(Bounds {
            min: self.min,
            max: self.max,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_starts_at_the_map_edges_and_shrinks_linearly_to_the_center() {
        let maze = Maze {
            width: 12,
            height: 8,
            walls: vec![],
        };
        assert_eq!(SafeZone::at_tick(&maze, 1799), None);
        for (tick, min, max) in [
            (1800, Vec2::new(0.0, 0.0), Vec2::new(12.0, 8.0)),
            (3600, Vec2::new(3.0, 2.0), Vec2::new(9.0, 6.0)),
            (5400, Vec2::new(6.0, 4.0), Vec2::new(6.0, 4.0)),
            (5500, Vec2::new(6.0, 4.0), Vec2::new(6.0, 4.0)),
        ] {
            assert_eq!(SafeZone::at_tick(&maze, tick), Some(SafeZone { min, max }));
        }
    }
}
