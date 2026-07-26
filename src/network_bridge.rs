use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender, unbounded};
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer};
use std::net::UdpSocket;
use std::str;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use crate::components::{
    Controlled, InfantryChassis, InfantryGimbal, InfantryLaunchOffset, SubscribeAutoAim,
};
use crate::config::SimulationConfig;
use crate::systems::{FrequencyMetricKind, FrequencyMetrics, projectile_launch};
use crate::telemetry::PendingAutoAimShotContext;

#[derive(Resource)]
struct NetworkBridgeReceiver {
    receiver: Receiver<NetworkGimbalCommand>,
}

#[derive(Resource, Default)]
struct LatestNetworkGimbalCommand {
    command: Option<NetworkGimbalCommand>,
    received_at_s: f64,
}

const FIRE_ALIGNMENT_TOLERANCE_RAD: f32 = 0.035;
const COMMAND_STALE_TIMEOUT_S: f64 = 0.25;
static NETWORK_COMMAND_RECEIVED_COUNT: AtomicU64 = AtomicU64::new(0);
static LAST_APPLIED_NETWORK_COMMAND_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Deserialize)]
struct NetworkGimbalCommand {
    #[serde(default)]
    command_id: u64,
    yaw: Option<f32>,
    yaw_deg: Option<f32>,
    pitch: Option<f32>,
    pitch_deg: Option<f32>,
    distance: Option<f32>,
    distance_m: Option<f32>,
    #[serde(default, alias = "fire", deserialize_with = "deserialize_fire_advice")]
    fire_advice: bool,
}

impl NetworkGimbalCommand {
    fn yaw_deg(self) -> Option<f32> {
        self.yaw_deg.or(self.yaw)
    }

    fn pitch_deg(self) -> Option<f32> {
        self.pitch_deg.or(self.pitch)
    }

    fn distance_m(self) -> Option<f32> {
        self.distance_m.or(self.distance)
    }

    fn should_ignore(self) -> bool {
        self.distance_m().is_some_and(|distance| distance == -1.0)
    }
}

fn deserialize_fire_advice<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Bool(value) => Ok(value),
        serde_json::Value::Number(value) => Ok(value.as_i64().unwrap_or_default() != 0),
        serde_json::Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "fire" => Ok(true),
            "0" | "false" | "no" | "" => Ok(false),
            other => Err(D::Error::custom(format!(
                "invalid fire_advice string {other:?}"
            ))),
        },
        serde_json::Value::Null => Ok(false),
        other => Err(D::Error::custom(format!(
            "invalid fire_advice value {other:?}"
        ))),
    }
}

pub struct NetworkBridgePlugin;

impl Plugin for NetworkBridgePlugin {
    fn build(&self, app: &mut App) {
        // Command draining stays installed even when the optional UDP receiver
        // is disabled. Initialize the latest-command cache unconditionally so
        // a headless/performance configuration cannot panic before telemetry
        // starts.
        app.init_resource::<LatestNetworkGimbalCommand>()
            .add_systems(Startup, setup_network_bridge)
            .add_systems(
                Update,
                drain_network_gimbal_commands
                    .run_if(|enabled: Res<SubscribeAutoAim>| enabled.load(Ordering::Acquire)),
            )
            .add_systems(
                FixedUpdate,
                apply_latest_network_gimbal_command
                    .run_if(|enabled: Res<SubscribeAutoAim>| enabled.load(Ordering::Acquire)),
            );
    }
}

pub fn network_command_received_count() -> u64 {
    NETWORK_COMMAND_RECEIVED_COUNT.load(Ordering::Relaxed)
}

pub fn last_applied_network_command_id() -> u64 {
    LAST_APPLIED_NETWORK_COMMAND_ID.load(Ordering::Acquire)
}

fn setup_network_bridge(mut commands: Commands, config: Res<SimulationConfig>) {
    if !config.network_bridge.enabled {
        info!("Network bridge disabled by config.");
        return;
    }

    let (sender, receiver) = unbounded();
    let bind = config.network_bridge.bind.clone();

    let socket = match UdpSocket::bind(&bind) {
        Ok(socket) => socket,
        Err(error) => {
            warn!("Network bridge failed to bind udp://{}: {}", bind, error);
            return;
        }
    };

    info!("Network bridge listening on udp://{}", bind);
    thread::Builder::new()
        .name("daedalus-network-bridge".to_string())
        .spawn(move || receive_network_commands(socket, sender))
        .expect("failed to spawn network bridge thread");

    commands.insert_resource(NetworkBridgeReceiver { receiver });
}

fn receive_network_commands(socket: UdpSocket, sender: Sender<NetworkGimbalCommand>) {
    let mut buffer = [0_u8; 2048];

    loop {
        let (length, peer) = match socket.recv_from(&mut buffer) {
            Ok(result) => result,
            Err(error) => {
                warn!("Network bridge receive error: {}", error);
                continue;
            }
        };

        let payload = match str::from_utf8(&buffer[..length]) {
            Ok(payload) => payload,
            Err(error) => {
                warn!(
                    "Network bridge ignored non-UTF8 packet from {}: {}",
                    peer, error
                );
                continue;
            }
        };

        let command = match serde_json::from_str::<NetworkGimbalCommand>(payload) {
            Ok(command) => command,
            Err(error) => {
                warn!(
                    "Network bridge ignored malformed JSON packet from {}: {}",
                    peer, error
                );
                continue;
            }
        };

        if sender.send(command).is_err() {
            break;
        }
        NETWORK_COMMAND_RECEIVED_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

fn drain_network_gimbal_commands(
    receiver: Option<Res<NetworkBridgeReceiver>>,
    time: Res<Time>,
    mut latest: ResMut<LatestNetworkGimbalCommand>,
) {
    let Some(receiver) = receiver else {
        return;
    };

    while let Ok(command) = receiver.receiver.try_recv() {
        latest.command = Some(command);
        latest.received_at_s = time.elapsed_secs_f64();
    }
}

fn apply_latest_network_gimbal_command(
    latest: Option<Res<LatestNetworkGimbalCommand>>,
    time: Res<Time>,
    config: Res<SimulationConfig>,
    mut metrics: ResMut<FrequencyMetrics>,
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
    let now_s = time.elapsed_secs_f64();
    metrics.mark(FrequencyMetricKind::SimCommandApply, now_s);

    let Some(latest) = latest else {
        return;
    };

    let Some(command) = latest.command else {
        return;
    };

    if command.should_ignore() {
        return;
    }
    if now_s - latest.received_at_s > COMMAND_STALE_TIMEOUT_S {
        return;
    }

    let (mut gimbal_transform, mut gimbal_data) = gimbal.into_inner();
    let (current_yaw, current_pitch, _) = gimbal_transform.rotation.to_euler(EulerRot::YXZ);
    let (yaw, pitch) = if should_apply_aim_command(command) {
        let max_step = config.vehicle.gimbal_rotation_speed * time.delta_secs();
        let yaw = command.yaw_deg().map_or(current_yaw, |yaw| {
            step_towards_angle(current_yaw, yaw.to_radians(), max_step)
        });
        let pitch = command.pitch_deg().map_or(current_pitch, |pitch| {
            let target = command_pitch_target_rad(pitch, config.vehicle.gimbal_pitch_limit);
            step_towards_scalar(current_pitch, target, max_step)
        });

        gimbal_data.local_yaw = yaw;
        gimbal_data.pitch = pitch;
        gimbal_transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
        metrics.mark(FrequencyMetricKind::GimbalUpdate, now_s);
        LAST_APPLIED_NETWORK_COMMAND_ID.store(command.command_id, Ordering::Release);

        (yaw, pitch)
    } else {
        (current_yaw, current_pitch)
    };

    let aim_aligned = is_command_aligned_for_launch(
        command,
        yaw,
        pitch,
        config.vehicle.gimbal_pitch_limit,
        FIRE_ALIGNMENT_TOLERANCE_RAD,
    );
    if should_launch_from_command(command, aim_aligned) {
        commands.insert_resource(PendingAutoAimShotContext {
            yaw_deg: command.yaw_deg(),
            pitch_deg: command.pitch_deg(),
            distance_m: command.distance_m(),
            fire_advice: command.fire_advice,
            aim_aligned,
            actual_yaw_deg: yaw.to_degrees(),
            actual_pitch_deg: pitch.to_degrees(),
        });
        commands.queue(|world: &mut World| {
            world.run_system_once(projectile_launch).unwrap();
        });
    }
}

fn should_launch_from_command(command: NetworkGimbalCommand, aim_aligned: bool) -> bool {
    command.fire_advice && aim_aligned && !command.should_ignore()
}

fn should_apply_aim_command(command: NetworkGimbalCommand) -> bool {
    !command.should_ignore() && (command.yaw_deg().is_some() || command.pitch_deg().is_some())
}

fn command_pitch_target_rad(pitch_deg: f32, pitch_limit: f32) -> f32 {
    (pitch_deg - 90.0)
        .to_radians()
        .clamp(-pitch_limit, pitch_limit)
}

fn is_command_aligned_for_launch(
    command: NetworkGimbalCommand,
    yaw: f32,
    pitch: f32,
    pitch_limit: f32,
    tolerance: f32,
) -> bool {
    let Some(yaw_target) = command.yaw_deg().map(f32::to_radians) else {
        return false;
    };
    let Some(pitch_target) = command
        .pitch_deg()
        .map(|pitch| command_pitch_target_rad(pitch, pitch_limit))
    else {
        return false;
    };

    normalize_angle_rad(yaw_target - yaw).abs() <= tolerance
        && (pitch_target - pitch).abs() <= tolerance
}

fn normalize_angle_rad(angle: f32) -> f32 {
    let two_pi = std::f32::consts::TAU;
    let mut result = (angle + std::f32::consts::PI) % two_pi;
    if result < 0.0 {
        result += two_pi;
    }
    result - std::f32::consts::PI
}

fn step_towards_scalar(current: f32, target: f32, max_step: f32) -> f32 {
    if max_step <= 0.0 {
        return current;
    }

    let delta = target - current;
    if delta.abs() <= max_step {
        target
    } else {
        current + delta.signum() * max_step
    }
}

fn step_towards_angle(current: f32, target: f32, max_step: f32) -> f32 {
    if max_step <= 0.0 {
        return normalize_angle_rad(current);
    }

    let delta = normalize_angle_rad(target - current);
    if delta.abs() <= max_step {
        normalize_angle_rad(target)
    } else {
        normalize_angle_rad(current + delta.signum() * max_step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_initializes_command_cache_without_udp_receiver() {
        let mut app = App::new();
        app.add_plugins(NetworkBridgePlugin);

        assert!(
            app.world()
                .contains_resource::<LatestNetworkGimbalCommand>()
        );
        assert!(!app.world().contains_resource::<NetworkBridgeReceiver>());
    }

    #[test]
    fn parses_ros_style_command() {
        let command: NetworkGimbalCommand =
            serde_json::from_str(r#"{"yaw":12.5,"pitch":91.5,"distance":3.2,"fire_advice":true}"#)
                .unwrap();

        assert_eq!(command.yaw_deg(), Some(12.5));
        assert_eq!(command.pitch_deg(), Some(91.5));
        assert_eq!(command.distance_m(), Some(3.2));
        assert!(command.fire_advice);
    }

    #[test]
    fn parses_talos_style_command() {
        let command: NetworkGimbalCommand =
            serde_json::from_str(r#"{"yaw_deg":-4.0,"pitch_deg":90.0,"distance_m":2.0,"fire":1}"#)
                .unwrap();

        assert_eq!(command.yaw_deg(), Some(-4.0));
        assert_eq!(command.pitch_deg(), Some(90.0));
        assert_eq!(command.distance_m(), Some(2.0));
        assert!(command.fire_advice);
    }

    #[test]
    fn fire_advice_requires_alignment() {
        let command: NetworkGimbalCommand =
            serde_json::from_str(r#"{"yaw_deg":0.0,"pitch_deg":90.0,"distance_m":2.0,"fire":1}"#)
                .unwrap();

        assert!(!should_launch_from_command(command, false));
        assert!(should_launch_from_command(command, true));
    }

    #[test]
    fn no_target_command_never_launches() {
        let command: NetworkGimbalCommand =
            serde_json::from_str(r#"{"distance_m":-1.0,"fire":1}"#).unwrap();

        assert!(!should_launch_from_command(command, true));
    }

    #[test]
    fn aim_command_applies_without_keyboard_authorization() {
        let command: NetworkGimbalCommand =
            serde_json::from_str(r#"{"yaw_deg":12.0,"pitch_deg":91.0,"distance_m":2.0}"#).unwrap();

        assert!(should_apply_aim_command(command));
    }

    #[test]
    fn no_target_aim_command_is_not_applied() {
        let command: NetworkGimbalCommand =
            serde_json::from_str(r#"{"yaw_deg":12.0,"pitch_deg":91.0,"distance_m":-1.0}"#).unwrap();

        assert!(!should_apply_aim_command(command));
    }

    #[test]
    fn scalar_step_limits_external_gimbal_motion() {
        assert_eq!(step_towards_scalar(0.0, 10.0, 3.0), 3.0);
        assert_eq!(step_towards_scalar(0.0, -10.0, 3.0), -3.0);
        assert_eq!(step_towards_scalar(0.0, 2.0, 3.0), 2.0);
    }

    #[test]
    fn launch_alignment_requires_gimbal_near_command() {
        let command: NetworkGimbalCommand =
            serde_json::from_str(r#"{"yaw_deg":10.0,"pitch_deg":80.0,"distance_m":2.0}"#).unwrap();
        let pitch_limit = 45.0_f32.to_radians();

        assert!(is_command_aligned_for_launch(
            command,
            10.5_f32.to_radians(),
            -10.5_f32.to_radians(),
            pitch_limit,
            1.0_f32.to_radians(),
        ));
        assert!(!is_command_aligned_for_launch(
            command,
            14.0_f32.to_radians(),
            -10.5_f32.to_radians(),
            pitch_limit,
            1.0_f32.to_radians(),
        ));
        assert!(!is_command_aligned_for_launch(
            command,
            10.5_f32.to_radians(),
            -14.0_f32.to_radians(),
            pitch_limit,
            1.0_f32.to_radians(),
        ));
    }

    #[test]
    fn angle_step_uses_shortest_wrapped_path() {
        let current = 179.0_f32.to_radians();
        let target = -179.0_f32.to_radians();
        let next = step_towards_angle(current, target, 5.0_f32.to_radians());

        assert!((next.to_degrees() + 179.0).abs() < 0.01);
    }
}
