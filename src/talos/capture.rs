use crate::capture::{
    CameraFov, CaptureBundle, CaptureCamera, CaptureDepthPrepassEnabled, CaptureSource,
    ExposureWallTimestamp, ImageHandle, compute_camera_intrinsics,
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

static FRAME_SEQ: AtomicU64 = AtomicU64::new(1);
static TALOS_PUBLISHED_FRAMES: AtomicU64 = AtomicU64::new(0);
static TALOS_PUBLISH_LOCK_DROPS: AtomicU64 = AtomicU64::new(0);
const TALOS_RGB_ONLY_ENV: &str = "DAEDALUS_TALOS_RGB_ONLY";
const TALOS_CAPTURE_MAX_HZ_ENV: &str = "DAEDALUS_TALOS_CAPTURE_MAX_HZ";

fn env_flag_enabled(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn talos_rgb_only_capture_enabled() -> bool {
    std::env::var(TALOS_RGB_ONLY_ENV)
        .map(|value| env_flag_enabled(&value))
        .unwrap_or(false)
}

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

        // A small scale-relative tolerance avoids losing an exact boundary to
        // floating-point representation without creating a second admission.
        let tolerance_s = f64::EPSILON * now_s.abs().max(1.0) * 8.0;
        if now_s + tolerance_s < self.next_due_s {
            return false;
        }

        // Preserve the original phase when one or more source periods were
        // missed. This deliberately admits at most this one current tick.
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
        if let Some(max_hz) = max_hz {
            info!("Talos source capture cadence limited to {max_hz:.3} Hz");
        }
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

fn talos_frame_dimensions_valid(width: u32, height: u32) -> bool {
    width > 0 && height > 0 && width <= IMAGE_WIDTH && height <= IMAGE_HEIGHT
}

#[derive(Resource, Debug, Clone, Copy)]
pub struct TalosFrameStamp {
    pub frame_seq: u64,
    pub timestamp_ns: u64,
}

impl Default for TalosFrameStamp {
    fn default() -> Self {
        Self {
            frame_seq: FRAME_SEQ.fetch_add(1, Ordering::Relaxed),
            timestamp_ns: now_ns().max(1),
        }
    }
}

pub fn advance_talos_frame_stamp(
    mut stamp: ResMut<TalosFrameStamp>,
    exposure_stamp: Res<ExposureWallTimestamp>,
) {
    stamp.frame_seq = FRAME_SEQ.fetch_add(1, Ordering::Relaxed);
    stamp.timestamp_ns = exposure_stamp.timestamp_ns.max(1);
}

pub fn talos_published_frames() -> u64 {
    TALOS_PUBLISHED_FRAMES.load(Ordering::Relaxed)
}

pub fn talos_publish_lock_drops() -> u64 {
    TALOS_PUBLISH_LOCK_DROPS.load(Ordering::Relaxed)
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
        let sink = match self.sink {
            TalosImageSink::File => {
                TalosSnapshotSink::File(world.resource::<TalosCaptureContextShared>().0.clone())
            }
            TalosImageSink::Tcp(publisher) => TalosSnapshotSink::Tcp(publisher),
        };

        Box::new(TalosSnapshot {
            sink,
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
        if !talos_frame_dimensions_valid(frame.width, frame.height) {
            warn!(
                "image resolution out of range: maximum {}x{}, got {}x{}",
                IMAGE_WIDTH, IMAGE_HEIGHT, frame.width, frame.height
            );
            return;
        }

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

        match &self.sink {
            TalosSnapshotSink::File(ctx) => match ctx.try_lock() {
                Ok(mut publisher) => {
                    publisher.publish_sized_image(
                        frame.data,
                        frame.width,
                        frame.height,
                        self.frame_seq,
                        self.timestamp_ns,
                    );
                    TALOS_PUBLISHED_FRAMES.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    TALOS_PUBLISH_LOCK_DROPS.fetch_add(1, Ordering::Relaxed);
                }
            },
            TalosSnapshotSink::Tcp(publisher) => match publisher.submit_rgba32(
                frame.width,
                frame.height,
                self.frame_seq,
                self.timestamp_ns,
                frame.data,
            ) {
                Ok(SubmitOutcome::Accepted | SubmitOutcome::Replaced | SubmitOutcome::Rejected) => {
                }
                Err(error) => warn!("rejecting invalid Talos TCP image: {error}"),
            },
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
        if !talos_frame_dimensions_valid(width, height) {
            return Err(data);
        }
        let expected_size = (width * height * 4) as usize;
        if data.len() != expected_size {
            return Err(data);
        }

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

impl TalosImageSink {
    fn is_tcp(&self) -> bool {
        matches!(self, Self::Tcp(_))
    }
}

struct TalosSnapshotCreator {
    sink: TalosImageSink,
}

impl TalosSnapshotCreator {
    fn new(sink: TalosImageSink) -> Self {
        Self { sink }
    }
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
    gimbal: Query<(&GlobalTransform, &InfantryGimbal), With<Controlled>>,
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
    let Ok((gimbal_transform, gimbal_data)) = gimbal.single() else {
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
            _pad1: [0; 3],
            gimbal_yaw_rad: gimbal_data.local_yaw,
            gimbal_pitch_rad: gimbal_data.pitch,
            frame_seq: frame_stamp.frame_seq,
            last_applied_command_id: crate::network_bridge::last_applied_network_command_id(),
            gimbal_yaw_velocity_rad_s: 0.0,
            gimbal_pitch_velocity_rad_s: 0.0,
            status_flags: 0,
            _pad: [0; 16],
        });
    }
}

impl Plugin for TalosCapturePlugin {
    fn build(&self, app: &mut App) {
        // Dataset-compatible color+depth remains the default. Benchmarks can explicitly opt into
        // the lower-overhead RGB-only path with DAEDALUS_TALOS_RGB_ONLY=1. The explicit TCP
        // performance path is always native RGBA color-only and does not reserve dataset frames.
        let tcp = self.context.image_sink.is_tcp();
        let rgb_only = tcp || talos_rgb_only_capture_enabled();
        app.insert_resource(CaptureDepthPrepassEnabled(!rgb_only));
        let capture = if tcp {
            info!("Talos TCP native RGBA capture enabled; depth/dataset capture disabled");
            CaptureBundle::color(
                app,
                self.config.clone(),
                vec![cadence_gated_handler(TalosSnapshotCreator::new(
                    self.context.image_sink.clone(),
                ))],
            )
        } else if rgb_only {
            info!(
                "Talos RGB-only capture enabled via {TALOS_RGB_ONLY_ENV}; depth capture disabled"
            );
            CaptureBundle::color(
                app,
                self.config.clone(),
                vec![
                    cadence_gated_handler(TalosSnapshotCreator::new(
                        self.context.image_sink.clone(),
                    )),
                    cadence_gated_handler(DatasetSnapshotCreator::default()),
                ],
            )
        } else {
            CaptureBundle::color_and_depth(
                app,
                self.config.clone(),
                vec![
                    cadence_gated_handler(TalosSnapshotCreator::new(
                        self.context.image_sink.clone(),
                    )),
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

#[cfg(test)]
mod tests {
    use super::{
        CapturedFrame, CapturedFrameKind, ExtractedPoseData, SnapshotAsync, SnapshotSync,
        TalosCaptureCadenceState, TalosFrameStamp, TalosImageSink, TalosSnapshot,
        TalosSnapshotCreator, TalosSnapshotSink, TalosSnapshotSync, cadence_gated_handler,
        capture_max_hz_from_value, env_flag_enabled, prepare_extracted_pose,
        talos_frame_dimensions_valid,
    };
    use crate::capture::driver::GpuCaptureHandler;
    use crate::talos::tcp_image::{HEADER_BYTES, TcpImageSender, TcpImageSenderConfig};
    use bevy::prelude::World;
    use std::io::Read;
    use std::net::{SocketAddr, TcpStream};
    use std::time::{Duration, Instant};
    use talos_ipc::{IMAGE_HEIGHT, IMAGE_WIDTH};

    #[test]
    fn talos_rgb_only_env_accepts_explicit_truthy_values() {
        for value in ["1", "true", "TRUE", " yes ", "On"] {
            assert!(env_flag_enabled(value), "expected {value:?} to be truthy");
        }
    }

    #[test]
    fn talos_rgb_only_env_rejects_non_truthy_values() {
        for value in ["", "0", "false", "no", "off", "rgb"] {
            assert!(!env_flag_enabled(value), "expected {value:?} to be false");
        }
    }

    #[test]
    fn capture_cadence_unset_and_invalid_values_are_always_active() {
        for value in [
            None,
            Some(""),
            Some("0"),
            Some("-1"),
            Some("NaN"),
            Some("inf"),
            Some("bad"),
        ] {
            let mut state = TalosCaptureCadenceState::new(capture_max_hz_from_value(value));
            for now_s in [0.0, 0.001, 1.0, 10.0] {
                assert!(
                    state.should_capture(now_s),
                    "expected {value:?} to remain active"
                );
            }
        }
    }

    #[test]
    fn capture_cadence_admits_180_hz_for_ten_seconds_without_catch_up_bursts() {
        let mut state = TalosCaptureCadenceState::new(Some(180.0));
        let mut admissions = 0_u64;
        for tick in 0..=10_000_u64 {
            let now_s = tick as f64 / 1_000.0;
            if state.should_capture(now_s) {
                admissions += 1;
                assert!(
                    !state.should_capture(now_s),
                    "a main tick admitted more than one capture at {now_s}"
                );
            }
        }

        let interval_rate_hz = admissions.saturating_sub(1) as f64 / 10.0;
        assert!(
            (179.5..=180.5).contains(&interval_rate_hz),
            "unexpected admission rate {interval_rate_hz} Hz ({admissions} endpoints)"
        );
    }

    #[test]
    fn capture_cadence_preserves_phase_across_missed_periods() {
        let mut state = TalosCaptureCadenceState::new(Some(180.0));
        assert!(state.should_capture(0.0));
        assert!(state.should_capture(1.0));
        assert!(!state.should_capture(1.0));
        assert!(!state.should_capture(1.005));
        assert!(state.should_capture(1.006));
        assert!(!state.should_capture(1.006));
    }

    #[test]
    fn inactive_extraction_invalidates_snapshot_while_active_preserves_identity() {
        let stamp = TalosFrameStamp {
            frame_seq: 42,
            timestamp_ns: 123_456,
        };
        let mut extracted = ExtractedPoseData {
            frame_seq: 7,
            timestamp_ns: 8,
            valid: true,
        };

        assert!(!prepare_extracted_pose(&mut extracted, &stamp, false));
        assert_eq!(extracted.frame_seq, stamp.frame_seq);
        assert_eq!(extracted.timestamp_ns, stamp.timestamp_ns);
        assert!(!extracted.valid);

        let mut world = World::new();
        world.insert_resource(extracted.clone());
        let creator = cadence_gated_handler(TalosSnapshotCreator::new(TalosImageSink::File));
        assert!(creator.captured(&world).is_none());

        assert!(prepare_extracted_pose(&mut extracted, &stamp, true));
        extracted.valid = true;
        world.insert_resource(extracted);
        assert!(creator.captured(&world).is_some());
    }

    #[test]
    fn talos_sized_frame_dimensions_accept_smaller_frames_within_fixed_maximum() {
        assert!(talos_frame_dimensions_valid(640, 360));
        assert!(talos_frame_dimensions_valid(IMAGE_WIDTH, IMAGE_HEIGHT));
        assert!(!talos_frame_dimensions_valid(0, 360));
        assert!(!talos_frame_dimensions_valid(640, 0));
        assert!(!talos_frame_dimensions_valid(IMAGE_WIDTH + 1, 360));
        assert!(!talos_frame_dimensions_valid(640, IMAGE_HEIGHT + 1));
    }

    #[test]
    fn talos_frame_identity_is_nonzero_before_the_first_capture() {
        let stamp = TalosFrameStamp::default();
        assert_ne!(stamp.frame_seq, 0);
        assert_ne!(stamp.timestamp_ns, 0);
    }

    #[test]
    fn only_tcp_sync_snapshot_opts_into_the_premap_owned_rgba_path() {
        let sender = TcpImageSender::bind(TcpImageSenderConfig::new(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            0x0102_0304_0506_0708,
        ))
        .unwrap();
        let tcp = TalosSnapshotSync {
            frame_seq: 1,
            timestamp_ns: 2,
            sink: TalosImageSink::Tcp(sender.publisher()),
        };
        let file = TalosSnapshotSync {
            frame_seq: 1,
            timestamp_ns: 2,
            sink: TalosImageSink::File,
        };

        assert!(tcp.accepts_owned_rgba());
        assert!(!file.accepts_owned_rgba());
    }

    #[test]
    fn tcp_capture_preserves_epoch_sequence_timestamp_dimensions_and_rgba_bytes() {
        let epoch = 0x0102_0304_0506_0708;
        let sender = TcpImageSender::bind(TcpImageSenderConfig::new(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            epoch,
        ))
        .unwrap();
        let mut client = TcpStream::connect(sender.local_addr()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while sender.counters().connect_total == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(sender.counters().connect_total, 1);

        let mut snapshot = TalosSnapshot {
            sink: TalosSnapshotSink::Tcp(sender.publisher()),
            frame_seq: 42,
            timestamp_ns: 123_456_789,
        };
        snapshot.captured(CapturedFrame {
            kind: CapturedFrameKind::Rgba8,
            width: 2,
            height: 1,
            data: &[1, 2, 3, 4, 5, 6, 7, 8],
        });

        let mut wire = vec![0; HEADER_BYTES + 8];
        client.read_exact(&mut wire).unwrap();
        assert_eq!(u16::from_be_bytes(wire[8..10].try_into().unwrap()), 2);
        assert_eq!(u32::from_be_bytes(wire[12..16].try_into().unwrap()), 2);
        assert_eq!(u32::from_be_bytes(wire[16..20].try_into().unwrap()), 1);
        assert_eq!(u64::from_be_bytes(wire[24..32].try_into().unwrap()), epoch);
        assert_eq!(u64::from_be_bytes(wire[32..40].try_into().unwrap()), 42);
        assert_eq!(
            u64::from_be_bytes(wire[40..48].try_into().unwrap()),
            123_456_789
        );
        assert_eq!(&wire[HEADER_BYTES..], &[1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn tcp_snapshot_explicitly_accepts_owned_rgba_and_preserves_wire_identity() {
        let epoch = 0x1112_1314_1516_1718;
        let sender = TcpImageSender::bind(TcpImageSenderConfig::new(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            epoch,
        ))
        .unwrap();
        let mut client = TcpStream::connect(sender.local_addr()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while sender.counters().connect_total == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(sender.counters().connect_total, 1);

        let mut snapshot = TalosSnapshot {
            sink: TalosSnapshotSink::Tcp(sender.publisher()),
            frame_seq: 77,
            timestamp_ns: 987_654_321,
        };
        assert!(snapshot.accepts_owned_rgba());
        snapshot
            .captured_owned_rgba(2, 1, vec![8, 7, 6, 5, 4, 3, 2, 1])
            .unwrap();

        let mut wire = vec![0; HEADER_BYTES + 8];
        client.read_exact(&mut wire).unwrap();
        assert_eq!(u16::from_be_bytes(wire[8..10].try_into().unwrap()), 2);
        assert_eq!(u32::from_be_bytes(wire[12..16].try_into().unwrap()), 2);
        assert_eq!(u32::from_be_bytes(wire[16..20].try_into().unwrap()), 1);
        assert_eq!(u64::from_be_bytes(wire[24..32].try_into().unwrap()), epoch);
        assert_eq!(u64::from_be_bytes(wire[32..40].try_into().unwrap()), 77);
        assert_eq!(
            u64::from_be_bytes(wire[40..48].try_into().unwrap()),
            987_654_321
        );
        assert_eq!(&wire[HEADER_BYTES..], &[8, 7, 6, 5, 4, 3, 2, 1]);
        assert_eq!(sender.counters().owned_submit_total, 1);
        assert_eq!(sender.counters().borrowed_submit_total, 0);
    }

    #[test]
    fn invalid_owned_tcp_snapshot_returns_the_original_vec_for_driver_fallback() {
        let sender = TcpImageSender::bind(TcpImageSenderConfig::new(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            0x2122_2324_2526_2728,
        ))
        .unwrap();
        let mut snapshot = TalosSnapshot {
            sink: TalosSnapshotSink::Tcp(sender.publisher()),
            frame_seq: 88,
            timestamp_ns: 123,
        };
        let data = vec![1, 2, 3, 4];
        let pointer = data.as_ptr() as usize;

        let returned = snapshot.captured_owned_rgba(2, 1, data).unwrap_err();

        assert_eq!(returned.as_ptr() as usize, pointer);
        assert_eq!(returned, [1, 2, 3, 4]);
        assert_eq!(sender.counters().owned_submit_total, 0);
        assert_eq!(sender.counters().borrowed_submit_total, 0);
    }
}

/// Extract pose data from MainApp to RenderApp
fn extract_pose_data(
    mut pose_data: ResMut<ExtractedPoseData>,
    frame_stamp: Extract<Res<TalosFrameStamp>>,
    capture_active: Extract<Res<TalosCaptureActive>>,
    camera: Extract<Query<&GlobalTransform, With<CaptureSource>>>,
    gimbal: Extract<Query<&GlobalTransform, (With<Controlled>, With<InfantryGimbal>)>>,
    muzzle_offset: Extract<
        Query<(&GlobalTransform, &Transform), (With<InfantryLaunchOffset>, With<Controlled>)>,
    >,
    chassis_obs: Extract<Res<ChassisObservationFrame>>,
) {
    if !prepare_extracted_pose(&mut pose_data, &frame_stamp, capture_active.0) {
        return;
    }

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
    pose_data.valid = true;
}

fn prepare_extracted_pose(
    pose_data: &mut ExtractedPoseData,
    frame_stamp: &TalosFrameStamp,
    capture_active: bool,
) -> bool {
    pose_data.frame_seq = frame_stamp.frame_seq;
    pose_data.timestamp_ns = frame_stamp.timestamp_ns;
    pose_data.valid = false;
    capture_active
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
