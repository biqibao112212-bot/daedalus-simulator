use avian3d::prelude::{CollisionEventsEnabled, CollisionStart, LinearVelocity};
use bevy::prelude::{
    ChildOf, Commands, Entity, GlobalTransform, On, Plugin, Query, Res, ResMut, Transform, Update,
    Vec3, With, info,
};
use std::collections::HashSet;

use super::construct::{Armor, ArmorHitZone, ArmorRoot};
use super::marker::MarkerData;
use crate::components::VehicleBodyCollider;
use crate::robomaster::power_rune::prelude::Projectile;
use crate::statistic::ProjectileStatistics;
use crate::telemetry::{
    ArmorTargetSnapshot, ProjectileImpactRecorded, ProjectileKinematics, ProjectileTelemetry,
    ProjectileTrace,
};

#[derive(Default, bevy::prelude::Resource)]
struct ConsumedVehicleProjectiles(HashSet<Entity>);

// The stock vehicle's broad chassis collider encloses the visual armor planes
// by roughly 25-50 mm. A body collision can therefore arrive before the
// dedicated armor collider even for a shot aimed at the plate centre. Allow a
// short forward ray segment to reach that same vehicle's first armor plane,
// but keep the segment far shorter than the chassis diameter so a rear plate
// can never turn a body shot into a score.
const BODY_TO_FRONT_ARMOR_MAX_DEPTH_M: f32 = 0.10;
const BODY_TO_FRONT_ARMOR_BACKTRACK_M: f32 = 0.025;

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
    armor_markers: Query<(&ArmorHitZone, &GlobalTransform, &MarkerData)>,
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

    let direct_target =
        find_armor_target(other_collider, &armor_hit_zones, &armor_roots, &child_of);
    let vehicle_body = direct_target
        .is_none()
        .then(|| find_vehicle_body(other_collider, &vehicle_bodies, &child_of))
        .flatten();
    if direct_target.is_none() && vehicle_body.is_none() {
        return;
    }

    let projected_target = vehicle_body.and_then(|vehicle_body| {
        let transform = projectile_data.0?;
        let velocity = projectile_data.1?.0;
        find_front_armor_along_body_impact(
            vehicle_body,
            transform.translation,
            velocity,
            &armor_markers,
            &armor_roots,
            &child_of,
        )
    });
    let projected_hit = projected_target.is_some();
    let target = direct_target.or_else(|| projected_target.map(|(target, _, _)| target));

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
        if projected_hit {
            info!(
                "Projectile {:?} scored on full-size front armor projected through enclosing body collider {:?}",
                projectile_entity, other_collider
            );
        } else {
            info!(
                "Projectile {:?} scored on full-size armor collider {:?}",
                projectile_entity, other_collider
            );
        }
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

fn find_vehicle_body(
    entity: Entity,
    vehicle_bodies: &Query<(), With<VehicleBodyCollider>>,
    child_of: &Query<&ChildOf>,
) -> Option<Entity> {
    if vehicle_bodies.contains(entity) {
        return Some(entity);
    }
    child_of
        .iter_ancestors(entity)
        .find(|ancestor| vehicle_bodies.contains(*ancestor))
}

fn find_front_armor_along_body_impact(
    vehicle_body: Entity,
    projectile_position: Vec3,
    projectile_velocity: Vec3,
    armor_markers: &Query<(&ArmorHitZone, &GlobalTransform, &MarkerData)>,
    armor_roots: &Query<(Entity, &Armor, Option<&GlobalTransform>), With<ArmorRoot>>,
    child_of: &Query<&ChildOf>,
) -> Option<(ArmorTargetSnapshot, Entity, f32)> {
    let direction = projectile_velocity.normalize_or_zero();
    if direction == Vec3::ZERO {
        return None;
    }

    armor_markers
        .iter()
        .filter(|(hit_zone, _, _)| {
            hit_zone.root == vehicle_body
                || child_of
                    .iter_ancestors(hit_zone.root)
                    .any(|ancestor| ancestor == vehicle_body)
        })
        .filter_map(|(hit_zone, marker_transform, marker)| {
            let corners = marker
                .0
                .map(|point| marker_transform.transform_point(point));
            let distance = ray_quad_distance(projectile_position, direction, corners)?;
            if !(-BODY_TO_FRONT_ARMOR_BACKTRACK_M..=BODY_TO_FRONT_ARMOR_MAX_DEPTH_M)
                .contains(&distance)
            {
                return None;
            }
            let target =
                armor_roots
                    .get(hit_zone.root)
                    .ok()
                    .map(|(entity, armor, transform)| {
                        ArmorTargetSnapshot::from_components(entity, armor, transform)
                    })?;
            Some((target, hit_zone.root, distance))
        })
        .min_by(|left, right| left.2.total_cmp(&right.2))
}

fn ray_quad_distance(origin: Vec3, direction: Vec3, corners: [Vec3; 4]) -> Option<f32> {
    // Marker meshes use two triangles with indices 1,0,2 and 1,2,3. Keeping
    // the exact asset topology makes the scoring polygon identical to the
    // full-size four-corner surface used by labels and rendering.
    ray_triangle_distance(origin, direction, corners[1], corners[0], corners[2])
        .into_iter()
        .chain(ray_triangle_distance(
            origin, direction, corners[1], corners[2], corners[3],
        ))
        .min_by(f32::total_cmp)
}

fn ray_triangle_distance(
    origin: Vec3,
    direction: Vec3,
    vertex0: Vec3,
    vertex1: Vec3,
    vertex2: Vec3,
) -> Option<f32> {
    const EPSILON: f32 = 1.0e-6;
    let edge1 = vertex1 - vertex0;
    let edge2 = vertex2 - vertex0;
    let cross = direction.cross(edge2);
    let determinant = edge1.dot(cross);
    if determinant.abs() <= EPSILON {
        return None;
    }

    let inverse_determinant = determinant.recip();
    let from_vertex = origin - vertex0;
    let u = from_vertex.dot(cross) * inverse_determinant;
    if !(-EPSILON..=1.0 + EPSILON).contains(&u) {
        return None;
    }

    let barycentric_cross = from_vertex.cross(edge1);
    let v = direction.dot(barycentric_cross) * inverse_determinant;
    if v < -EPSILON || u + v > 1.0 + EPSILON {
        return None;
    }
    Some(edge2.dot(barycentric_cross) * inverse_determinant)
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

    const TEST_PLATE_HALF_WIDTH_M: f32 = 0.0669;
    const TEST_PLATE_HALF_HEIGHT_M: f32 = 0.0260;

    fn collision_test_app() -> App {
        let mut app = App::new();
        app.insert_resource(ProjectileStatistics::default());
        app.insert_resource(ProjectileTelemetry::from_env());
        app.init_resource::<ConsumedVehicleProjectiles>();
        app.add_observer(handle_vehicle_projectile_collision);
        app
    }

    fn spawn_test_armor(app: &mut App, vehicle_body: Entity, plane_z: f32, name: &str) -> Entity {
        let spec = ArmorSpec::Small(SmallArmorLabel::Outpost);
        let armor_root = app
            .world_mut()
            .spawn((
                ArmorRoot {
                    id: ArmorId::new(10),
                },
                Armor {
                    name: name.to_string(),
                    team: Team::Red,
                    spec,
                    label: spec.label(),
                },
                GlobalTransform::from_translation(Vec3::new(0.0, 0.0, plane_z)),
            ))
            .id();
        app.world_mut()
            .entity_mut(vehicle_body)
            .add_child(armor_root);

        let marker = app
            .world_mut()
            .spawn((
                ArmorHitZone { root: armor_root },
                MarkerData([
                    Vec3::new(TEST_PLATE_HALF_WIDTH_M, TEST_PLATE_HALF_HEIGHT_M, 0.0),
                    Vec3::new(TEST_PLATE_HALF_WIDTH_M, -TEST_PLATE_HALF_HEIGHT_M, 0.0),
                    Vec3::new(-TEST_PLATE_HALF_WIDTH_M, TEST_PLATE_HALF_HEIGHT_M, 0.0),
                    Vec3::new(-TEST_PLATE_HALF_WIDTH_M, -TEST_PLATE_HALF_HEIGHT_M, 0.0),
                ]),
                GlobalTransform::from_translation(Vec3::new(0.0, 0.0, plane_z)),
            ))
            .id();
        app.world_mut().entity_mut(armor_root).add_child(marker);
        marker
    }

    fn trigger_body_hit(app: &mut App, vehicle_body: Entity, projectile_x: f32) -> Entity {
        let projectile = app
            .world_mut()
            .spawn((
                Projectile,
                CollisionEventsEnabled,
                Transform::from_xyz(projectile_x, 0.0, 0.2675),
                LinearVelocity(Vec3::new(0.0, 0.0, -25.0)),
            ))
            .id();
        app.world_mut().trigger(CollisionStart {
            collider1: projectile,
            collider2: vehicle_body,
            body1: Some(projectile),
            body2: Some(vehicle_body),
        });
        app.world_mut().flush();
        projectile
    }

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

    #[test]
    fn body_contact_aimed_at_full_front_plate_counts_as_accurate() {
        let mut app = collision_test_app();
        let vehicle_body = app.world_mut().spawn(VehicleBodyCollider).id();
        spawn_test_armor(&mut app, vehicle_body, 0.22, "front armor");

        trigger_body_hit(&mut app, vehicle_body, 0.0);

        assert_eq!(
            app.world()
                .resource::<ProjectileStatistics>()
                .accurate_count,
            1
        );
    }

    #[test]
    fn body_contact_just_outside_full_front_plate_is_a_miss() {
        let mut app = collision_test_app();
        let vehicle_body = app.world_mut().spawn(VehicleBodyCollider).id();
        spawn_test_armor(&mut app, vehicle_body, 0.22, "front armor");

        trigger_body_hit(&mut app, vehicle_body, TEST_PLATE_HALF_WIDTH_M + 0.002);

        assert_eq!(
            app.world()
                .resource::<ProjectileStatistics>()
                .accurate_count,
            0
        );
    }

    #[test]
    fn body_contact_cannot_project_through_chassis_to_rear_plate() {
        let mut app = collision_test_app();
        let vehicle_body = app.world_mut().spawn(VehicleBodyCollider).id();
        spawn_test_armor(&mut app, vehicle_body, -0.22, "rear armor");

        trigger_body_hit(&mut app, vehicle_body, 0.0);

        assert_eq!(
            app.world()
                .resource::<ProjectileStatistics>()
                .accurate_count,
            0
        );
    }
}
