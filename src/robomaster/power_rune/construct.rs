use crate::capture::{AimCaptureTargetCandidate, mark_aim_capture_target};
use crate::robomaster::power_rune::collision::RuneIndex;
use crate::robomaster::power_rune::common::{RUNE_TARGET_COUNT, RuneMode};
use crate::robomaster::power_rune::rotation::PowerRuneRotation;
use crate::robomaster::power_rune::rune::{PowerRune, PowerRuneMechanism};
use crate::robomaster::power_rune::visual::{PowerRuneVisuals, RuneVisual};
use crate::robomaster::prelude::Team;
use crate::robomaster::visibility::{Controller, StatefulAppearanceCreator};
use crate::util::bevy::{drain_entities_by, insert_all_child};
use crate::{material, visibility};
use avian3d::prelude::CollisionEventsEnabled;
use bevy::ecs::system::SystemParam;
use bevy::prelude::{Children, Commands, Component, Entity, Name, On, Query, Res, With};
use bevy::world_serialization::{WorldInstanceReady, WorldInstanceSpawner};
use rand::Rng;
use std::collections::HashMap;

#[derive(Component)]
pub struct PowerRuneRoot;

#[derive(Component, Debug, Copy, Clone)]
pub struct RuneVisualIndex {
    pub target: usize,
    pub rune: Entity,
}

fn build_targets(
    face_index: usize,
    face_entity: Entity,
    name_map: &mut HashMap<&str, Entity>,
    param: &mut PowerRuneParam,
    creator: &mut StatefulAppearanceCreator,
) -> Option<[RuneVisual; RUNE_TARGET_COUNT]> {
    let mut targets = Vec::new();
    for target_idx in 1..=5 {
        let prefix = format!("FACE_{}_TARGET_{}", face_index, target_idx);

        let padding_entities = drain_entities_by(name_map, |name| {
            name.starts_with(&format!("{}_PADDING", prefix))
        });
        let padding_segments =
            creator.create_controller(padding_entities.clone(), material!(on = { completed }));
        let progress_entities = drain_entities_by(name_map, |name| {
            name.starts_with(&format!("{}_LEGGING_PROGRESSING", prefix))
        });
        let progress_segments =
            creator.create_controller(progress_entities.clone(), visibility!(activating));

        let ad = format!("{}_ACTIVATED", prefix);
        let at = format!("{}_ACTIVE", prefix);
        let d = format!("{}_DISABLED", prefix);
        let c = format!("{}_COMPLETED", prefix);
        let activated = ad.as_str();
        let active = at.as_str();
        let deactivated = d.as_str();
        let completed = c.as_str();

        let activated = name_map.remove(activated);
        let activating = name_map.remove(active);
        let deactivated = name_map.remove(deactivated);
        let completed = name_map.remove(completed);

        let logical_index = targets.len();
        let target_entities: Vec<Entity> = [deactivated, activating, activated, completed]
            .into_iter()
            .flatten()
            .collect();
        for entity in target_entities.iter().copied() {
            insert_all_child(&mut param.commands, entity, &param.children, || {
                (
                    RuneIndex {
                        target: logical_index,
                        rune: face_entity,
                    },
                    CollisionEventsEnabled,
                )
            });
        }

        let mut visual_entities = target_entities;
        visual_entities.extend(padding_entities.iter().copied());
        visual_entities.extend(progress_entities.iter().copied());

        let mut legging_segments: [Controller; 3] = [
            Controller::new_combined(vec![]),
            Controller::new_combined(vec![]),
            Controller::new_combined(vec![]),
        ];
        for legging_idx in 1..=3 {
            let legging_entities = drain_entities_by(name_map, |name| {
                name.starts_with(&format!("{}_LEGGING_{}", prefix, legging_idx))
                    && !name.contains("PROGRESSING")
            });
            visual_entities.extend(legging_entities.iter().copied());
            legging_segments[legging_idx - 1] =
                creator.create_controller(legging_entities, material!(on = {activated, completed}))
        }

        for entity in visual_entities {
            insert_all_child(&mut param.commands, entity, &param.children, || {
                RuneVisualIndex {
                    target: logical_index,
                    rune: face_entity,
                }
            });
        }

        targets.push(RuneVisual::new(
            Controller::new_visibility(deactivated, activating, activated, completed),
            legging_segments,
            padding_segments,
            progress_segments,
        ));
    }
    targets.try_into().ok()
}

#[derive(SystemParam)]
struct PowerRuneParam<'w, 's> {
    commands: Commands<'w, 's>,
    scene_spawner: Res<'w, WorldInstanceSpawner>,

    power_query: Query<'w, 's, (), With<PowerRuneRoot>>,
    capture_targets: Query<'w, 's, (), With<AimCaptureTargetCandidate>>,
    names: Query<'w, 's, &'static Name>,
    children: Query<'w, 's, &'static Children>,
}

fn setup_power_rune(
    events: On<WorldInstanceReady>,
    mut param: PowerRuneParam,
    mut creator: StatefulAppearanceCreator,
) {
    if !param.power_query.contains(events.entity) {
        return;
    }

    if param.capture_targets.contains(events.entity) {
        let descendants: Vec<_> = param
            .scene_spawner
            .iter_instance_entities(events.instance_id)
            .collect();
        mark_aim_capture_target(&mut param.commands, events.entity, descendants);
    }

    let names = param.names;
    let mut name_map = param
        .scene_spawner
        .iter_instance_entities(events.instance_id)
        .filter_map(|entity| names.get(entity).map(|n| (n.as_str(), entity)).ok())
        .fold(HashMap::new(), |mut m, (name, entity)| {
            m.insert(name, entity);
            m
        });

    if name_map.is_empty() {
        return;
    }

    let mut faces: Vec<(usize, Entity)> = name_map
        .iter()
        .filter_map(|(name, &entity)| {
            let rest = name.strip_prefix("FACE_")?;
            if rest.contains('_') {
                return None;
            }
            let index = rest.parse::<usize>().ok()?;
            Some((index, entity))
        })
        .collect();

    faces.sort_by_key(|(idx, _)| *idx);
    if faces.is_empty() {
        return;
    }

    let red_clockwise = rand::thread_rng().gen_bool(0.5);

    for (index, face_entity) in faces {
        let mode = RuneMode::Small;

        let deactivated = name_map.remove(format!("FACE_{}_R_UNPOWERED", index).as_str());
        let activated = name_map.remove(format!("FACE_{}_R_POWERED", index).as_str());

        let Some(targets) =
            build_targets(index, face_entity, &mut name_map, &mut param, &mut creator)
        else {
            continue;
        };

        let team = if (index & 1) > 0 {
            Team::Red
        } else {
            Team::Blue
        };
        let clockwise = match team {
            Team::Red => red_clockwise,
            Team::Blue => !red_clockwise,
        };
        let mut visuals = PowerRuneVisuals::new(
            Controller::new_visibility(deactivated, activated, activated, activated),
            targets,
        );
        let mechanism = PowerRuneMechanism::new(mode);
        visuals.apply(mode, mechanism.state(), &mut creator.appearance);

        param.commands.entity(face_entity).insert((
            PowerRune::new(team, mode),
            mechanism,
            PowerRuneRotation::new(clockwise),
            visuals,
        ));
    }
}

#[derive(Default)]
pub(super) struct PowerRuneConstructorPlugin;

impl bevy::app::Plugin for PowerRuneConstructorPlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.add_observer(setup_power_rune);
    }
}
