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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TalosImageTransport {
    #[default]
    File,
    Tcp,
}

fn talos_image_transport_from_env() -> TalosImageTransport {
    match std::env::var(TALOS_IMAGE_TRANSPORT_ENV) {
        Ok(value) if value.trim().eq_ignore_ascii_case("tcp") => TalosImageTransport::Tcp,
        Ok(value) if value.trim().eq_ignore_ascii_case("file") => TalosImageTransport::File,
        Ok(value) => {
            warn!("Ignoring invalid {TALOS_IMAGE_TRANSPORT_ENV}={value:?}; using file");
            TalosImageTransport::File
        }
        Err(_) => TalosImageTransport::File,
    }
}

fn talos_tcp_bind_from_env() -> SocketAddr {
    let fallback = DEFAULT_TALOS_TCP_BIND.parse().unwrap();
    std::env::var(TALOS_TCP_BIND_ENV)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(fallback)
}

fn talos_tcp_write_timeout_from_env() -> Option<Duration> {
    std::env::var(TALOS_TCP_WRITE_TIMEOUT_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|milliseconds| (100..=2000).contains(milliseconds))
        .map(Duration::from_millis)
}

fn talos_dimension_from_env(key: &str, maximum: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|dimension| (1..=maximum).contains(dimension))
        .unwrap_or(maximum)
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
            tcp_write_timeout_override: talos_tcp_write_timeout_from_env(),
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
                (
                    TalosImageSink::File,
                    CapturedFrameKind::Rgb8,
                    self.config.texture_format,
                    None,
                )
            }
            TalosImageTransport::Tcp => {
                let mut config =
                    TcpImageSenderConfig::new(self.config.tcp_bind_addr, producer_epoch);
                if let Some(timeout) = self.config.tcp_write_timeout_override {
                    config.write_timeout = timeout;
                }
                let sender = match TcpImageSender::bind(config) {
                    Ok(sender) => sender,
                    Err(error) => {
                        error!("Cannot bind Talos TCP image transport: {error}; capture disabled");
                        return;
                    }
                };
                let sink = TalosImageSink::Tcp(sender.publisher());
                (
                    sink,
                    CapturedFrameKind::Rgba8,
                    TextureFormat::Rgba8UnormSrgb,
                    Some(sender),
                )
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
        app.add_systems(Last, (advance_talos_frame_stamp, heartbeat_system));
        app.add_systems(
            Last,
            publish_talos_pose_system.after(advance_talos_frame_stamp),
        );
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
    mut commands: Commands,
    gimbal: Single<
        (&mut Transform, &mut InfantryGimbal),
        (
            With<Controlled>,
            Without<InfantryChassis>,
            Without<InfantryLaunchOffset>,
        ),
    >,
    muzzle_offset: Single<
        (&GlobalTransform, &Transform),
        (With<InfantryLaunchOffset>, With<Controlled>),
    >,
) {
    let Some(ctx) = context else {
        return;
    };
    let (mut gimbal_transform, mut gimbal_data) = gimbal.into_inner();

    let Some(cmd) = recv_gimbal_cmd(&ctx) else {
        return;
    };
    if cmd.distance_m == -1.0 {
        return;
    }
    if cmd.fire_advice == 1 {
        commands.queue(|w: &mut World| {
            w.run_system_once(projectile_launch).unwrap();
        });
    }
    let yaw_f32 = (cmd.yaw_deg).to_radians();
    let pitch_f32 = (-cmd.pitch_deg - 90.0).to_radians();
    gimbal_data.local_yaw = yaw_f32;
    gimbal_data.pitch = pitch_f32;
    let expected_rotation = Quat::from_euler(EulerRot::YXZ, yaw_f32, pitch_f32, 0.0);
    let current_rotation = muzzle_offset.0.rotation();
    let delta = expected_rotation * current_rotation.inverse();
    gimbal_transform.rotation = delta * gimbal_transform.rotation;
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
