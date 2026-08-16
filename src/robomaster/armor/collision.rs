use avian3d::prelude::{CollisionEventsEnabled, CollisionStart, LinearVelocity};
use bevy::prelude::{
    ChildOf, Commands, Entity, GlobalTransform, On, Plugin, Query, Res, ResMut, Transform, Update,
    With, info,
};
use std::collections::HashSet;

use super::construct::{Armor, ArmorHitZone, ArmorRoot};
use crate::components::VehicleBodyCollider;
use crate::robomaster::power_rune::prelude::Projectile;
use crate::statistic::ProjectileStatistics;
use crate::telemetry::{
    ArmorTargetSnapshot, ProjectileImpactRecorded, ProjectileKinematics, ProjectileTelemetry,
    ProjectileTrace,
};

#[derive(Default, bevy::prelude::Resource)]
struct ConsumedVehicleProjectiles(HashSet<Entity>);

fn handle_vehicle_projectile_collision(
    event: On<CollisionStart>,
    mut commands: Commands,
    mut consumed: ResMut<ConsumedVehicleProjectiles>,
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
    vehicle_bodies: Query<(), With<VehicleBodyCollider>>,
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

    let target = find_armor_target(other_collider, &armor_hit_zones, &armor_roots, &child_of);
    let hit_vehicle_body = target.is_none()
        && (vehicle_bodies.contains(other_collider)
            || child_of
                .iter_ancestors(other_collider)
                .any(|ancestor| vehicle_bodies.contains(ancestor)));
    if target.is_none() && !hit_vehicle_body {
        return;
    }

    // Resolve both scoring armor and non-scoring vehicle-body contacts in one
    // observer. The set is updated immediately, unlike deferred component
    // commands, so simultaneous contacts cannot score the same projectile
    // after a body hit.
    if !consumed.0.insert(projectile_entity) {
        return;
    }

    commands
        .entity(projectile_entity)
        .remove::<CollisionEventsEnabled>()
        .insert(ProjectileImpactRecorded);

    if let Some(target) = target {
        info!(
            "Projectile {:?} scored on full-size armor collider {:?}",
            projectile_entity, other_collider
        );
        stats.increase_accurate();
        let projectile =
            ProjectileKinematics::from_components(projectile_data.0, projectile_data.1);
        telemetry.write_armor_hit(projectile_data.2, projectile, target);
    } else {
        info!(
            "Projectile {:?} hit vehicle body collider {:?}; recorded as miss",
            projectile_entity, other_collider
        );
    }
}

fn cleanup_consumed_vehicle_projectiles(
    mut consumed: ResMut<ConsumedVehicleProjectiles>,
    projectiles: Query<(), With<Projectile>>,
) {
    consumed.0.retain(|entity| projectiles.contains(*entity));
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
        app.init_resource::<ConsumedVehicleProjectiles>()
            .add_systems(Update, cleanup_consumed_vehicle_projectiles)
            .add_observer(handle_vehicle_projectile_collision);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::robomaster::prelude::{ArmorId, ArmorSpec, SmallArmorLabel, Team};
    use crate::telemetry::ProjectileTelemetry;
    use avian3d::prelude::CollisionStart;
    use bevy::prelude::App;

    #[test]
    fn projectile_hit_on_armor_hit_zone_counts_as_accurate() {
        let mut app = App::new();
        app.insert_resource(ProjectileStatistics::default());
        app.insert_resource(ProjectileTelemetry::from_env());
        app.init_resource::<ConsumedVehicleProjectiles>();
        app.add_observer(handle_vehicle_projectile_collision);

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

        app.world_mut().trigger(CollisionStart {
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
        app.init_resource::<ConsumedVehicleProjectiles>();
        app.add_observer(handle_vehicle_projectile_collision);

        let projectile = app
            .world_mut()
            .spawn((Projectile, CollisionEventsEnabled))
            .id();
        let vehicle_body = app.world_mut().spawn(VehicleBodyCollider).id();

        app.world_mut().trigger(CollisionStart {
            collider1: projectile,
            collider2: vehicle_body,
            body1: Some(projectile),
            body2: None,
        });
        app.world_mut().flush();

        let stats = app.world().resource::<ProjectileStatistics>();
        assert_eq!(stats.accurate_count, 0);
        assert!(
            !app.world()
                .entity(projectile)
                .contains::<CollisionEventsEnabled>()
        );
        assert!(
            app.world()
                .entity(projectile)
                .contains::<ProjectileImpactRecorded>()
        );
    }

    #[test]
    fn projectile_cannot_score_on_armor_after_vehicle_body_contact() {
        let mut app = App::new();
        app.insert_resource(ProjectileStatistics::default());
        app.insert_resource(ProjectileTelemetry::from_env());
        app.init_resource::<ConsumedVehicleProjectiles>();
        app.add_observer(handle_vehicle_projectile_collision);

        let projectile = app
            .world_mut()
            .spawn((Projectile, CollisionEventsEnabled))
            .id();
        let vehicle_body = app.world_mut().spawn(VehicleBodyCollider).id();
        let spec = ArmorSpec::Small(SmallArmorLabel::Outpost);
        let armor_root = app
            .world_mut()
            .spawn((
                ArmorRoot {
                    id: ArmorId::new(1),
                },
                Armor {
                    name: "rear armor".to_string(),
                    team: Team::Red,
                    spec,
                    label: spec.label(),
                },
            ))
            .id();
        let rear_armor = app
            .world_mut()
            .spawn(ArmorHitZone { root: armor_root })
            .id();

        app.world_mut().trigger(CollisionStart {
            collider1: projectile,
            collider2: vehicle_body,
            body1: Some(projectile),
            body2: Some(vehicle_body),
        });
        app.world_mut().trigger(CollisionStart {
            collider1: projectile,
            collider2: rear_armor,
            body1: Some(projectile),
            body2: Some(vehicle_body),
        });
        app.world_mut().flush();

        assert_eq!(
            app.world()
                .resource::<ProjectileStatistics>()
                .accurate_count,
            0
        );
    }
}
