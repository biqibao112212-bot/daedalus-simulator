mod camera;
mod chassis_observation;
mod debug;
mod frequency;
mod input;
mod projectile;
mod range_control;
mod uav;
pub use camera::*;
pub use chassis_observation::*;
pub use debug::*;
pub use frequency::*;
pub use input::*;
pub use projectile::*;
pub use range_control::*;
pub use uav::*;

use bevy::prelude::*;

#[derive(SystemSet, Clone, PartialEq, Eq, Hash, Debug)]
pub enum GameplaySystems {
    Input,
    GameLogic,
    Camera,
    Cleanup,
}
