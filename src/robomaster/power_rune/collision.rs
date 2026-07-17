use crate::robomaster::power_rune::common::RuneHitOutcome;
use crate::robomaster::power_rune::rotation::PowerRuneRotation;
use crate::robomaster::power_rune::rune::{PowerRune, PowerRuneMechanism};
use crate::telemetry::{
    ProjectileImpactRecorded, ProjectileKinematics, ProjectileTelemetry, ProjectileTrace,
    RuneTargetSnapshot,
};
use avian3d::prelude::LinearVelocity;
use avian3d::prelude::{CollisionEnd, CollisionEventsEnabled};
use bevy::prelude::{
    ChildOf, Commands, Component, Entity, EntityEvent, GlobalTransform, On, Query, Res, ResMut,
    Resource, Transform, Update, With,
};
use std::collections::HashSet;

#[derive(Component)]
#[require(CollisionEventsEnabled)]
pub struct Projectile;

#[derive(Resource, Default)]
struct ConsumedRuneProjectiles(HashSet<Entity>);

#[derive(Component, Debug, Copy, Clone)]
pub struct RuneIndex {
    pub target: usize,
    pub rune: Entity,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct HitResult {
    pub outcome: RuneHitOutcome,
}

impl HitResult {
    pub const fn accurate(self) -> bool {
        self.outcome.is_accurate()
    }
}

#[derive(EntityEvent)]
pub struct RuneActivated {
    #[event_target]
    pub rune: Entity,
}

#[derive(EntityEvent)]
pub struct RuneHit {
    #[event_target]
    pub rune: Entity,
    pub result: HitResult,
}

fn handle_rune_collision(
    event: On<CollisionEnd>,
    mut commands: Commands,
    mut consumed_projectiles: ResMut<ConsumedRuneProjectiles>,
    telemetry: Res<ProjectileTelemetry>,
    mut runes: Query<(
        &mut PowerRuneMechanism,
        &mut PowerRuneRotation,
        &PowerRune,
        Option<&GlobalTransform>,
    )>,
    targets: Query<&RuneIndex>,
    projectiles: Query<
        (
            Option<&Transform>,
            Option<&LinearVelocity>,
            Option<&ProjectileTrace>,
        ),
        With<Projectile>,
    >,
    transforms: Query<&GlobalTransform>,
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

    let (projectile_entity, projectile_data, target_collider) =
        match (projectile_body1, projectile_body2) {
            (Some((projectile, projectile_data)), _) => {
                (projectile, projectile_data, event.collider2)
            }
            (_, Some((projectile, projectile_data))) => {
                (projectile, projectile_data, event.collider1)
            }
            _ => return,
        };

    let target_position_m = transforms
        .get(target_collider)
        .ok()
        .map(GlobalTransform::translation);

    let Some(target) = find_rune_target(target_collider, &targets, &child_of) else {
        return;
    };

    let Ok((mut mechanism, mut rotation, rune, rune_transform)) = runes.get_mut(target.rune) else {
        return;
    };

    if !consumed_projectiles.0.insert(projectile_entity) {
        return;
    }

    commands
        .entity(projectile_entity)
        .remove::<CollisionEventsEnabled>()
        .insert(ProjectileImpactRecorded);

    let mut rng = rand::thread_rng();
    let outcome = mechanism.state_mut().hit(target.target, &mut rng);
    rotation.sync_activation(
        mechanism.state().mode(),
        mechanism.state().is_activating(),
        &mut rng,
    );

    let projectile = ProjectileKinematics::from_components(projectile_data.0, projectile_data.1);
    let rune_target = RuneTargetSnapshot {
        rune: target.rune,
        target_index: target.target,
        team: format!("{:?}", rune.team()),
        mode: format!("{:?}", rune.mode()),
        rune_position_m: rune_transform.map(GlobalTransform::translation),
        target_position_m,
    };
    telemetry.write_rune_hit(
        projectile_data.2,
        projectile,
        rune_target,
        &format!("{:?}", outcome),
        outcome.is_accurate(),
        outcome.activates_rune(),
    );

    commands.trigger(RuneHit {
        rune: target.rune,
        result: HitResult { outcome },
    });

    if outcome.activates_rune() {
        commands.trigger(RuneActivated { rune: target.rune });
    }
}

fn cleanup_consumed_rune_projectiles(
    mut consumed_projectiles: ResMut<ConsumedRuneProjectiles>,
    projectiles: Query<(), With<Projectile>>,
) {
    consumed_projectiles
        .0
        .retain(|entity| projectiles.contains(*entity));
}

fn find_rune_target(
    entity: Entity,
    targets: &Query<&RuneIndex>,
    child_of: &Query<&ChildOf>,
) -> Option<RuneIndex> {
    if let Ok(target) = targets.get(entity) {
        return Some(*target);
    }

    child_of
        .iter_ancestors(entity)
        .find_map(|ancestor| targets.get(ancestor).ok().copied())
}

#[derive(Default)]
pub(super) struct PowerRuneCollisionPlugin;

impl bevy::app::Plugin for PowerRuneCollisionPlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.init_resource::<ConsumedRuneProjectiles>()
            .add_systems(Update, cleanup_consumed_rune_projectiles)
            .add_observer(handle_rune_collision);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::on_hit;
    use crate::robomaster::power_rune::common::{RUNE_TARGET_COUNT, RuneMode};
    use crate::robomaster::power_rune::rune::PowerRune;
    use crate::robomaster::prelude::Team;
    use crate::statistic::ProjectileStatistics;
    use crate::telemetry::{ProjectileImpactRecorded, ProjectileTelemetry};
    use bevy::prelude::{App, Transform};

    #[test]
    fn projectile_hit_on_active_small_rune_target_counts_as_accurate() {
        let mut app = App::new();
        app.insert_resource(ProjectileStatistics::default());
        app.insert_resource(ProjectileTelemetry::from_env());
        app.init_resource::<ConsumedRuneProjectiles>();
        app.add_observer(handle_rune_collision);
        app.add_observer(on_hit);

        let mut rng = rand::thread_rng();
        let mut mechanism = PowerRuneMechanism::new(RuneMode::Small);
        mechanism.state_mut().tick(f32::MAX, &mut rng);

        let rune = app
            .world_mut()
            .spawn((
                PowerRune::new(Team::Red, RuneMode::Small),
                mechanism,
                PowerRuneRotation::new(true),
                Transform::default(),
            ))
            .id();

        for target_index in 0..RUNE_TARGET_COUNT {
            let target = app
                .world_mut()
                .spawn(RuneIndex {
                    target: target_index,
                    rune,
                })
                .id();
            let projectile = app
                .world_mut()
                .spawn((Projectile, CollisionEventsEnabled))
                .id();

            app.world_mut().trigger(CollisionEnd {
                collider1: projectile,
                collider2: target,
                body1: Some(projectile),
                body2: None,
            });
            app.world_mut().flush();

            if app
                .world()
                .resource::<ProjectileStatistics>()
                .accurate_count
                == 1
            {
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
                return;
            }
        }

        panic!("no active small rune target registered an accurate projectile hit");
    }
}
