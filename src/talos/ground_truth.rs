use crate::capture::CaptureSource;
use crate::components::{Controlled, Infantry, InfantryGimbal, InfantryLaunchOffset};
use crate::robomaster::prelude::{
    Activation, ArmorRoot, MechanismState, PowerRune, PowerRuneMechanism, PowerRuneRotation,
    RuneMode, Team,
};
use crate::talos::capture::{TalosCaptureContext, TalosFrameStamp};
use crate::talos::plugin::M_ALIGN_MAT3;
use avian3d::prelude::{AngularVelocity, LinearVelocity};
use bevy::prelude::*;
use std::f32::consts::PI;
use talos_ipc::*;

// Infantry targets in this simulator use the four-armor Z4 layout.  Scene
// construction is deferred, so a target can legitimately exist for one or
// more startup frames before all ArmorRoot components have been installed.
// Such frames still carry the chassis world state, but they are not eligible
// as armor-geometry truth until the complete Z4 set is present.
const EXPECTED_Z4_ARMOR_COUNT: u8 = 4;

fn to_ros_vec3(v: Vec3) -> Vec3 {
    M_ALIGN_MAT3 * v
}

fn team_to_u8(team: &Team) -> u8 {
    match team {
        Team::Red => 0,
        Team::Blue => 1,
    }
}

fn activation_to_u8(a: &Activation) -> u8 {
    match a {
        Activation::Deactivated => 0,
        Activation::Activating => 1,
        Activation::Activated => 2,
        Activation::Completed => 3,
    }
}

fn mechanism_state_to_u8(s: &MechanismState) -> u8 {
    match s {
        MechanismState::Inactive { .. } => 0,
        MechanismState::Activating(_) => 1,
        MechanismState::Activated { .. } => 2,
        MechanismState::Failed { .. } => 3,
    }
}

fn rune_mode_to_u8(m: &RuneMode) -> u8 {
    match m {
        RuneMode::Small => 0,
        RuneMode::Large => 1,
    }
}

/// Compute yaw in the ROS reference frame from a Bevy GlobalTransform.
///
/// The alignment matrix maps Bevy (Y-up) → ROS (Z-up).
/// We convert the rotation quaternion through the alignment to extract the Z-up yaw.
fn ros_rotation(global_tf: &GlobalTransform) -> Quat {
    let align_quat = Quat::from_mat3(&M_ALIGN_MAT3);
    align_quat * global_tf.rotation() * align_quat.inverse()
}

fn ros_yaw(global_tf: &GlobalTransform) -> f32 {
    let ros_rot = ros_rotation(global_tf);
    let (yaw, _, _) = ros_rot.to_euler(EulerRot::ZYX);
    yaw
}

#[derive(Clone, Copy, Debug)]
struct ArmorGeometrySample {
    position: Vec3,
    visibility: u8,
}

fn belongs_to_via(
    entity: Entity,
    root: Entity,
    mut parent_of: impl FnMut(Entity) -> Option<Entity>,
) -> bool {
    let mut current = entity;
    while current != root {
        let Some(parent) = parent_of(current) else {
            return false;
        };
        current = parent;
    }
    true
}

fn belongs_to(entity: Entity, root: Entity, parents: &Query<&ChildOf>) -> bool {
    belongs_to_via(entity, root, |current| {
        parents.get(current).ok().map(ChildOf::parent)
    })
}

fn armor_geometry_is_truth_eligible(armor_count: u8) -> bool {
    armor_count == EXPECTED_Z4_ARMOR_COUNT
}

fn summarize_armor_geometry(
    mut samples: Vec<ArmorGeometrySample>,
) -> (
    u8,
    f32,
    f32,
    f32,
    [GroundTruthArmor; GROUND_TRUTH_MAX_ARMORS_PER_TARGET],
) {
    samples.sort_by(|a, b| {
        a.position
            .y
            .atan2(a.position.x)
            .total_cmp(&b.position.y.atan2(b.position.x))
    });
    samples.truncate(GROUND_TRUTH_MAX_ARMORS_PER_TARGET);

    let mut armors = [GroundTruthArmor::default(); GROUND_TRUTH_MAX_ARMORS_PER_TARGET];
    let mut even_radius_sum = 0.0;
    let mut odd_radius_sum = 0.0;
    let mut even_count = 0u32;
    let mut odd_count = 0u32;
    let mut height_sum = 0.0;

    for (slot, sample) in samples.iter().enumerate() {
        let radius = sample.position.x.hypot(sample.position.y);
        let outward = if radius > f32::EPSILON {
            Vec3::new(sample.position.x / radius, sample.position.y / radius, 0.0)
        } else {
            Vec3::ZERO
        };
        armors[slot] = GroundTruthArmor {
            relative_slot: slot as u8,
            visibility: sample.visibility,
            _pad1: [0; 2],
            relative_position: sample.position.to_array(),
            outward_normal: outward.to_array(),
            relative_yaw: outward.y.atan2(outward.x),
        };
        if slot % 2 == 0 {
            even_radius_sum += radius;
            even_count += 1;
        } else {
            odd_radius_sum += radius;
            odd_count += 1;
        }
        height_sum += sample.position.z;
    }

    let mean = |sum: f32, count: u32| {
        if count == 0 { 0.0 } else { sum / count as f32 }
    };
    let armor_count = samples.len() as u8;
    (
        armor_count,
        mean(even_radius_sum, even_count),
        mean(odd_radius_sum, odd_count),
        mean(height_sum, armor_count as u32),
        armors,
    )
}

pub fn publish_ground_truth_system(
    context: Option<Res<TalosCaptureContext>>,
    frame_stamp: Res<TalosFrameStamp>,
    infantry_query: Query<
        (
            Entity,
            &GlobalTransform,
            Option<&LinearVelocity>,
            Option<&AngularVelocity>,
            &Infantry,
        ),
        Without<Controlled>,
    >,
    controlled_query: Query<
        (
            Entity,
            &GlobalTransform,
            Option<&LinearVelocity>,
            Option<&AngularVelocity>,
            &Infantry,
        ),
        With<Controlled>,
    >,
    armor_query: Query<
        (
            Entity,
            &GlobalTransform,
            Option<&Visibility>,
            Option<&InheritedVisibility>,
        ),
        With<ArmorRoot>,
    >,
    parent_query: Query<&ChildOf>,
    controlled_chassis_query: Query<&GlobalTransform, (With<Controlled>, With<Infantry>)>,
    gimbal_query: Query<
        (&GlobalTransform, &InfantryGimbal),
        (With<Controlled>, With<InfantryGimbal>),
    >,
    launch_offset_query: Query<&Transform, (With<Controlled>, With<InfantryLaunchOffset>)>,
    camera_query: Query<&GlobalTransform, With<CaptureSource>>,
    rune_query: Query<(
        &GlobalTransform,
        &Transform,
        &PowerRune,
        &PowerRuneMechanism,
        &PowerRuneRotation,
    )>,
) {
    let Some(ctx) = context else {
        return;
    };

    let frame_seq = frame_stamp.frame_seq;
    let timestamp_ns = frame_stamp.timestamp_ns;

    let mut batch = GroundTruthBatch::default();
    batch.frame_seq = frame_seq;
    batch.timestamp_ns = timestamp_ns;

    let mut exposure_state = ExposureState {
        frame_seq,
        timestamp_ns,
        world_frame: GROUND_TRUTH_FRAME_ROS_ODOM,
        ..Default::default()
    };
    let set_world_pose =
        |global_tf: &GlobalTransform, position: &mut [f32; 3], quaternion: &mut [f32; 4]| {
            let p = to_ros_vec3(global_tf.translation());
            let q = ros_rotation(global_tf);
            *position = p.to_array();
            *quaternion = [q.w, q.x, q.y, q.z];
        };
    if let Ok(chassis_tf) = controlled_chassis_query.single() {
        set_world_pose(
            chassis_tf,
            &mut exposure_state.chassis_position_world,
            &mut exposure_state.chassis_quaternion_world_wxyz,
        );
        let q = ros_rotation(chassis_tf);
        let (yaw, pitch, roll) = q.to_euler(EulerRot::ZYX);
        exposure_state.chassis_rpy_world = [roll, pitch, yaw];
        exposure_state.state_flags |= EXPOSURE_STATE_HAS_CHASSIS_WORLD_POSE;
    }
    if let (Ok((gimbal_tf, gimbal_state)), Ok(launch_local)) =
        (gimbal_query.single(), launch_offset_query.single())
    {
        let p = to_ros_vec3(gimbal_tf.translation());
        let q = {
            let adjusted = gimbal_tf.rotation()
                * launch_local.rotation
                * Quat::from_euler(EulerRot::ZYX, 0.0, 0.0, PI / 2.0);
            let align = Quat::from_mat3(&M_ALIGN_MAT3);
            align * adjusted * align.inverse()
        };
        exposure_state.gimbal_position_world = p.to_array();
        exposure_state.gimbal_quaternion_world_wxyz = [q.w, q.x, q.y, q.z];
        exposure_state.gimbal_yaw_rad = gimbal_state.local_yaw;
        exposure_state.gimbal_pitch_rad = gimbal_state.pitch;
        exposure_state.state_flags |= EXPOSURE_STATE_HAS_GIMBAL_WORLD_POSE;
    }
    if let Ok(camera_tf) = camera_query.single() {
        set_world_pose(
            camera_tf,
            &mut exposure_state.camera_position_world,
            &mut exposure_state.camera_quaternion_world_wxyz,
        );
        exposure_state.state_flags |= EXPOSURE_STATE_HAS_CAMERA_WORLD_POSE;
    }

    // Distribution consumers need exact image/exposure pose and gimbal state,
    // but not simulator target truth. Publish an empty batch with the frame
    // identity so the 16-slot history remains a synchronization channel.
    if crate::distribution::is_locked() {
        debug_assert_eq!(batch.target_count, 0);
        debug_assert_eq!(batch.rune_count, 0);
        if let Ok(mut publisher) = ctx.publisher.try_lock() {
            publisher.publish_ground_truth(&batch, &exposure_state);
        }
        return;
    }

    // Collect robot ground truth from all infantry robots
    let all_robots = infantry_query.iter().chain(controlled_query.iter());

    for (robot_entity, global_tf, lin_vel, ang_vel, infantry) in all_robots {
        let pos_ros = to_ros_vec3(global_tf.translation());
        let vel_ros = lin_vel
            .map(|velocity| to_ros_vec3(velocity.0))
            .unwrap_or(Vec3::ZERO);
        let team = &infantry.team;
        let config = infantry.config;

        let vyaw = ang_vel
            .map(|av| {
                let ros_ang = to_ros_vec3(av.0);
                ros_ang.z
            })
            .unwrap_or(0.0);

        let yaw = ros_yaw(global_tf);
        let world_rotation = ros_rotation(global_tf);

        let inverse_robot = global_tf.affine().inverse();
        let armor_samples = armor_query
            .iter()
            .filter(|(entity, ..)| belongs_to(*entity, robot_entity, &parent_query))
            .map(|(_, armor_tf, visibility, inherited_visibility)| {
                let local_bevy = inverse_robot.transform_point3(armor_tf.translation());
                let visibility = if matches!(visibility, Some(Visibility::Hidden))
                    || matches!(inherited_visibility, Some(inherited) if !inherited.get())
                {
                    GROUND_TRUTH_VISIBILITY_HIDDEN
                } else {
                    // This system has no camera-frustum/depth-occlusion result. Do not
                    // turn scene-level visibility into a false "camera visible" claim.
                    GROUND_TRUTH_VISIBILITY_UNKNOWN
                };
                ArmorGeometrySample {
                    position: to_ros_vec3(local_bevy),
                    visibility,
                }
            })
            .collect();
        let (armor_count, radius_even, radius_odd, armor_height, armors) =
            summarize_armor_geometry(armor_samples);
        let mut state_flags =
            GROUND_TRUTH_TARGET_HAS_WORLD_STATE | GROUND_TRUTH_TARGET_HAS_WORLD_ORIENTATION;
        // Do not advertise a startup/partial hierarchy as usable geometry.
        // Consumers must require both this flag and armor_count == 4.
        if armor_geometry_is_truth_eligible(armor_count) {
            state_flags |= GROUND_TRUTH_TARGET_HAS_ARMOR_GEOMETRY;
        }

        if (batch.target_count as usize) < GROUND_TRUTH_MAX_TARGETS {
            let idx = batch.target_count as usize;
            batch.targets[idx] = GroundTruthTarget {
                frame_seq,
                timestamp_ns,
                team: team_to_u8(team),
                armor_label: config.armor.label() as u8,
                is_outpost: 0,
                armor_count,
                position: [pos_ros.x, pos_ros.y, pos_ros.z],
                vyaw,
                yaw,
                velocity: [vel_ros.x, vel_ros.y, vel_ros.z],
                radius_even,
                radius_odd,
                armor_height,
                armors,
                target_id: robot_entity.to_bits(),
                world_quaternion_wxyz: [
                    world_rotation.w,
                    world_rotation.x,
                    world_rotation.y,
                    world_rotation.z,
                ],
                world_state_frame: GROUND_TRUTH_FRAME_ROS_ODOM,
                armor_geometry_frame: GROUND_TRUTH_FRAME_CHASSIS_LOCAL_ROS,
                _pad2: [0; 2],
                state_flags,
            };
            batch.target_count += 1;
        }
    }

    // Collect rune ground truth
    for (global_tf, local_tf, power_rune, mechanism, rotation) in rune_query.iter() {
        if (batch.rune_count as usize) >= GROUND_TRUTH_MAX_RUNES {
            break;
        }

        let pos_ros = to_ros_vec3(global_tf.translation());

        // Extract current rotation angle around the actual rune axis (-1, 0, -1).
        // The rune rotates via `rotate_local_axis(direction, angle)`, so we must
        // project the quaternion back onto that axis — not extract an Euler X angle.
        let rune_axis = Dir3::from_xyz(-1.0, 0.0, -1.0).unwrap();
        let (axis, angle) = local_tf.rotation.to_axis_angle();
        let current_angle = angle * axis.dot(*rune_axis).signum();

        let controller = rotation.controller();
        let direction = if controller.is_clockwise() { 1 } else { -1 };

        let (sin_amplitude, sin_omega, relative_time, sin_offset) = controller
            .variable_params()
            .map(|(a, omega, t)| (a, omega, t, 2.090 - a))
            .unwrap_or((0.0, 0.0, 0.0, 0.0));

        let mut target_activations = [0u8; 5];
        for (i, a) in mechanism.state().target_states().iter().enumerate() {
            if i < 5 {
                target_activations[i] = activation_to_u8(a);
            }
        }

        let idx = batch.rune_count as usize;
        batch.runes[idx] = GroundTruthRune {
            frame_seq,
            timestamp_ns,
            team: team_to_u8(&power_rune.team()),
            rune_mode: rune_mode_to_u8(&power_rune.mode()),
            mechanism_state: mechanism_state_to_u8(mechanism.state()),
            _pad1: 0,
            r_center_odom: [pos_ros.x, pos_ros.y, pos_ros.z],
            radius: 0.0,
            current_angle,
            v_roll: 0.0,
            direction,
            sin_amplitude,
            sin_omega,
            sin_phase: 0.0,
            sin_offset,
            relative_time,
            blade_id: -1,
            target_activations,
            _pad: [0; 20],
        };
        batch.rune_count += 1;
    }

    if let Ok(mut publisher) = ctx.publisher.try_lock() {
        publisher.publish_ground_truth(&batch, &exposure_state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn ros_yaw_uses_ros_z_component_after_alignment() {
        let yaw = 1.0_f32;
        let tf = GlobalTransform::from(Transform::from_rotation(Quat::from_rotation_y(yaw)));

        assert!((ros_yaw(&tf) - yaw).abs() < 1e-5);
        assert!(ros_rotation(&tf).dot(Quat::from_rotation_z(yaw)).abs() > 1.0 - 1e-5);
    }

    #[test]
    fn armor_geometry_has_deterministic_z4_slots_radii_and_height() {
        let samples = vec![
            ArmorGeometrySample {
                position: Vec3::new(0.30, 0.0, 0.10),
                visibility: GROUND_TRUTH_VISIBILITY_UNKNOWN,
            },
            ArmorGeometrySample {
                position: Vec3::new(0.0, 0.20, 0.12),
                visibility: GROUND_TRUTH_VISIBILITY_HIDDEN,
            },
            ArmorGeometrySample {
                position: Vec3::new(-0.30, 0.0, 0.14),
                visibility: GROUND_TRUTH_VISIBILITY_UNKNOWN,
            },
            ArmorGeometrySample {
                position: Vec3::new(0.0, -0.20, 0.16),
                visibility: GROUND_TRUTH_VISIBILITY_UNKNOWN,
            },
        ];

        let (count, radius_even, radius_odd, height, armors) = summarize_armor_geometry(samples);
        assert_eq!(count, 4);
        assert!((radius_even - 0.20).abs() < 1e-6);
        assert!((radius_odd - 0.30).abs() < 1e-6);
        assert!((height - 0.13).abs() < 1e-6);
        assert_eq!(armors.map(|armor| armor.relative_slot), [0, 1, 2, 3]);
        assert_eq!(armors[0].relative_position, [0.0, -0.20, 0.16]);
        assert!((armors[0].relative_yaw + std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert_eq!(armors[2].visibility, GROUND_TRUTH_VISIBILITY_HIDDEN);
    }

    #[test]
    fn nested_armor_roots_are_associated_only_with_their_vehicle() {
        let robot = Entity::from_raw_u32(1).unwrap();
        let base = Entity::from_raw_u32(2).unwrap();
        let armor_root = Entity::from_raw_u32(3).unwrap();
        let other_robot = Entity::from_raw_u32(4).unwrap();
        let parents = HashMap::from([(base, robot), (armor_root, base)]);

        assert!(belongs_to_via(armor_root, robot, |entity| parents
            .get(&entity)
            .copied()));
        assert!(!belongs_to_via(armor_root, other_robot, |entity| parents
            .get(&entity)
            .copied()));
    }

    #[test]
    fn only_complete_z4_geometry_is_truth_eligible() {
        assert!(!armor_geometry_is_truth_eligible(0));
        assert!(!armor_geometry_is_truth_eligible(1));
        assert!(!armor_geometry_is_truth_eligible(3));
        assert!(armor_geometry_is_truth_eligible(4));
    }

    #[cfg(feature = "distribution-release")]
    #[test]
    fn distribution_build_keeps_the_public_target_truth_batch_empty() {
        assert!(crate::distribution::is_locked());
        let batch = GroundTruthBatch::default();
        assert_eq!(batch.target_count, 0);
        assert_eq!(batch.rune_count, 0);
    }
}
