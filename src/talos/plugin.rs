use crate::capture::advance_exposure_wall_timestamp;
use crate::capture::driver::{CaptureConfig, CapturedFrameKind};
use crate::components::{
    Controlled, InfantryChassis, InfantryGimbal, InfantryLaunchOffset, SubscribeAutoAim,
};
use crate::config::SimulationConfig;
use crate::systems::projectile_launch;
use crate::talos::capture::{
    TalosCaptureContext, TalosCapturePlugin, TalosFrameStamp, TalosImageSink,
    advance_talos_frame_stamp, publish_talos_pose_system,
};
use crate::talos::tcp_image::{TcpImageSender, TcpImageSenderConfig, mark_file_image_transport};
use crate::telemetry::PendingAutoAimShotContext;
use bevy::ecs::system::RunSystemOnce;
use bevy::image::BevyDefault;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use talos_ipc::*;

const TALOS_WIDTH_ENV: &str = "DAEDALUS_TALOS_WIDTH";
const TALOS_HEIGHT_ENV: &str = "DAEDALUS_TALOS_HEIGHT";
const TALOS_IMAGE_TRANSPORT_ENV: &str = "DAEDALUS_TALOS_IMAGE_TRANSPORT";
const TALOS_TCP_BIND_ENV: &str = "DAEDALUS_TALOS_TCP_BIND";
const TALOS_TCP_WRITE_TIMEOUT_ENV: &str = "DAEDALUS_TALOS_TCP_WRITE_TIMEOUT_MS";
const DEFAULT_TALOS_TCP_BIND: &str = "0.0.0.0:5602";
const MIN_TALOS_TCP_WRITE_TIMEOUT_MS: u64 = 100;
const MAX_TALOS_TCP_WRITE_TIMEOUT_MS: u64 = 2000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TalosImageTransport {
    #[default]
    File,
    Tcp,
}

fn parse_talos_image_transport(value: &str) -> Option<TalosImageTransport> {
    match value.trim().to_ascii_lowercase().as_str() {
        "file" => Some(TalosImageTransport::File),
        "tcp" => Some(TalosImageTransport::Tcp),
        _ => None,
    }
}

fn talos_image_transport_from_env() -> TalosImageTransport {
    match std::env::var(TALOS_IMAGE_TRANSPORT_ENV) {
        Ok(value) => match parse_talos_image_transport(&value) {
            Some(transport) => transport,
            None => {
                warn!(
                    "Ignoring invalid {TALOS_IMAGE_TRANSPORT_ENV}={value:?}; expected file or tcp, using file"
                );
                TalosImageTransport::File
            }
        },
        Err(std::env::VarError::NotPresent) => TalosImageTransport::File,
        Err(std::env::VarError::NotUnicode(_)) => {
            warn!("Ignoring non-Unicode {TALOS_IMAGE_TRANSPORT_ENV}; using file");
            TalosImageTransport::File
        }
    }
}

fn parse_talos_tcp_bind(value: &str) -> Option<SocketAddr> {
    value.trim().parse().ok()
}

fn talos_tcp_bind_from_env() -> SocketAddr {
    let fallback = DEFAULT_TALOS_TCP_BIND
        .parse()
        .expect("default Talos TCP bind address must be valid");
    match std::env::var(TALOS_TCP_BIND_ENV) {
        Ok(value) => match parse_talos_tcp_bind(&value) {
            Some(address) => address,
            None => {
                warn!(
                    "Ignoring invalid {TALOS_TCP_BIND_ENV}={value:?}; expected an IP:port socket address, using {fallback}"
                );
                fallback
            }
        },
        Err(std::env::VarError::NotPresent) => fallback,
        Err(std::env::VarError::NotUnicode(_)) => {
            warn!("Ignoring non-Unicode {TALOS_TCP_BIND_ENV}; using {fallback}");
            fallback
        }
    }
}

fn parse_talos_tcp_write_timeout_override(value: &str) -> Option<Duration> {
    value
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|milliseconds| {
            (MIN_TALOS_TCP_WRITE_TIMEOUT_MS..=MAX_TALOS_TCP_WRITE_TIMEOUT_MS).contains(milliseconds)
        })
        .map(Duration::from_millis)
}

/// Returns `None` when the startup-only diagnostic override is absent or
/// invalid. `TcpImageSenderConfig::new` then retains its historical 250 ms
/// default exactly.
fn talos_tcp_write_timeout_override_from_env() -> Option<Duration> {
    match std::env::var(TALOS_TCP_WRITE_TIMEOUT_ENV) {
        Ok(value) => match parse_talos_tcp_write_timeout_override(&value) {
            Some(timeout) => Some(timeout),
            None => {
                warn!(
                    "Ignoring invalid {TALOS_TCP_WRITE_TIMEOUT_ENV}={value:?}; expected integer {MIN_TALOS_TCP_WRITE_TIMEOUT_MS}..={MAX_TALOS_TCP_WRITE_TIMEOUT_MS}ms, retaining sender default"
                );
                None
            }
        },
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            warn!("Ignoring non-Unicode {TALOS_TCP_WRITE_TIMEOUT_ENV}; retaining sender default");
            None
        }
    }
}

fn parse_talos_dimension_override(value: &str, maximum: u32) -> Option<u32> {
    value
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|dimension| (1..=maximum).contains(dimension))
}

fn talos_dimension_from_env(key: &str, maximum: u32) -> u32 {
    match std::env::var(key) {
        Ok(value) => match parse_talos_dimension_override(&value, maximum) {
            Some(dimension) => dimension,
            None => {
                warn!(
                    "Ignoring invalid {key}={value:?}; expected an integer in 1..={maximum}, using {maximum}"
                );
                maximum
            }
        },
        Err(std::env::VarError::NotPresent) => maximum,
        Err(std::env::VarError::NotUnicode(_)) => {
            warn!("Ignoring non-Unicode {key}; using {maximum}");
            maximum
        }
    }
}

#[derive(Resource)]
pub struct ShmSubscriberRes(pub Arc<Mutex<ShmSubscriber>>);

#[derive(Resource, Deref, DerefMut)]
pub struct TalosEnabled(pub AtomicBool);

pub struct TalosPluginConfig {
    pub width: u32,
    pub height: u32,
    pub fov_y: f32,
    pub texture_format: TextureFormat,
    pub image_transport: TalosImageTransport,
    pub tcp_bind_addr: SocketAddr,
    pub tcp_write_timeout_override: Option<Duration>,
}

impl Default for TalosPluginConfig {
    fn default() -> Self {
        let config = SimulationConfig::default();
        Self {
            width: talos_dimension_from_env(TALOS_WIDTH_ENV, talos_ipc::IMAGE_WIDTH),
            height: talos_dimension_from_env(TALOS_HEIGHT_ENV, talos_ipc::IMAGE_HEIGHT),
            fov_y: config.camera.fov.to_radians(),
            texture_format: TextureFormat::Rgba8UnormSrgb,
            image_transport: talos_image_transport_from_env(),
            tcp_bind_addr: talos_tcp_bind_from_env(),
            tcp_write_timeout_override: talos_tcp_write_timeout_override_from_env(),
        }
    }
}

#[derive(Resource)]
struct TcpImageSenderResource(TcpImageSender);

#[derive(Default)]
pub struct TalosPlugin {
    pub config: TalosPluginConfig,
}

impl Plugin for TalosPlugin {
    fn build(&self, app: &mut App) {
        info!(
            "Talos effective capture resolution: {}x{} (fixed v3 slot maximum {}x{})",
            self.config.width,
            self.config.height,
            talos_ipc::IMAGE_WIDTH,
            talos_ipc::IMAGE_HEIGHT
        );

        let publisher = match ShmPublisher::create() {
            Ok(p) => {
                info!("talos shm created");
                p
            }
            Err(e) => {
                error!("cannot create talos shm: {}", e);
                return;
            }
        };

        let producer_epoch = publisher.producer_epoch();
        let (image_sink, frame_kind, texture_format, tcp_sender) = match self.config.image_transport
        {
            TalosImageTransport::File => {
                mark_file_image_transport();
                info!("Talos image transport: file (compatibility/default)");
                (
                    TalosImageSink::File,
                    CapturedFrameKind::Rgb8,
                    self.config.texture_format,
                    None,
                )
            }
            TalosImageTransport::Tcp => {
                let mut sender_config =
                    TcpImageSenderConfig::new(self.config.tcp_bind_addr, producer_epoch);
                if let Some(timeout) = self.config.tcp_write_timeout_override {
                    sender_config.write_timeout = timeout;
                    info!(
                        "Talos TCP image sender diagnostic write timeout override={}ms",
                        timeout.as_millis()
                    );
                }
                match TcpImageSender::bind(sender_config) {
                    Ok(sender) => {
                        info!(
                            "Talos image transport: TCP native RGBA32 listener={} producer_epoch={producer_epoch}",
                            sender.local_addr()
                        );
                        let sink = TalosImageSink::Tcp(sender.publisher());
                        (
                            sink,
                            CapturedFrameKind::Rgba8,
                            TextureFormat::Rgba8UnormSrgb,
                            Some(sender),
                        )
                    }
                    Err(error) => {
                        error!(
                            "Cannot bind explicit Talos TCP image transport at {}: {error}; Talos capture is disabled (no file fallback)",
                            self.config.tcp_bind_addr
                        );
                        return;
                    }
                }
            }
        };

        let publisher = Arc::new(Mutex::new(publisher));

        let capture_config = CaptureConfig {
            width: self.config.width,
            height: self.config.height,
            texture_format,
            frame_kind,
        };

        let capture_context = TalosCaptureContext {
            publisher: publisher.clone(),
            fov_y: self.config.fov_y,
            image_sink,
        };

        app.init_resource::<TalosFrameStamp>();

        app.add_plugins(TalosCapturePlugin {
            config: capture_config,
            context: capture_context,
        });
        if let Some(sender) = tcp_sender {
            app.insert_resource(TcpImageSenderResource(sender));
        }

        match ShmSubscriber::connect() {
            Ok(subscriber) => {
                info!("connected to talos-cpp");
                app.insert_resource(ShmSubscriberRes(Arc::new(Mutex::new(subscriber))));
            }
            Err(_) => {
                info!("could not connect to talos-cpp");
            }
        }

        app.insert_resource(TalosEnabled(AtomicBool::new(true)));
        app.add_systems(
            PostUpdate,
            advance_talos_frame_stamp
                .after(advance_exposure_wall_timestamp)
                .before(TransformSystems::Propagate),
        );
        app.add_systems(Last, heartbeat_system);
        app.add_systems(Last, publish_talos_pose_system);
        app.add_systems(
            Last,
            crate::talos::ground_truth::publish_ground_truth_system
                .after(publish_talos_pose_system),
        );
        app.add_systems(
            Last,
            process_subscription
                .run_if(|enabled: Res<SubscribeAutoAim>| enabled.load(Ordering::Acquire)),
        );
    }
}

fn process_subscription(
    context: Option<Res<ShmSubscriberRes>>,
    config: Res<SimulationConfig>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    gimbal: Single<
        (&mut Transform, &mut InfantryGimbal),
        (
            With<Controlled>,
            Without<InfantryChassis>,
            Without<InfantryLaunchOffset>,
        ),
    >,
    _muzzle_offset: Single<
        (&GlobalTransform, &Transform),
        (With<InfantryLaunchOffset>, With<Controlled>),
    >,
) {
    if !config
        .auto_aim
        .command_transport
        .eq_ignore_ascii_case("talos")
    {
        return;
    }

    let Some(ctx) = context else {
        return;
    };
    let Some(cmd) = recv_gimbal_cmd(&ctx) else {
        return;
    };
    if cmd.distance_m == -1.0 {
        return;
    }
    let yaw_f32 = (cmd.yaw_deg).to_radians();
    let pitch_f32 = (-cmd.pitch_deg - 90.0).to_radians();
    let space_pressed = keyboard.pressed(KeyCode::Space);
    if should_launch_from_talos_cmd(&cmd, space_pressed) {
        commands.insert_resource(PendingAutoAimShotContext {
            yaw_deg: Some(cmd.yaw_deg),
            pitch_deg: Some(cmd.pitch_deg),
            distance_m: Some(cmd.distance_m),
            fire_advice: cmd.fire_advice == 1,
            aim_aligned: true,
            actual_yaw_deg: yaw_f32.to_degrees(),
            actual_pitch_deg: pitch_f32.to_degrees(),
        });
        commands.queue(|w: &mut World| {
            w.run_system_once(projectile_launch).unwrap();
        });
    }
    if !should_apply_talos_aim_cmd(&cmd, space_pressed) {
        return;
    }
    let (mut gimbal_transform, mut gimbal_data) = gimbal.into_inner();
    gimbal_data.local_yaw = yaw_f32;
    gimbal_data.pitch = pitch_f32;
    gimbal_transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw_f32, pitch_f32, 0.0);
    //info!("yaw={} pitch={}", cmd.yaw_deg, cmd.pitch_deg);
}

fn heartbeat_system(context: Option<Res<TalosCaptureContext>>) {
    if let Some(ctx) = context {
        if let Ok(mut publisher) = ctx.publisher.try_lock() {
            publisher.update_heartbeat();
        }
    }
}

pub fn publish_pose(
    context: &TalosCaptureContext,
    index: PoseIndex,
    position: [f32; 3],
    quaternion: [f32; 4],
    frame_seq: u64,
    timestamp_ns: u64,
) {
    if let Ok(mut publisher) = context.publisher.lock() {
        publisher.publish_pose(index, position, quaternion, frame_seq, timestamp_ns);
    }
}

pub fn recv_gimbal_cmd(subscriber: &ShmSubscriberRes) -> Option<GimbalCmd> {
    subscriber.0.lock().ok()?.recv_gimbal_cmd()
}

fn should_launch_from_talos_cmd(cmd: &GimbalCmd, fire_pressed: bool) -> bool {
    fire_pressed && cmd.distance_m != -1.0 && cmd.fire_advice == 1
}

fn should_apply_talos_aim_cmd(cmd: &GimbalCmd, lock_pressed: bool) -> bool {
    lock_pressed && cmd.distance_m != -1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn talos_dimension_override_accepts_only_bounded_positive_integers() {
        assert_eq!(parse_talos_dimension_override("1", 1440), Some(1));
        assert_eq!(parse_talos_dimension_override(" 640 ", 1440), Some(640));
        assert_eq!(parse_talos_dimension_override("1440", 1440), Some(1440));
        assert_eq!(parse_talos_dimension_override("0", 1440), None);
        assert_eq!(parse_talos_dimension_override("1441", 1440), None);
        assert_eq!(parse_talos_dimension_override("invalid", 1440), None);
    }

    #[test]
    fn image_transport_is_explicit_and_file_is_the_only_compatibility_default() {
        assert_eq!(
            parse_talos_image_transport("file"),
            Some(TalosImageTransport::File)
        );
        assert_eq!(
            parse_talos_image_transport(" TCP "),
            Some(TalosImageTransport::Tcp)
        );
        assert_eq!(parse_talos_image_transport("shm"), None);
        assert_eq!(TalosImageTransport::default(), TalosImageTransport::File);
    }

    #[test]
    fn tcp_bind_parser_accepts_numeric_listener_addresses() {
        assert_eq!(
            parse_talos_tcp_bind(" 0.0.0.0:5602 "),
            Some(SocketAddr::from(([0, 0, 0, 0], 5602)))
        );
        assert_eq!(
            parse_talos_tcp_bind("127.0.0.1:0"),
            Some(SocketAddr::from(([127, 0, 0, 1], 0)))
        );
        assert_eq!(parse_talos_tcp_bind("localhost:5602"), None);
        assert_eq!(parse_talos_tcp_bind("invalid"), None);
    }

    #[test]
    fn tcp_write_timeout_override_is_strictly_bounded_and_default_is_not_encoded_here() {
        assert_eq!(
            parse_talos_tcp_write_timeout_override("100"),
            Some(Duration::from_millis(100))
        );
        assert_eq!(
            parse_talos_tcp_write_timeout_override(" 1000 "),
            Some(Duration::from_millis(1000))
        );
        assert_eq!(
            parse_talos_tcp_write_timeout_override("2000"),
            Some(Duration::from_millis(2000))
        );
        assert_eq!(parse_talos_tcp_write_timeout_override("99"), None);
        assert_eq!(parse_talos_tcp_write_timeout_override("2001"), None);
        assert_eq!(parse_talos_tcp_write_timeout_override("invalid"), None);
    }

    #[test]
    fn talos_fire_advice_requires_space_authorization() {
        let cmd = GimbalCmd {
            distance_m: 2.0,
            fire_advice: 1,
            ..Default::default()
        };

        assert!(!should_launch_from_talos_cmd(&cmd, false));
        assert!(should_launch_from_talos_cmd(&cmd, true));
    }

    #[test]
    fn talos_no_target_command_never_launches() {
        let cmd = GimbalCmd {
            distance_m: -1.0,
            fire_advice: 1,
            ..Default::default()
        };

        assert!(!should_launch_from_talos_cmd(&cmd, true));
    }

    #[test]
    fn talos_aim_requires_space_authorization() {
        let cmd = GimbalCmd {
            distance_m: 2.0,
            ..Default::default()
        };

        assert!(!should_apply_talos_aim_cmd(&cmd, false));
        assert!(should_apply_talos_aim_cmd(&cmd, true));
    }

    #[test]
    fn talos_no_target_aim_is_not_applied() {
        let cmd = GimbalCmd {
            distance_m: -1.0,
            ..Default::default()
        };

        assert!(!should_apply_talos_aim_cmd(&cmd, true));
    }
}

pub const M_ALIGN_MAT3: Mat3 = Mat3::from_cols(
    Vec3::new(0.0, -1.0, 0.0), // M[0,0], M[1,0], M[2,0]
    Vec3::new(0.0, 0.0, 1.0),  // M[0,1], M[1,1], M[2,1]
    Vec3::new(-1.0, 0.0, 0.0), // M[0,2], M[1,2], M[2,2]
);

#[inline]
pub fn to_ros(bevy_transform: Transform) -> Transform {
    let new_rotation = to_ros_quat(bevy_transform.rotation);
    let new_translation = to_ros_translation(bevy_transform.translation);
    Transform::from_translation(new_translation).with_rotation(new_rotation)
}

pub fn to_ros_translation(vec3: Vec3) -> Vec3 {
    let align_rot_mat = M_ALIGN_MAT3;
    let new_translation = align_rot_mat * vec3;
    new_translation
}

pub fn to_ros_quat(quat: Quat) -> Quat {
    let align_rot_mat = M_ALIGN_MAT3;
    let align_quat = Quat::from_mat3(&align_rot_mat);
    let new_rotation = align_quat * quat * align_quat.inverse();
    new_rotation
}
