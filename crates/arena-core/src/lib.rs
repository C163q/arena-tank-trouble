mod collision;
mod hull;
mod maze;
mod motion;
mod pickups;
mod projectile;
mod round;
mod zone;

pub use maze::{Maze, Pose, Wall};
pub use motion::{Action, Params, drive};
pub use pickups::PICKUP_SIZE as LASER_PICKUP_SIZE;
pub use pickups::{PICKUP_SIZE, Pickup, PickupKind};
pub use projectile::ProjectileKind;
pub use round::{
    Elimination, LaserTrail, ProjectileView, Round, RoundOutcome, RoundView, TankView,
};
pub use zone::{SafeZone, ZONE_SCHEDULE, ZoneSchedule};

pub const TICKS_PER_SECOND: u32 = 60;
pub const WALL_THICKNESS: f64 = 0.08;
pub const TANK_LENGTH: f64 = 0.42;
pub const TANK_WIDTH: f64 = 0.34;
pub const MUZZLE_DISTANCE: f64 = 0.32;
pub const PROJECTILE_RADIUS: f64 = 0.042;
pub const LASER_RADIUS: f64 = 0.030;

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

impl Vec2 {
    pub fn from_angle(angle: f64) -> Self {
        let (y, x) = angle.sin_cos();
        Self::new(x, y)
    }

    pub fn scale(self, factor: f64) -> Self {
        Self::new(self.x * factor, self.y * factor)
    }

    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y
    }
}

impl std::ops::Add for Vec2 {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }
}
