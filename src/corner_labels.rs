use crate::capture::driver::CaptureConfig;
use crate::capture::{CaptureCamera, compute_camera_intrinsics};
use crate::components::ActiveSlapper;
use crate::robomaster::prelude::{Armor, ArmorParts, ArmorRoot, ArmorType, MarkerData};
use crate::setup::{ShootingRangeTarget, ShootingRangeTargetKind};
use crate::systems::{RangeTargetMotionMode, RangeTargetMotionSettings, ShootingRangeControlState};
use crate::talos::M_ALIGN_MAT3;
use crate::talos::capture::{TalosCaptureActive, TalosFrameStamp};
use crate::talos::tcp_image::TcpImageHeader;
use avian3d::prelude::{AngularVelocity, LinearVelocity};
use bevy::prelude::*;
use bevy::render::Extract;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub const CORNER_LABELS_ENV: &str = "DAEDALUS_CORNER_LABELS_JSONL";
pub const CORNER_LABEL_SCHEMA_VERSION: &str = "daedalus.offline-exact-corners/1";
pub const CORNER_LABEL_CAMERA_PROFILE: &str = "daedalus-camera-1440x1080-v2";
pub const SMALL_ARMOR_ASSET_SHA256: &str =
    "1cc0a3cd1ab05bc9822b616271db3afb64d078e56b9bbf452a8acc6d9bad0a6f";
pub const MOTION_UNIFORM_GUARD_NS: u64 = 100_000_000;
const MOTION_ENDPOINT_EPSILON_M: f32 = 0.001;
const MOTION_STATE_EPSILON: f32 = 1.0e-4;
const PROJECTION_W_EPSILON: f32 = 1.0e-6;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CornerLabelIntrinsics {
    pub model: String,
    pub fx: f64,
    pub fy: f64,
    pub cx: f64,
    pub cy: f64,
    pub distortion_model: String,
    pub distortion: [f64; 5],
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CornerLabelImage {
    pub width: u32,
    pub height: u32,
    pub pixel_format: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CornerLabelCamera {
    pub profile_id: String,
    pub intrinsics: CornerLabelIntrinsics,
    pub image: CornerLabelImage,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CornerLabelVisibility {
    pub classification: String,
    pub scene_hidden: bool,
    pub corners_in_frame: u8,
    pub occlusion_tested: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CornerLabelPlateGeometry {
    pub source: String,
    pub asset_sha256: String,
    pub armor_type: String,
    pub nominal_width_m: f64,
    pub nominal_height_m: f64,
    pub measured_width_m: f64,
    pub measured_height_m: f64,
    pub tilt_from_vertical_deg: f64,
    pub object_corners_armor_m: [[f64; 3]; 4],
    pub corner_order: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CornerLabelRecord {
    pub schema_version: String,
    pub producer_epoch: u64,
    pub frame_seq: u64,
    pub timestamp_ns: u64,
    pub camera: CornerLabelCamera,
    pub target_id: u64,
    pub target_number: u8,
    pub relative_slot: u8,
    pub armor_label: String,
    pub visibility: CornerLabelVisibility,
    pub exact_corners_px: [[f64; 2]; 4],
    pub plate_geometry: CornerLabelPlateGeometry,
    pub motion_uniform: bool,
    pub motion_uniform_guard_ns: u64,
    pub distance_m: f64,
    pub velocity_frame: String,
    pub linear_velocity_world_mps: [f64; 3],
    pub angular_velocity_world_rad_s: [f64; 3],
    pub future_truth_included: bool,
}

impl CornerLabelRecord {
    fn is_valid_for(&self, frame_seq: u64, timestamp_ns: u64, width: u32, height: u32) -> bool {
        self.schema_version == CORNER_LABEL_SCHEMA_VERSION
            && self.producer_epoch == 0
            && self.frame_seq == frame_seq
            && self.timestamp_ns == timestamp_ns
            && self.camera.image.width == width
            && self.camera.image.height == height
            && self.relative_slot < 4
            && !self.future_truth_included
            && self.camera.intrinsics.fx.is_finite()
            && self.camera.intrinsics.fy.is_finite()
            && self.camera.intrinsics.cx.is_finite()
            && self.camera.intrinsics.cy.is_finite()
            && self.distance_m.is_finite()
            && finite_2d(self.exact_corners_px)
            && finite_3d(self.plate_geometry.object_corners_armor_m)
            && self
                .linear_velocity_world_mps
                .iter()
                .chain(self.angular_velocity_world_rad_s.iter())
                .all(|value| value.is_finite())
    }
}

fn finite_2d(values: [[f64; 2]; 4]) -> bool {
    values.iter().flatten().all(|value| value.is_finite())
}

fn finite_3d(values: [[f64; 3]; 4]) -> bool {
    values.iter().flatten().all(|value| value.is_finite())
}

#[derive(Clone, Debug, Default)]
pub struct CornerLabelFrame {
    pub frame_seq: u64,
    pub timestamp_ns: u64,
    pub records: Vec<CornerLabelRecord>,
}

impl CornerLabelFrame {
    pub fn matches(&self, frame_seq: u64, timestamp_ns: u64) -> bool {
        self.frame_seq == frame_seq && self.timestamp_ns == timestamp_ns
    }
}

#[derive(Debug)]
pub struct CornerLabelJsonlWriter {
    producer_epoch: u64,
    path: PathBuf,
    file: Mutex<File>,
    failed: AtomicBool,
    rows_written: AtomicU64,
}

impl CornerLabelJsonlWriter {
    pub fn create_new(path: impl AsRef<Path>, producer_epoch: u64) -> io::Result<Arc<Self>> {
        if producer_epoch == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "corner-label producer epoch must be nonzero",
            ));
        }
        let path = path.as_ref();
        if path.as_os_str().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "corner-label path must not be empty",
            ));
        }
        if !path.is_absolute()
            || !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "corner-label path must be an absolute .jsonl file",
            ));
        }
        let parent = path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "corner-label path must have an existing parent directory",
            )
        })?;
        let resolved_parent = parent.canonicalize()?;
        let resolved_path = resolved_parent.join(path.file_name().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "corner-label file name is missing",
            )
        })?);
        if output_is_inside_executable_tree(&resolved_path) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "corner-label output must be outside the executable/Release tree",
            ));
        }
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&resolved_path)?;
        Ok(Arc::new(Self {
            producer_epoch,
            path: resolved_path,
            file: Mutex::new(file),
            failed: AtomicBool::new(false),
            rows_written: AtomicU64::new(0),
        }))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn rows_written(&self) -> u64 {
        self.rows_written.load(Ordering::Relaxed)
    }

    pub fn write_sent_frame(
        &self,
        header: &TcpImageHeader,
        labels: &CornerLabelFrame,
    ) -> io::Result<()> {
        if self.failed.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "corner-label writer is disabled after an earlier failure",
            ));
        }
        if header.producer_epoch != self.producer_epoch
            || !labels.matches(header.sequence, header.capture_timestamp_ns)
            || labels.records.is_empty()
            || labels.records.iter().any(|record| {
                !record.is_valid_for(
                    header.sequence,
                    header.capture_timestamp_ns,
                    header.width,
                    header.height,
                )
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "corner-label identity or payload does not match the sent TCP image",
            ));
        }

        let mut batch = Vec::new();
        for record in &labels.records {
            let mut record = record.clone();
            record.producer_epoch = self.producer_epoch;
            serde_json::to_writer(&mut batch, &record)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            batch.push(b'\n');
        }

        let mut file = self
            .file
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let result = file.write_all(&batch).and_then(|()| file.flush());
        match result {
            Ok(()) => {
                self.rows_written
                    .fetch_add(labels.records.len() as u64, Ordering::Relaxed);
                Ok(())
            }
            Err(error) => {
                self.failed.store(true, Ordering::Release);
                Err(error)
            }
        }
    }
}

fn output_is_inside_executable_tree(path: &Path) -> bool {
    let Ok(executable) = std::env::current_exe().and_then(|path| path.canonicalize()) else {
        return true;
    };
    let Some(executable_dir) = executable.parent() else {
        return true;
    };
    let install_root = if executable_dir
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("bin"))
    {
        executable_dir.parent().unwrap_or(executable_dir)
    } else {
        executable_dir
    };
    path.starts_with(install_root)
}

#[derive(Resource, Debug, Default)]
pub struct ExtractedCornerLabelData {
    pub frame: Option<CornerLabelFrame>,
    motion: HashMap<Entity, MotionTracker>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MotionSignature {
    mode: u8,
    direction: u32,
    speed: u32,
    span: u32,
    spin: u32,
    linear: [u32; 3],
    angular: [u32; 3],
}

#[derive(Clone, Copy, Debug)]
struct MotionTracker {
    signature: MotionSignature,
    changed_ns: u64,
    last_timestamp_ns: u64,
}

#[derive(Clone)]
struct ArmorSnapshot {
    armor_global: GlobalTransform,
    marker_global: GlobalTransform,
    marker_points: [Vec3; 4],
    armor_type: String,
    armor_label: String,
    scene_hidden: bool,
}

struct TargetSnapshot {
    entity: Entity,
    kind: ShootingRangeTargetKind,
    origin: Vec3,
    global: GlobalTransform,
    linear_velocity: Vec3,
    angular_velocity: Vec3,
    scene_hidden: bool,
    armors: Vec<ArmorSnapshot>,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn extract_corner_label_data(
    mut extracted: ResMut<ExtractedCornerLabelData>,
    frame_stamp: Extract<Res<TalosFrameStamp>>,
    capture_active: Extract<Res<TalosCaptureActive>>,
    targets: Extract<
        Query<(
            Entity,
            &ShootingRangeTarget,
            &GlobalTransform,
            &LinearVelocity,
            &AngularVelocity,
            Option<&ActiveSlapper>,
            Option<&Visibility>,
            Option<&InheritedVisibility>,
        )>,
    >,
    armors: Extract<
        Query<(
            Entity,
            &Armor,
            &ArmorRoot,
            &ArmorParts,
            &GlobalTransform,
            Option<&Visibility>,
            Option<&InheritedVisibility>,
        )>,
    >,
    markers: Extract<Query<(&GlobalTransform, &MarkerData)>>,
    parents: Extract<Query<&ChildOf>>,
    motion_state: Extract<Res<ShootingRangeControlState>>,
    camera: Extract<Single<(&Projection, &GlobalTransform), With<CaptureCamera>>>,
    config: Res<CaptureConfig>,
) {
    extracted.frame = None;
    if !capture_active.0 || frame_stamp.frame_seq == 0 || frame_stamp.timestamp_ns == 0 {
        return;
    }

    let mut by_target = HashMap::new();
    for (entity, target, global, linear, angular, active, visibility, inherited) in &targets {
        if active.is_none() {
            continue;
        }
        by_target.insert(
            entity,
            TargetSnapshot {
                entity,
                kind: target.kind,
                origin: target.origin,
                global: *global,
                linear_velocity: linear.0,
                angular_velocity: angular.0,
                scene_hidden: scene_hidden(visibility, inherited),
                armors: Vec::new(),
            },
        );
    }

    for (entity, armor, _root, parts, armor_global, visibility, inherited) in &armors {
        // Schema v1 is the approved 135 x 55 mm nominal small-armor training
        // surface. The #1 shooting-range target uses real 230 mm-class large
        // armor geometry and must not be mislabeled under this contract.
        if armor.spec.armor_type() != ArmorType::Small {
            continue;
        }
        let Some(target_entity) = find_ancestor_target(entity, &parents, &by_target) else {
            continue;
        };
        let Ok((marker_global, marker_data)) = markers.get(parts.marker()) else {
            continue;
        };
        let Some(target) = by_target.get_mut(&target_entity) else {
            continue;
        };
        target.armors.push(ArmorSnapshot {
            armor_global: *armor_global,
            marker_global: *marker_global,
            marker_points: marker_data.0,
            armor_type: format!("{:?}", armor.spec.armor_type()).to_ascii_lowercase(),
            armor_label: format!("{:?}", armor.label),
            scene_hidden: scene_hidden(visibility, inherited),
        });
    }

    let (projection, camera_global) = **camera;
    let Projection::Perspective(perspective) = projection else {
        return;
    };
    let intrinsics = compute_camera_intrinsics(config.width, config.height, perspective.fov);
    let camera_info = CornerLabelCamera {
        profile_id: CORNER_LABEL_CAMERA_PROFILE.to_string(),
        intrinsics: CornerLabelIntrinsics {
            model: "pinhole".to_string(),
            fx: intrinsics.fx,
            fy: intrinsics.fy,
            cx: intrinsics.cx,
            cy: intrinsics.cy,
            distortion_model: "plumb_bob".to_string(),
            distortion: [0.0; 5],
        },
        image: CornerLabelImage {
            width: config.width,
            height: config.height,
            pixel_format: "rgba32".to_string(),
        },
    };

    let mut records = Vec::new();
    let mut seen_targets = HashSet::new();
    for target in by_target.values_mut() {
        seen_targets.insert(target.entity);
        if target.armors.len() != 4 {
            continue;
        }
        target.armors.sort_by(|left, right| {
            armor_slot_angle(&target.global, &left.armor_global)
                .total_cmp(&armor_slot_angle(&target.global, &right.armor_global))
        });

        let settings = match target.kind {
            ShootingRangeTargetKind::Armor3 => &motion_state.armor_3,
            ShootingRangeTargetKind::Armor1 => &motion_state.armor_1,
        };
        let motion_uniform = update_motion_uniform(
            &mut extracted.motion,
            target,
            settings,
            frame_stamp.timestamp_ns,
        );
        let linear_ros = M_ALIGN_MAT3 * target.linear_velocity;
        let angular_ros = M_ALIGN_MAT3 * target.angular_velocity;

        let mut target_records = Vec::with_capacity(4);
        for (relative_slot, armor) in target.armors.iter().enumerate() {
            let world_points = armor
                .marker_points
                .map(|point| armor.marker_global.transform_point(point));
            let Some(screen_points) = project_world_corners(
                world_points,
                camera_global,
                projection,
                config.width,
                config.height,
            ) else {
                continue;
            };
            let Some(order) = screen_corner_order(screen_points) else {
                continue;
            };
            let exact_corners_px = order.map(|index| {
                let point = screen_points[index];
                [point.x as f64, point.y as f64]
            });
            let armor_inverse = armor.armor_global.affine().inverse();
            let object_corners_armor_m = order.map(|index| {
                let point = armor_inverse.transform_point3(world_points[index]);
                [point.x as f64, point.y as f64, point.z as f64]
            });
            let (measured_width_m, measured_height_m) = measured_plate_size(object_corners_armor_m);
            let marker_center_world = world_points.into_iter().sum::<Vec3>() * 0.25;
            let target_normal = target
                .global
                .affine()
                .inverse()
                .transform_vector3(armor.armor_global.affine().transform_vector3(Vec3::Y));
            let tilt_from_vertical_deg = target_normal
                .normalize_or_zero()
                .y
                .abs()
                .asin()
                .to_degrees() as f64;
            let corners_in_frame = exact_corners_px
                .iter()
                .filter(|point| {
                    point[0] >= 0.0
                        && point[0] < config.width as f64
                        && point[1] >= 0.0
                        && point[1] < config.height as f64
                })
                .count() as u8;
            let scene_hidden = target.scene_hidden || armor.scene_hidden;
            let classification = if scene_hidden {
                "scene_hidden"
            } else if corners_in_frame == 4 {
                "fully_in_frame"
            } else if corners_in_frame > 0 {
                "partially_in_frame"
            } else {
                "out_of_frame"
            };
            target_records.push(CornerLabelRecord {
                schema_version: CORNER_LABEL_SCHEMA_VERSION.to_string(),
                producer_epoch: 0,
                frame_seq: frame_stamp.frame_seq,
                timestamp_ns: frame_stamp.timestamp_ns,
                camera: camera_info.clone(),
                target_id: target.entity.to_bits(),
                target_number: target.kind.number(),
                relative_slot: relative_slot as u8,
                armor_label: armor.armor_label.clone(),
                visibility: CornerLabelVisibility {
                    classification: classification.to_string(),
                    scene_hidden,
                    corners_in_frame,
                    occlusion_tested: false,
                },
                exact_corners_px,
                plate_geometry: CornerLabelPlateGeometry {
                    source: "asset_marker_mesh".to_string(),
                    asset_sha256: SMALL_ARMOR_ASSET_SHA256.to_string(),
                    armor_type: armor.armor_type.clone(),
                    nominal_width_m: 0.135,
                    nominal_height_m: 0.055,
                    measured_width_m,
                    measured_height_m,
                    tilt_from_vertical_deg,
                    object_corners_armor_m,
                    corner_order: "bl,tl,tr,br".to_string(),
                },
                motion_uniform,
                motion_uniform_guard_ns: MOTION_UNIFORM_GUARD_NS,
                distance_m: camera_global.translation().distance(marker_center_world) as f64,
                velocity_frame: "ros_odom".to_string(),
                linear_velocity_world_mps: vec3_f64(linear_ros),
                angular_velocity_world_rad_s: vec3_f64(angular_ros),
                future_truth_included: false,
            });
        }

        // Schema v1 is a strict Z4 training contract. A single marker that is
        // behind the camera, non-finite, or screen-degenerate makes this
        // target exposure ambiguous for consumers; drop the whole exposure
        // rather than serializing a misleading partial target.
        if complete_z4_records(&target_records) {
            records.extend(target_records);
        }
    }
    extracted
        .motion
        .retain(|entity, _| seen_targets.contains(entity));

    if !records.is_empty() {
        extracted.frame = Some(CornerLabelFrame {
            frame_seq: frame_stamp.frame_seq,
            timestamp_ns: frame_stamp.timestamp_ns,
            records,
        });
    }
}

fn complete_z4_records(records: &[CornerLabelRecord]) -> bool {
    records.len() == 4
        && records.iter().all(|record| record.target_number == 3)
        && records
            .iter()
            .map(|record| record.relative_slot)
            .collect::<HashSet<_>>()
            == HashSet::from([0, 1, 2, 3])
}

fn find_ancestor_target(
    entity: Entity,
    parents: &Query<&ChildOf>,
    targets: &HashMap<Entity, TargetSnapshot>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if targets.contains_key(&current) {
            return Some(current);
        }
        current = parents.get(current).ok()?.parent();
    }
}

fn scene_hidden(visibility: Option<&Visibility>, inherited: Option<&InheritedVisibility>) -> bool {
    matches!(visibility, Some(Visibility::Hidden))
        || matches!(inherited, Some(inherited) if !inherited.get())
}

fn armor_slot_angle(target: &GlobalTransform, armor: &GlobalTransform) -> f32 {
    let target_local = target
        .affine()
        .inverse()
        .transform_point3(armor.translation());
    let ros = M_ALIGN_MAT3 * target_local;
    ros.y.atan2(ros.x)
}

fn project_world_corners(
    points: [Vec3; 4],
    camera: &GlobalTransform,
    projection: &Projection,
    width: u32,
    height: u32,
) -> Option<[Vec2; 4]> {
    let clip_from_world = projection.get_clip_from_view() * camera.to_matrix().inverse();
    let mut output = [Vec2::ZERO; 4];
    for (index, point) in points.into_iter().enumerate() {
        let clip = clip_from_world * point.extend(1.0);
        if !clip.is_finite() || clip.w <= PROJECTION_W_EPSILON {
            return None;
        }
        let ndc = clip.xyz() / clip.w;
        let screen = Vec2::new(
            (ndc.x + 1.0) * 0.5 * width as f32,
            (1.0 - ndc.y) * 0.5 * height as f32,
        );
        if !screen.is_finite() {
            return None;
        }
        output[index] = screen;
    }
    Some(output)
}

/// Return source indices in the public screen-canonical order bl,tl,tr,br.
fn screen_corner_order(points: [Vec2; 4]) -> Option<[usize; 4]> {
    if points.iter().any(|point| !point.is_finite()) {
        return None;
    }
    for left in 0..4 {
        for right in (left + 1)..4 {
            if points[left].distance_squared(points[right]) <= 1.0e-6 {
                return None;
            }
        }
    }
    let mut left_to_right = [0usize, 1, 2, 3];
    left_to_right.sort_by(|left, right| {
        points[*left]
            .x
            .total_cmp(&points[*right].x)
            .then_with(|| points[*left].y.total_cmp(&points[*right].y))
    });
    let mut left = [left_to_right[0], left_to_right[1]];
    let mut right = [left_to_right[2], left_to_right[3]];
    if points[left[0]].x.max(points[left[1]].x) + 1.0e-3
        >= points[right[0]].x.min(points[right[1]].x)
    {
        return None;
    }
    left.sort_by(|top, bottom| points[*top].y.total_cmp(&points[*bottom].y));
    right.sort_by(|top, bottom| points[*top].y.total_cmp(&points[*bottom].y));
    if points[left[1]].y - points[left[0]].y <= 1.0e-3
        || points[right[1]].y - points[right[0]].y <= 1.0e-3
    {
        return None;
    }
    let order = [left[1], left[0], right[0], right[1]];

    let polygon = order.map(|index| points[index]);
    let crosses = [0, 1, 2, 3].map(|index| {
        let current = polygon[index];
        let next = polygon[(index + 1) % 4];
        let after = polygon[(index + 2) % 4];
        (next - current).perp_dot(after - next)
    });
    crosses
        .iter()
        .all(|cross| cross.is_finite() && *cross > 1.0e-3)
        .then_some(order)
}

fn measured_plate_size(points: [[f64; 3]; 4]) -> (f64, f64) {
    let distance = |left: [f64; 3], right: [f64; 3]| {
        ((left[0] - right[0]).powi(2) + (left[1] - right[1]).powi(2) + (left[2] - right[2]).powi(2))
            .sqrt()
    };
    // `bl,tl,tr,br` is a *screen* canonical order. Depending on the camera
    // viewpoint, its vertical screen edges can be either physical marker
    // width or physical marker height. Preserve the audited physical naming
    // by deriving the two opposing-edge spans and choosing the long pair as
    // small-armor width (the actual asset is non-square).
    let span_a = (distance(points[0], points[1]) + distance(points[2], points[3])) * 0.5;
    let span_b = (distance(points[0], points[3]) + distance(points[1], points[2])) * 0.5;
    if span_a >= span_b {
        (span_a, span_b)
    } else {
        (span_b, span_a)
    }
}

fn motion_signature(
    settings: &RangeTargetMotionSettings,
    linear: Vec3,
    angular: Vec3,
) -> MotionSignature {
    MotionSignature {
        mode: match settings.mode {
            RangeTargetMotionMode::Stationary => 0,
            RangeTargetMotionMode::Linear => 1,
            RangeTargetMotionMode::Spin => 2,
            RangeTargetMotionMode::LinearAndSpin => 3,
        },
        direction: settings.direction_deg.to_bits(),
        speed: settings.linear_speed_mps.to_bits(),
        span: settings.linear_span_m.to_bits(),
        spin: settings.spin_deg_s.to_bits(),
        linear: linear.to_array().map(f32::to_bits),
        angular: angular.to_array().map(f32::to_bits),
    }
}

fn update_motion_uniform(
    trackers: &mut HashMap<Entity, MotionTracker>,
    target: &TargetSnapshot,
    settings: &RangeTargetMotionSettings,
    timestamp_ns: u64,
) -> bool {
    if timestamp_ns == 0
        || !target.linear_velocity.is_finite()
        || !target.angular_velocity.is_finite()
        || !target.global.translation().is_finite()
    {
        return false;
    }
    let signature = motion_signature(settings, target.linear_velocity, target.angular_velocity);
    let tracker = trackers.entry(target.entity).or_insert(MotionTracker {
        signature,
        changed_ns: timestamp_ns,
        last_timestamp_ns: timestamp_ns,
    });
    if timestamp_ns <= tracker.last_timestamp_ns {
        tracker.signature = signature;
        tracker.changed_ns = timestamp_ns;
        tracker.last_timestamp_ns = timestamp_ns;
        return false;
    }
    if tracker.signature != signature {
        tracker.signature = signature;
        tracker.changed_ns = timestamp_ns;
    }
    tracker.last_timestamp_ns = timestamp_ns;
    let stable_long_enough =
        timestamp_ns.saturating_sub(tracker.changed_ns) > MOTION_UNIFORM_GUARD_NS;
    stable_long_enough && instantaneous_motion_is_uniform(target, settings)
}

fn instantaneous_motion_is_uniform(
    target: &TargetSnapshot,
    settings: &RangeTargetMotionSettings,
) -> bool {
    let heading = settings.direction_deg.to_radians();
    let axis = Vec3::new(heading.sin(), 0.0, heading.cos()).normalize_or_zero();
    let has_linear = matches!(
        settings.mode,
        RangeTargetMotionMode::Linear | RangeTargetMotionMode::LinearAndSpin
    );
    let has_spin = matches!(
        settings.mode,
        RangeTargetMotionMode::Spin | RangeTargetMotionMode::LinearAndSpin
    );
    let expected_speed = if has_linear {
        settings.linear_speed_mps.max(0.0)
    } else {
        0.0
    };
    let expected_angular = if has_spin {
        Vec3::Y * settings.spin_deg_s.to_radians()
    } else {
        Vec3::ZERO
    };
    if (target.angular_velocity - expected_angular).length() > MOTION_STATE_EPSILON {
        return false;
    }
    if expected_speed <= MOTION_STATE_EPSILON {
        return target.linear_velocity.length() <= MOTION_STATE_EPSILON;
    }
    if axis == Vec3::ZERO
        || (target.linear_velocity.length() - expected_speed).abs() > MOTION_STATE_EPSILON
        || target.linear_velocity.reject_from_normalized(axis).length() > MOTION_STATE_EPSILON
    {
        return false;
    }
    let half_span = settings.linear_span_m.clamp(0.0, 8.0) * 0.5;
    let offset = (target.global.translation() - target.origin).dot(axis);
    if !half_span.is_finite() || offset.abs() > half_span + MOTION_ENDPOINT_EPSILON_M {
        return false;
    }
    let nearest_endpoint_m = (half_span - offset.abs()).max(0.0);
    let guard_m =
        expected_speed * MOTION_UNIFORM_GUARD_NS as f32 * 1.0e-9 + MOTION_ENDPOINT_EPSILON_M;
    nearest_endpoint_m > guard_m
}

fn vec3_f64(value: Vec3) -> [f64; 3] {
    [value.x as f64, value.y as f64, value.z as f64]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::talos::tcp_image::{
        HEADER_BYTES, PixelFormat, TcpImageSender, TcpImageSenderConfig,
    };
    use std::fs;
    use std::io::Read;
    use std::net::{SocketAddr, TcpStream};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "daedalus-{name}-{}-{unique}.jsonl",
            std::process::id()
        ))
    }

    fn sample_record() -> CornerLabelRecord {
        CornerLabelRecord {
            schema_version: CORNER_LABEL_SCHEMA_VERSION.to_string(),
            producer_epoch: 0,
            frame_seq: 7,
            timestamp_ns: 11,
            camera: CornerLabelCamera {
                profile_id: CORNER_LABEL_CAMERA_PROFILE.to_string(),
                intrinsics: CornerLabelIntrinsics {
                    model: "pinhole".to_string(),
                    fx: 100.0,
                    fy: 100.0,
                    cx: 50.0,
                    cy: 50.0,
                    distortion_model: "plumb_bob".to_string(),
                    distortion: [0.0; 5],
                },
                image: CornerLabelImage {
                    width: 100,
                    height: 100,
                    pixel_format: "rgba32".to_string(),
                },
            },
            target_id: 3,
            target_number: 3,
            relative_slot: 0,
            armor_label: "InfantryOrHeroThree".to_string(),
            visibility: CornerLabelVisibility {
                classification: "fully_in_frame".to_string(),
                scene_hidden: false,
                corners_in_frame: 4,
                occlusion_tested: false,
            },
            exact_corners_px: [[10.0, 60.0], [10.0, 40.0], [90.0, 40.0], [90.0, 60.0]],
            plate_geometry: CornerLabelPlateGeometry {
                source: "asset_marker_mesh".to_string(),
                asset_sha256: SMALL_ARMOR_ASSET_SHA256.to_string(),
                armor_type: "small".to_string(),
                nominal_width_m: 0.135,
                nominal_height_m: 0.055,
                measured_width_m: 0.1338,
                measured_height_m: 0.0539,
                tilt_from_vertical_deg: 15.0,
                object_corners_armor_m: [
                    [-0.0669, 0.0027, -0.0270],
                    [-0.0669, 0.0027, 0.0269],
                    [0.0669, 0.0027, 0.0269],
                    [0.0669, 0.0027, -0.0270],
                ],
                corner_order: "bl,tl,tr,br".to_string(),
            },
            motion_uniform: true,
            motion_uniform_guard_ns: MOTION_UNIFORM_GUARD_NS,
            distance_m: 5.0,
            velocity_frame: "ros_odom".to_string(),
            linear_velocity_world_mps: [1.0, 0.0, 0.0],
            angular_velocity_world_rad_s: [0.0, 0.0, 1.0],
            future_truth_included: false,
        }
    }

    #[test]
    fn screen_order_is_bl_tl_tr_br_and_rejects_degenerate_points() {
        let points = [
            Vec2::new(90.0, 40.0),
            Vec2::new(10.0, 60.0),
            Vec2::new(90.0, 60.0),
            Vec2::new(10.0, 40.0),
        ];
        let order = screen_corner_order(points).unwrap();
        assert_eq!(
            order.map(|index| points[index]),
            [
                Vec2::new(10.0, 60.0),
                Vec2::new(10.0, 40.0),
                Vec2::new(90.0, 40.0),
                Vec2::new(90.0, 60.0),
            ]
        );
        assert!(screen_corner_order([Vec2::ZERO; 4]).is_none());
        assert!(
            screen_corner_order([
                Vec2::new(10.0, 60.0),
                Vec2::new(10.0, 40.0),
                Vec2::new(50.0, 50.0),
                Vec2::new(90.0, 60.0),
            ])
            .is_none()
        );
    }

    #[test]
    fn every_input_permutation_has_the_same_screen_order() {
        let canonical = [
            Vec2::new(11.0, 62.0),
            Vec2::new(13.0, 38.0),
            Vec2::new(91.0, 40.0),
            Vec2::new(89.0, 60.0),
        ];
        let mut count = 0;
        for a in 0..4 {
            for b in 0..4 {
                for c in 0..4 {
                    for d in 0..4 {
                        let indexes = [a, b, c, d];
                        if indexes.iter().copied().collect::<HashSet<_>>().len() != 4 {
                            continue;
                        }
                        let points = indexes.map(|index| canonical[index]);
                        let order = screen_corner_order(points).unwrap();
                        assert_eq!(order.map(|index| points[index]), canonical);
                        count += 1;
                    }
                }
            }
        }
        assert_eq!(count, 24);
    }

    #[test]
    fn projection_keeps_subpixels_outside_the_frame_and_rejects_behind_camera() {
        let projection = Projection::Perspective(PerspectiveProjection {
            fov: 45.0_f32.to_radians(),
            aspect_ratio: 4.0 / 3.0,
            ..default()
        });
        let camera = GlobalTransform::IDENTITY;
        let points = [
            Vec3::new(-10.0, -1.0, -5.0),
            Vec3::new(-10.0, 1.0, -5.0),
            Vec3::new(1.0, 1.0, -5.0),
            Vec3::new(1.0, -1.0, -5.0),
        ];
        let projected = project_world_corners(points, &camera, &projection, 1440, 1080).unwrap();
        assert!(projected[0].x < 0.0);
        assert!(projected.iter().all(|point| point.is_finite()));
        assert!(
            project_world_corners(
                points.map(|mut point| {
                    point.z = 5.0;
                    point
                }),
                &camera,
                &projection,
                1440,
                1080,
            )
            .is_none()
        );
    }

    #[test]
    fn measured_plate_size_uses_the_exported_true_quadrilateral() {
        let record = sample_record();
        let (width, height) = measured_plate_size(record.plate_geometry.object_corners_armor_m);
        assert!((width - 0.1338).abs() < 1.0e-4);
        assert!((height - 0.0539).abs() < 1.0e-4);

        // Regression: the public bl,tl,tr,br order must not use the old
        // edge pairing and swap width with height when a plate faces another
        // camera direction.
        assert!(width > height);

        let rotated_screen_order = [
            record.plate_geometry.object_corners_armor_m[1],
            record.plate_geometry.object_corners_armor_m[2],
            record.plate_geometry.object_corners_armor_m[3],
            record.plate_geometry.object_corners_armor_m[0],
        ];
        let (rotated_width, rotated_height) = measured_plate_size(rotated_screen_order);
        assert!((rotated_width - width).abs() < 1.0e-9);
        assert!((rotated_height - height).abs() < 1.0e-9);
    }

    #[test]
    fn labels_fail_closed_when_an_exposure_is_not_complete_z4() {
        let base = sample_record();
        let complete = (0..4)
            .map(|relative_slot| {
                let mut record = base.clone();
                record.relative_slot = relative_slot;
                record
            })
            .collect::<Vec<_>>();
        assert!(complete_z4_records(&complete));

        let partial = complete[..3].to_vec();
        assert!(!complete_z4_records(&partial));

        let mut duplicate_slot = complete.clone();
        duplicate_slot[3].relative_slot = 2;
        assert!(!complete_z4_records(&duplicate_slot));
    }

    #[test]
    fn jsonl_writer_is_create_new_and_rejects_identity_mismatch() {
        let path = temp_path("corner-label-writer-test");
        let writer = CornerLabelJsonlWriter::create_new(&path, 5).unwrap();
        assert!(CornerLabelJsonlWriter::create_new(&path, 5).is_err());
        let header = TcpImageHeader::new(PixelFormat::Rgba32, 100, 100, 5, 7, 11).unwrap();
        let labels = CornerLabelFrame {
            frame_seq: 7,
            timestamp_ns: 11,
            records: vec![sample_record()],
        };
        writer.write_sent_frame(&header, &labels).unwrap();
        assert_eq!(writer.rows_written(), 1);
        let row: CornerLabelRecord =
            serde_json::from_str(fs::read_to_string(&path).unwrap().trim()).unwrap();
        assert_eq!(row.producer_epoch, 5);
        assert!(!row.future_truth_included);

        let wrong_header = TcpImageHeader::new(PixelFormat::Rgba32, 100, 100, 5, 8, 11).unwrap();
        assert!(writer.write_sent_frame(&wrong_header, &labels).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn motion_uniform_excludes_initialization_and_both_sides_of_a_reversal() {
        let settings = RangeTargetMotionSettings {
            mode: RangeTargetMotionMode::Linear,
            direction_deg: 90.0,
            linear_speed_mps: 1.0,
            linear_span_m: 8.0,
            ..default()
        };
        let mut target = TargetSnapshot {
            entity: Entity::from_raw_u32(7).unwrap(),
            kind: ShootingRangeTargetKind::Armor3,
            origin: Vec3::ZERO,
            global: GlobalTransform::from_translation(Vec3::ZERO),
            linear_velocity: Vec3::X,
            angular_velocity: Vec3::ZERO,
            scene_hidden: false,
            armors: Vec::new(),
        };
        let mut trackers = HashMap::new();
        let start = 1_000_000_000;
        assert!(!update_motion_uniform(
            &mut trackers,
            &target,
            &settings,
            start
        ));
        assert!(update_motion_uniform(
            &mut trackers,
            &target,
            &settings,
            start + MOTION_UNIFORM_GUARD_NS + 1
        ));

        target.global = GlobalTransform::from_translation(Vec3::new(3.95, 0.0, 0.0));
        assert!(!update_motion_uniform(
            &mut trackers,
            &target,
            &settings,
            start + MOTION_UNIFORM_GUARD_NS * 2
        ));

        target.global = GlobalTransform::from_translation(Vec3::new(3.90, 0.0, 0.0));
        target.linear_velocity = Vec3::NEG_X;
        let reversed = start + MOTION_UNIFORM_GUARD_NS * 3;
        assert!(!update_motion_uniform(
            &mut trackers,
            &target,
            &settings,
            reversed
        ));
        target.global = GlobalTransform::from_translation(Vec3::new(3.70, 0.0, 0.0));
        assert!(update_motion_uniform(
            &mut trackers,
            &target,
            &settings,
            reversed + MOTION_UNIFORM_GUARD_NS + 1
        ));
    }

    #[test]
    fn labels_are_written_only_after_the_complete_tcp_frame_is_received() {
        let path = temp_path("corner-label-tcp-complete");
        let writer = CornerLabelJsonlWriter::create_new(&path, 23).unwrap();
        let mut config = TcpImageSenderConfig::new(SocketAddr::from(([127, 0, 0, 1], 0)), 23);
        config.corner_label_writer = Some(writer.clone());
        let sender = TcpImageSender::bind(config).unwrap();
        let mut client = TcpStream::connect(sender.local_addr()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let connected_deadline = Instant::now() + Duration::from_secs(2);
        while sender.counters().connect_total == 0 && Instant::now() < connected_deadline {
            std::thread::sleep(Duration::from_millis(5));
        }

        let mut record = sample_record();
        record.camera.image.width = 2;
        record.camera.image.height = 1;
        let labels = CornerLabelFrame {
            frame_seq: 7,
            timestamp_ns: 11,
            records: vec![record],
        };
        sender
            .publisher()
            .submit_rgba32_owned_with_corner_labels(
                2,
                1,
                7,
                11,
                vec![1, 2, 3, 4, 5, 6, 7, 8],
                Some(labels),
            )
            .unwrap();
        let mut wire = vec![0; HEADER_BYTES + 8];
        client.read_exact(&mut wire).unwrap();
        let written_deadline = Instant::now() + Duration::from_secs(2);
        while writer.rows_written() == 0 && Instant::now() < written_deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(writer.rows_written(), 1);
        let row: CornerLabelRecord =
            serde_json::from_str(fs::read_to_string(&path).unwrap().trim()).unwrap();
        assert_eq!(
            (row.producer_epoch, row.frame_seq, row.timestamp_ns),
            (23, 7, 11)
        );
        assert_eq!(u64::from_be_bytes(wire[24..32].try_into().unwrap()), 23);
        assert_eq!(u64::from_be_bytes(wire[32..40].try_into().unwrap()), 7);
        assert_eq!(u64::from_be_bytes(wire[40..48].try_into().unwrap()), 11);
        drop(client);
        drop(sender);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn no_client_and_mailbox_replacement_never_write_orphan_labels() {
        let path = temp_path("corner-label-tcp-replaced");
        let writer = CornerLabelJsonlWriter::create_new(&path, 29).unwrap();
        let mut config = TcpImageSenderConfig::new(SocketAddr::from(([127, 0, 0, 1], 0)), 29);
        config.corner_label_writer = Some(writer.clone());
        let sender = TcpImageSender::bind(config).unwrap();

        for sequence in [7, 8] {
            let mut record = sample_record();
            record.frame_seq = sequence;
            record.camera.image.width = 2;
            record.camera.image.height = 1;
            let labels = CornerLabelFrame {
                frame_seq: sequence,
                timestamp_ns: 11,
                records: vec![record],
            };
            sender
                .publisher()
                .submit_rgba32_owned_with_corner_labels(
                    2,
                    1,
                    sequence,
                    11,
                    vec![0; 8],
                    Some(labels),
                )
                .unwrap();
        }
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(writer.rows_written(), 0);
        assert!(sender.counters().replaced_total >= 1);
        drop(sender);
        assert!(fs::read_to_string(&path).unwrap().is_empty());
        fs::remove_file(path).unwrap();
    }
}
