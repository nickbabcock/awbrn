pub use awbrn_game::world::{
    FogActive, FogOfWarMap, FogOfWarState, FriendlyFactions, TerrainFogProperties,
};
use bevy::prelude::*;

pub struct FogPlugin;

impl Plugin for FogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FogOfWarMap>()
            .init_resource::<FogActive>()
            .init_resource::<FriendlyFactions>();
    }
}
