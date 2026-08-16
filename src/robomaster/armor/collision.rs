use avian3d::prelude::{CollisionEnd, CollisionEventsEnabled, LinearVelocity};
use bevy::prelude::{
    ChildOf, Commands, Entity, GlobalTransform, On, Plugin, Query, Res, ResMut, Transform, With,
};

use super::construct::{Armor, ArmorHitZone, ArmorRoot};
use crate::robomaster::power_rune::prelude::Projectile;
use crate::statistic::ProjectileStatistics;
use crate::telemetry::{
    ArmorTargetSnapshot, ProjectileImpactRecorded, ProjectileKinematics, ProjectileTelemetry,
    ProjectileTrace,
};

fn handle_armor_collision(
    event: On<CollisionEnd>,
    mut commands: Commands,
    mut stats: ResMut<ProjectileStatistics>,
    telemetry: Res<ProjectileTelemetry>,
    projectiles: Query<
        (
            Option<&Transform>,
            Option<&LinearVelocity>,
            Option<&ProjectileTrace>,
        ),
        With<Projectile>,
    >,
    armor_hit_zones: Query<&ArmorHitZone>,
    armor_roots: Query<(Entity, &Armor, Option<&GlobalTransform>), With<ArmorRoot>>,
    child_of: Query<&ChildOf>,
) {
    let projectile_body1 = event.body1.and_then(|body| {
        projectiles
            .get(body)
            .ok()
            .map(|projectile| (body, projectile))
    });
    let projectile_body2 = event.body2.and_then(|body| {
        projectiles
            .get(body)
            .ok()
            .map(|projectile| (body, projectile))
    });

    let (projectile_entity, projectile_data) = match (projectile_body1, projectile_body2) {
        (Some(projectile), _) => projectile,
        (_, Some(projectile)) => projectile,
        _ => return,
    };

    let other_collider = if event.body1 == Some(projectile_entity) {
        event.collider2
    } else {
        event.collider1
    };

    let Some(target) = find_armor_target(other_collider, &armor_hit_zones, &armor_roots, &child_of)
    else {
        return;
    };

    // Disable collision events for this projectile so it only counts once.
    commands
        .entity(projectile_entity)
        .remove::<CollisionEventsEnabled>()
        .insert(ProjectileImpactRecorded);
    stats.increase_accurate();
    let projectile = ProjectileKinematics::from_components(projectile_data.0, projectile_data.1);
    telemetry.write_armor_hit(projectile_data.2, projectile, target);
}

fn find_armor_target(
    entity: Entity,
    armor_hit_zones: &Query<&ArmorHitZone>,
    armor_roots: &Query<(Entity, &Armor, Option<&GlobalTransform>), With<ArmorRoot>>,
    child_of: &Query<&ChildOf>,
) -> Option<ArmorTargetSnapshot> {
    let hit_zone = armor_hit_zones.get(entity).ok().copied().or_else(|| {
        child_of
            .iter_ancestors(entity)
            .find_map(|ancestor| armor_hit_zones.get(ancestor).ok().copied())
    })?;

    armor_roots
        .get(hit_zone.root)
        .ok()
        .map(|(entity, armor, transform)| {
            ArmorTargetSnapshot::from_components(entity, armor, transform)
        })
}

#[derive(Default)]
pub(super) struct ArmorCollisionPlugin;

impl Plugin for ArmorCollisionPlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.add_observer(handle_armor_collision);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::robomaster::prelude::{ArmorId, ArmorSpec, SmallArmorLabel, Team};
    use crate::telemetry::ProjectileTelemetry;
    use avian3d::prelude::CollisionEnd;
    use bevy::prelude::App;

    #[test]
    fn projectile_hit_on_armor_hit_zone_counts_as_accurate() {
        let mut app = App::new();
        app.insert_resource(ProjectileStatistics::default());
        app.insert_resource(ProjectileTelemetry::from_env());
        app.add_observer(handle_armor_collision);

        let projectile = app
            .world_mut()
            .spawn((Projectile, CollisionEventsEnabled))
            .id();
        let spec = ArmorSpec::Small(SmallArmorLabel::Outpost);
        let armor_root = app
            .world_mut()
            .spawn((
                ArmorRoot {
                    id: ArmorId::new(0),
                },
                Armor {
                    name: "outpost".to_string(),
                    team: Team::Red,
                    spec,
                    label: spec.label(),
                },
            ))
            .id();
        let outpost_armor = app
            .world_mut()
            .spawn(ArmorHitZone { root: armor_root })
            .id();

        app.world_mut().trigger(CollisionEnd {
            collider1: projectile,
            collider2: outpost_armor,
            body1: Some(projectile),
            body2: None,
        });
        app.world_mut().flush();

        let stats = app.world().resource::<ProjectileStatistics>();
        assert_eq!(stats.accurate_count, 1);
        assert!(
            !app.world()
                .entity(projectile)
                .contains::<CollisionEventsEnabled>()
        );
    }

    #[test]
    fn projectile_hit_on_vehicle_body_is_not_accurate() {
        let mut app = App::new();
        app.insert_resource(ProjectileStatistics::default());
        app.insert_resource(ProjectileTelemetry::from_env());
        app.add_observer(handle_armor_collision);

        let projectile = app
            .world_mut()
            .spawn((Projectile, CollisionEventsEnabled))
            .id();
        let vehicle_body = app.world_mut().spawn_empty().id();

        app.world_mut().trigger(CollisionEnd {
            collider1: projectile,
            collider2: vehicle_body,
            body1: Some(projectile),
            body2: None,
        });
        app.world_mut().flush();

        let stats = app.world().resource::<ProjectileStatistics>();
        assert_eq!(stats.accurate_count, 0);
        assert!(
            app.world()
                .entity(projectile)
                .contains::<CollisionEventsEnabled>()
        );
        assert!(
            !app.world()
                .entity(projectile)
                .contains::<ProjectileImpactRecorded>()
        );
    }
}
