use crate::render::animation::UnitPathAnimation;
use awbrn_game::MapPosition;
use awbrn_game::world::{CarriedBy, Faction, Hiding, Unit};
pub use awbrn_game::world::{
    FogActive, FogOfWarMap, FogOfWarState, FriendlyFactions, TerrainFogProperties,
};
use awbrn_types::UnitDomain;
use bevy::prelude::*;

type FogUnitFilter = (With<Unit>, Without<CarriedBy>, Without<UnitPathAnimation>);

/// Sets `Visibility` on non-animating units based on the fog map.
/// Units with `UnitPathAnimation` are excluded — their visibility is handled
/// per-tile in `animate_unit_paths`.
pub fn apply_fog_to_units(
    fog_map: Res<FogOfWarMap>,
    fog_active: Res<FogActive>,
    friendly: Res<FriendlyFactions>,
    mut units: Query<(&Faction, &Unit, &MapPosition, &mut Visibility, Has<Hiding>), FogUnitFilter>,
) {
    if !fog_active.0 {
        for (_, _, _, mut vis, _) in &mut units {
            vis.set_if_neq(Visibility::Inherited);
        }
        return;
    }

    for (faction, unit, pos, mut vis, is_hiding) in &mut units {
        let is_friendly = friendly.0.contains(&faction.0);
        let visible_to_viewer = is_friendly
            || (!is_hiding
                && fog_map.is_unit_visible(pos.position(), unit.0.domain() == UnitDomain::Air));
        let target = if visible_to_viewer {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        vis.set_if_neq(target);
    }
}

pub struct FogPlugin;

impl Plugin for FogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FogOfWarMap>()
            .init_resource::<FogActive>()
            .init_resource::<FriendlyFactions>()
            .add_systems(
                Update,
                apply_fog_to_units.run_if(in_state(crate::core::AppState::InGame)),
            );
    }
}
