use crate::capture::{
    CameraFov, CaptureBundle, CaptureCamera, CaptureDepthPrepassEnabled, CaptureSource,
    ImageHandle, compute_camera_intrinsics,
    driver::{
        CaptureConfig, CapturedFrame, CapturedFrameKind, GpuCaptureHandler, SnapshotAsync,
        SnapshotSync,
    },
    setup_capture_camera, setup_preview_window, sync_capture_camera,
};
use crate::components::{Controlled, InfantryGimbal, InfantryLaunchOffset, SubscribeAutoAim};
use crate::dataset::prelude::DatasetSnapshotCreator;
use crate::systems::{ChassisObservationFrame, GameplaySystems};
use crate::talos::plugin::{to_ros_quat, to_ros_translation};
use crate::talos::tcp_image::{SubmitOutcome, TcpImagePublisher};
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy::render::{Extract, ExtractSchedule, RenderApp, RenderSystems};
use std::f32::consts::PI;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use talos_ipc::*;

static FRAME_SEQ: AtomicU64 = AtomicU64::new(0);
const TALOS_CAPTURE_MAX_HZ_ENV: &str = "DAEDALUS_TALOS_CAPTURE_MAX_HZ";

fn capture_max_hz_from_value(value: Option<&str>) -> Option<f64> {
    value
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|max_hz| max_hz.is_finite() && *max_hz > 0.0)
}

#[derive(Debug, Default)]
struct TalosCaptureCadenceState {
    period_s: Option<f64>,
    next_due_s: f64,
    initialized: bool,
    last_now_s: f64,
}

impl TalosCaptureCadenceState {
    fn new(max_hz: Option<f64>) -> Self {
        Self {
            period_s: max_hz.map(|max_hz| 1.0 / max_hz),
            ..default()
        }
    }

    fn should_capture(&mut self, now_s: f64) -> bool {
        let Some(period_s) = self.period_s else {
            return true;
        };
        if !self.initialized || !now_s.is_finite() || now_s < self.last_now_s {
            self.initialized = true;
            self.last_now_s = now_s;
            self.next_due_s = now_s + period_s;
            return true;
        }
        self.last_now_s = now_s;
        let tolerance_s = f64::EPSILON * now_s.abs().max(1.0) * 8.0;
        if now_s + tolerance_s < self.next_due_s {
            return false;
        }
        let periods_elapsed = ((now_s - self.next_due_s) / period_s).floor().max(0.0) + 1.0;
        self.next_due_s += periods_elapsed * period_s;
        true
    }
}

#[derive(Resource, Debug)]
struct TalosCaptureCadence {
    started: Instant,
    state: TalosCaptureCadenceState,
}

impl TalosCaptureCadence {
    fn from_env() -> Self {
        let max_hz =
            capture_max_hz_from_value(std::env::var(TALOS_CAPTURE_MAX_HZ_ENV).ok().as_deref());
        Self {
            started: Instant::now(),
            state: TalosCaptureCadenceState::new(max_hz),
        }
    }
}

#[derive(Resource, Debug, Clone, Copy)]
struct TalosCaptureActive(bool);

impl Default for TalosCaptureActive {
    fn default() -> Self {
        Self(true)
    }
}

fn update_talos_capture_cadence(
    mut cadence: ResMut<TalosCaptureCadence>,
    mut active: ResMut<TalosCaptureActive>,
    mut cameras: Query<&mut Camera, With<CaptureCamera>>,
) {
    let now_s = cadence.started.elapsed().as_secs_f64();
    active.0 = cadence.state.should_capture(now_s);
    for mut camera in &mut cameras {
        camera.is_active = active.0;
    }
}

#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct TalosFrameStamp {
    pub frame_seq: u64,
    pub timestamp_ns: u64,
}

pub fn advance_talos_frame_stamp(mut stamp: ResMut<TalosFrameStamp>) {
    stamp.frame_seq = FRAME_SEQ.fetch_add(1, Ordering::Relaxed);
    stamp.timestamp_ns = now_ns();
}

/// Extracted pose data from MainApp to RenderApp for synchronized publishing
#[derive(Resource, Clone, Default)]
pub struct ExtractedPoseData {
    pub frame_seq: u64,
    pub timestamp_ns: u64,
    pub valid: bool,
}

/// Pose data captured at frame snapshot time
#[derive(Clone)]
struct CapturedPoseData {
    gimbal_ros: [f32; 3],
    gimbal_quat: [f32; 4],
    muzzle_rel: [f32; 3],
    camera_rel: [f32; 3],
    chassis_observation: ChassisObservation,
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

struct TalosSnapshotSync {
    frame_seq: u64,
    timestamp_ns: u64,
    sink: TalosImageSink,
}

impl SnapshotSync for TalosSnapshotSync {
    fn accepts_owned_rgba(&self) -> bool {
        matches!(self.sink, TalosImageSink::Tcp(_))
    }

    fn captured(
        self: Box<Self>,
        world: &mut DeferredWorld,
        _config: &CaptureConfig,
    ) -> Box<dyn SnapshotAsync> {
        Box::new(TalosSnapshot {
            sink: match self.sink {
                TalosImageSink::File => {
                    TalosSnapshotSink::File(world.resource::<TalosCaptureContextShared>().0.clone())
                }
                TalosImageSink::Tcp(publisher) => TalosSnapshotSink::Tcp(publisher),
            },
            frame_seq: self.frame_seq,
            timestamp_ns: self.timestamp_ns,
        })
    }
}

struct TalosSnapshot {
    sink: TalosSnapshotSink,
    frame_seq: u64,
    timestamp_ns: u64,
}

enum TalosSnapshotSink {
    File(Arc<Mutex<ShmPublisher>>),
    Tcp(TcpImagePublisher),
}

impl SnapshotAsync for TalosSnapshot {
    fn captured(&mut self, frame: CapturedFrame<'_>) {
        let expected_size = match &self.sink {
            TalosSnapshotSink::File(_) if frame.kind == CapturedFrameKind::Rgb8 => {
                (frame.width * frame.height * 3) as usize
            }
            TalosSnapshotSink::Tcp(_) if frame.kind == CapturedFrameKind::Rgba8 => {
                (frame.width * frame.height * 4) as usize
            }
            _ => return,
        };
        if frame.data.len() != expected_size {
            warn!(
                "图像大小不匹配: expected {} bytes, got {} bytes",
                expected_size,
                frame.data.len()
            );
            return;
        }

        if frame.width != IMAGE_WIDTH || frame.height != IMAGE_HEIGHT {
            warn!(
                "image reesolution mismatched: expected {}x{}, got {}x{}",
                IMAGE_WIDTH, IMAGE_HEIGHT, frame.width, frame.height
            );
            return;
        }

        match &self.sink {
            TalosSnapshotSink::File(ctx) => {
                if let Ok(mut publisher) = ctx.try_lock() {
                    publisher.publish_image(frame.data, self.frame_seq, self.timestamp_ns);
                }
            }
            TalosSnapshotSink::Tcp(publisher) => {
                if let Err(error) = publisher.submit_rgba32(
                    frame.width,
                    frame.height,
                    self.frame_seq,
                    self.timestamp_ns,
                    frame.data,
                ) {
                    warn!("rejecting invalid Talos TCP image: {error}");
                }
            }
        }
    }

    fn accepts_owned_rgba(&self) -> bool {
        matches!(self.sink, TalosSnapshotSink::Tcp(_))
    }

    fn captured_owned_rgba(
        &mut self,
        width: u32,
        height: u32,
        data: Vec<u8>,
    ) -> Result<(), Vec<u8>> {
        let TalosSnapshotSink::Tcp(publisher) = &self.sink else {
            return Err(data);
        };
        match publisher.submit_rgba32_owned(width, height, self.frame_seq, self.timestamp_ns, data)
        {
            Ok(SubmitOutcome::Accepted | SubmitOutcome::Replaced | SubmitOutcome::Rejected) => {
                Ok(())
            }
            Err(error) => {
                let (_, data) = error.into_parts();
                Err(data)
            }
        }
    }
}

#[derive(Clone)]
pub(crate) enum TalosImageSink {
    File,
    Tcp(TcpImagePublisher),
}

struct TalosSnapshotCreator {
    sink: TalosImageSink,
}

impl GpuCaptureHandler for TalosSnapshotCreator {
    fn captured(&self, world: &World) -> Option<Box<dyn SnapshotSync>> {
        // Timestamp, frame sequence and pose must come from the same ExtractSchedule snapshot.
        let extracted = world.get_resource::<ExtractedPoseData>()?;
        if !extracted.valid {
            return None;
        }

        Some(Box::new(TalosSnapshotSync {
            frame_seq: extracted.frame_seq,
            timestamp_ns: extracted.timestamp_ns,
            sink: self.sink.clone(),
        }))
    }
}

struct CadenceGatedCaptureHandler {
    inner: Box<dyn GpuCaptureHandler>,
}

impl GpuCaptureHandler for CadenceGatedCaptureHandler {
    fn captured(&self, world: &World) -> Option<Box<dyn SnapshotSync>> {
        world
            .get_resource::<ExtractedPoseData>()
            .filter(|extracted| extracted.valid)?;
        self.inner.captured(world)
    }
}

fn cadence_gated_handler(inner: impl GpuCaptureHandler) -> Box<dyn GpuCaptureHandler> {
    Box::new(CadenceGatedCaptureHandler {
        inner: Box::new(inner),
    })
}

#[derive(Resource, Clone, Deref, DerefMut)]
pub struct TalosCaptureContextShared(pub Arc<Mutex<ShmPublisher>>);

#[derive(Resource, Clone)]
pub struct TalosCaptureContext {
    pub publisher: Arc<Mutex<ShmPublisher>>,
    pub fov_y: f32,
    pub(crate) image_sink: TalosImageSink,
}

pub struct TalosCapturePlugin {
    pub config: CaptureConfig,
    pub context: TalosCaptureContext,
}

pub fn publish_talos_pose_system(
    context: Option<Res<TalosCaptureContext>>,
    frame_stamp: Res<TalosFrameStamp>,
    camera: Query<&GlobalTransform, With<CaptureSource>>,
    gimbal: Query<&GlobalTransform, (With<Controlled>, With<InfantryGimbal>)>,
    muzzle_offset: Query<
        (&GlobalTransform, &Transform),
        (With<InfantryLaunchOffset>, With<Controlled>),
    >,
    chassis_obs: Res<ChassisObservationFrame>,
    following: Res<SubscribeAutoAim>,
) {
    let Some(ctx) = context else {
        return;
    };
    let Ok(cam_transform) = camera.single() else {
        return;
    };
    let Ok(gimbal_transform) = gimbal.single() else {
        return;
    };
    let Ok((muzzle_global, muzzle_local)) = muzzle_offset.single() else {
        return;
    };

    let pose = captured_pose_data(
        cam_transform,
        gimbal_transform,
        muzzle_global,
        muzzle_local,
        &chassis_obs,
        frame_stamp.frame_seq,
        frame_stamp.timestamp_ns,
    );

    if let Ok(mut publisher) = ctx.publisher.try_lock() {
        publish_pose_data(
            &mut publisher,
            frame_stamp.frame_seq,
            frame_stamp.timestamp_ns,
            &pose,
        );
        publisher.publish_runtime_state(RuntimeState {
            timestamp_ns: frame_stamp.timestamp_ns,
            following: u8::from(following.load(Ordering::Acquire)),
            _pad: [0; 55],
        });
    }
}

impl Plugin for TalosCapturePlugin {
    fn build(&self, app: &mut App) {
        let tcp = matches!(self.context.image_sink, TalosImageSink::Tcp(_));
        app.insert_resource(CaptureDepthPrepassEnabled(!tcp));
        let capture = if tcp {
            CaptureBundle::color(
                app,
                self.config.clone(),
                vec![cadence_gated_handler(TalosSnapshotCreator {
                    sink: self.context.image_sink.clone(),
                })],
            )
        } else {
            CaptureBundle::color_and_depth(
                app,
                self.config.clone(),
                vec![
                    cadence_gated_handler(TalosSnapshotCreator {
                        sink: self.context.image_sink.clone(),
                    }),
                    cadence_gated_handler(DatasetSnapshotCreator::default()),
                ],
                vec![cadence_gated_handler(DatasetSnapshotCreator::depth())],
            )
        };
        let render_target_handle = capture.color_target().unwrap().clone();

        {
            let mut publisher = self.context.publisher.lock().unwrap();
            let intrinsics = compute_camera_intrinsics(
                self.config.width,
                self.config.height,
                self.context.fov_y,
            );

            publisher.set_camera_info(CameraInfo {
                timestamp_ns: now_ns(),
                fx: intrinsics.fx,
                fy: intrinsics.fy,
                cx: intrinsics.cx,
                cy: intrinsics.cy,
                distortion: [0.0; 5],
                width: intrinsics.width,
                height: intrinsics.height,
                _pad: [0; 24],
            });
        }

        app.add_plugins(capture)
            .insert_resource(ImageHandle(render_target_handle))
            .insert_resource(CameraFov(self.context.fov_y))
            .insert_resource(self.context.clone())
            .insert_resource(TalosCaptureCadence::from_env())
            .init_resource::<TalosCaptureActive>()
            .add_systems(Startup, setup_capture_camera)
            .add_systems(Startup, setup_preview_window)
            .add_systems(
                Update,
                (sync_capture_camera, update_talos_capture_cadence)
                    .chain()
                    .after(GameplaySystems::Camera)
                    .before(RenderSystems::Render),
            );

        app.sub_app_mut(RenderApp)
            .insert_resource(TalosCaptureContextShared(self.context.publisher.clone()))
            .insert_resource(self.context.clone())
            .insert_resource(ExtractedPoseData::default())
            .add_systems(ExtractSchedule, extract_pose_data);
    }
}

/// Extract pose data from MainApp to RenderApp
fn extract_pose_data(
    mut pose_data: ResMut<ExtractedPoseData>,
    frame_stamp: Extract<Res<TalosFrameStamp>>,
    camera: Extract<Query<&GlobalTransform, With<CaptureSource>>>,
    gimbal: Extract<Query<&GlobalTransform, (With<Controlled>, With<InfantryGimbal>)>>,
    muzzle_offset: Extract<
        Query<(&GlobalTransform, &Transform), (With<InfantryLaunchOffset>, With<Controlled>)>,
    >,
    chassis_obs: Extract<Res<ChassisObservationFrame>>,
    capture_active: Extract<Res<TalosCaptureActive>>,
) {
    pose_data.frame_seq = frame_stamp.frame_seq;
    pose_data.timestamp_ns = frame_stamp.timestamp_ns;

    let Ok(cam_transform) = camera.single() else {
        pose_data.valid = false;
        return;
    };
    let Ok(gimbal_transform) = gimbal.single() else {
        pose_data.valid = false;
        return;
    };
    let Ok((muzzle_global, muzzle_local)) = muzzle_offset.single() else {
        pose_data.valid = false;
        return;
    };

    let _pose = captured_pose_data(
        cam_transform,
        gimbal_transform,
        muzzle_global,
        muzzle_local,
        &chassis_obs,
        pose_data.frame_seq,
        pose_data.timestamp_ns,
    );
    pose_data.valid = capture_active.0;
}

#[cfg(test)]
mod cadence_tests {
    use super::*;

    #[test]
    fn unlimited_invalid_and_rate_limited_modes_are_fail_safe() {
        assert_eq!(capture_max_hz_from_value(None), None);
        assert_eq!(capture_max_hz_from_value(Some("invalid")), None);
        assert_eq!(capture_max_hz_from_value(Some("180")), Some(180.0));
    }

    #[test]
    fn cadence_preserves_phase_without_catch_up_bursts() {
        let mut state = TalosCaptureCadenceState::new(Some(180.0));
        assert!(state.should_capture(0.0));
        assert!(!state.should_capture(0.001));
        assert!(state.should_capture(1.0 / 180.0));
        assert!(state.should_capture(1.0));
        assert!(!state.should_capture(1.0));
    }
}

fn captured_pose_data(
    cam_transform: &GlobalTransform,
    gimbal_transform: &GlobalTransform,
    muzzle_global: &GlobalTransform,
    muzzle_local: &Transform,
    chassis_obs: &ChassisObservationFrame,
    frame_seq: u64,
    timestamp_ns: u64,
) -> CapturedPoseData {
    let cam_rel = cam_transform.reparented_to(gimbal_transform);
    let muzzle_rel = muzzle_global.reparented_to(gimbal_transform);

    let gimbal_rot = gimbal_transform.rotation()
        * muzzle_local.rotation
        * Quat::from_euler(EulerRot::ZYX, 0.0, 0.0, PI / 2.0);

    let gimbal_ros = to_ros_translation(gimbal_transform.translation());
    let gimbal_rot = to_ros_quat(gimbal_rot);
    let muzzle = to_ros_translation(muzzle_rel.translation);
    let camera = to_ros_translation(cam_rel.translation);

    CapturedPoseData {
        gimbal_ros: [gimbal_ros.x, gimbal_ros.y, gimbal_ros.z],
        gimbal_quat: [gimbal_rot.w, gimbal_rot.x, gimbal_rot.y, gimbal_rot.z],
        muzzle_rel: [muzzle.x, muzzle.y, muzzle.z],
        camera_rel: [camera.x, camera.y, camera.z],
        chassis_observation: ChassisObservation {
            frame_seq,
            timestamp_ns,
            dt_s: chassis_obs.dt_s,
            v_body: [chassis_obs.v_body.x, chassis_obs.v_body.y],
            wz_radps: chassis_obs.wz_radps,
            wheel_linear_mps: chassis_obs.wheel_linear_mps,
            wheel_angular_radps: chassis_obs.wheel_angular_radps,
            a_body: [chassis_obs.a_body.x, chassis_obs.a_body.y],
            alpha_z_radps2: chassis_obs.alpha_z_radps2,
            rpy_rad: [
                chassis_obs.rpy_rad.x,
                chassis_obs.rpy_rad.y,
                chassis_obs.rpy_rad.z,
            ],
            gyro_xyz_radps: [
                chassis_obs.gyro_xyz_radps.x,
                chassis_obs.gyro_xyz_radps.y,
                chassis_obs.gyro_xyz_radps.z,
            ],
            accel_xyz_mps2: [
                chassis_obs.accel_xyz_mps2.x,
                chassis_obs.accel_xyz_mps2.y,
                chassis_obs.accel_xyz_mps2.z,
            ],
            _pad: [0; 16],
        },
    }
}

fn publish_pose_data(
    publisher: &mut ShmPublisher,
    frame_seq: u64,
    timestamp_ns: u64,
    pose: &CapturedPoseData,
) {
    publisher.publish_pose(
        PoseIndex::Odom,
        pose.gimbal_ros,
        [1.0, 0.0, 0.0, 0.0],
        frame_seq,
        timestamp_ns,
    );

    publisher.publish_pose(
        PoseIndex::Gimbal,
        [0.0, 0.0, 0.0],
        pose.gimbal_quat,
        frame_seq,
        timestamp_ns,
    );

    publisher.publish_pose(
        PoseIndex::Muzzle,
        pose.muzzle_rel,
        [1.0, 0.0, 0.0, 0.0],
        frame_seq,
        timestamp_ns,
    );

    publisher.publish_pose(
        PoseIndex::Camera,
        pose.camera_rel,
        [1.0, 0.0, 0.0, 0.0],
        frame_seq,
        timestamp_ns,
    );

    let mut observation = pose.chassis_observation;
    observation.frame_seq = frame_seq;
    observation.timestamp_ns = timestamp_ns;
    publisher.publish_chassis_observation(observation);

    // Legacy compatibility path for consumers still reading pose slot 4.
    publisher.publish_pose_with_aux(
        PoseIndex::ChassisObservation,
        [
            observation.v_body[0],
            observation.v_body[1],
            observation.wz_radps,
        ],
        observation.wheel_angular_radps,
        [
            observation.a_body[0],
            observation.a_body[1],
            observation.alpha_z_radps2,
            observation.dt_s,
        ],
        frame_seq,
        timestamp_ns,
    );
}
