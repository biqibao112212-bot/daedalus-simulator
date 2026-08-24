use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender, bounded};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::armor_hit_api::ArmorHitLedger;
use crate::robomaster::prelude::{
    Activation, BigRuneScores, ManualPowerRuneControlState, PowerRune, PowerRuneMechanism,
    PowerRuneRotation, RUNE_TARGET_COUNT, RuneMode, RuneTargetStates, Team,
    apply_scene_control_power_rune_scenario, apply_scene_control_power_rune_state,
};
use crate::setup::{
    AutoAimSceneMode, AutoAimSceneState, ShootingRangeTarget, scene_is_available_in_build,
};
use crate::systems::{RangeTargetMotionMode, ShootingRangeControlState};

const PROTOCOL: &str = "daedalus.scene-control/2";
const DEFAULT_BIND: &str = "127.0.0.1:5603";
const MAX_COMMANDS_PER_FRAME: usize = 64;

#[derive(Debug)]
struct InboundDatagram {
    peer: SocketAddr,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct OutboundDatagram {
    peer: SocketAddr,
    bytes: Vec<u8>,
}

#[derive(Resource)]
pub(crate) struct SceneControlTransport {
    incoming: Receiver<InboundDatagram>,
    outgoing: Sender<OutboundDatagram>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl SceneControlTransport {
    fn start() -> std::io::Result<Self> {
        let bind_addr = std::env::var("DAEDALUS_SCENE_CONTROL_BIND")
            .unwrap_or_else(|_| DEFAULT_BIND.to_string());
        let socket = UdpSocket::bind(&bind_addr)?;
        socket.set_read_timeout(Some(Duration::from_millis(10)))?;
        let local_addr = socket.local_addr()?;
        let (incoming_tx, incoming) = bounded(256);
        let (outgoing, outgoing_rx) = bounded::<OutboundDatagram>(256);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("daedalus-scene-control-udp".to_string())
            .spawn(move || {
                let mut buffer = vec![0_u8; 65_507];
                let mut last_recoverable_error_log = Instant::now() - Duration::from_secs(1);
                while !worker_stop.load(Ordering::Acquire) {
                    while let Ok(response) = outgoing_rx.try_recv() {
                        let _ = socket.send_to(&response.bytes, response.peer);
                    }
                    match socket.recv_from(&mut buffer) {
                        Ok((len, peer)) => {
                            let _ = incoming_tx.try_send(InboundDatagram {
                                peer,
                                bytes: buffer[..len].to_vec(),
                            });
                        }
                        Err(error) if is_idle_udp_receive_error(&error) => {}
                        Err(error) if is_recoverable_udp_receive_error(&error) => {
                            if last_recoverable_error_log.elapsed() >= Duration::from_secs(1) {
                                warn!(
                                    "Scene control UDP receive recovered from {}: {error}",
                                    error.kind()
                                );
                                last_recoverable_error_log = Instant::now();
                            }
                        }
                        Err(error) => {
                            error!("Scene control UDP worker stopped after receive error: {error}");
                            break;
                        }
                    }
                }
                while let Ok(response) = outgoing_rx.try_recv() {
                    let _ = socket.send_to(&response.bytes, response.peer);
                }
            })?;
        info!("Scene control v2 listening on udp://{local_addr}");
        Ok(Self {
            incoming,
            outgoing,
            stop,
            worker: Some(worker),
        })
    }
}

fn is_idle_udp_receive_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

/// Windows reports a delayed ICMP port-unreachable response as ConnectionReset or
/// ConnectionRefused on the bound UDP socket.  A client retry may close its old
/// ephemeral socket before the simulator sends an ACK, so neither condition makes
/// the long-lived scene-control transport unusable.
fn is_recoverable_udp_receive_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::Interrupted
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionRefused
    )
}

impl Drop for SceneControlTransport {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Debug, Deserialize)]
struct SceneControlRequest {
    protocol: String,
    command_id: u64,
    session_id: String,
    op: String,
    args: Map<String, Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ResponseStatus {
    Ok,
    InvalidRequest,
    Unsupported,
    NotReady,
    InternalError,
}

#[derive(Debug, Serialize)]
struct SceneControlResponse {
    protocol: &'static str,
    command_id: u64,
    session_id: String,
    status: ResponseStatus,
    applied_frame_seq: u64,
    timestamp_ns: u64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Debug)]
struct PendingSceneResponse {
    peer: SocketAddr,
    command_id: u64,
    session_id: String,
    expected_mode: AutoAimSceneMode,
    expected_generation: u64,
    not_before_frame_seq: u64,
}

#[derive(Resource, Debug, Default)]
pub(crate) struct SceneControlRuntime {
    session_id: Option<String>,
    frame_seq: u64,
    pending_scene: Option<PendingSceneResponse>,
}

#[derive(Default)]
pub struct SceneControlPlugin;

impl Plugin for SceneControlPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SceneControlRuntime>();
        match SceneControlTransport::start() {
            Ok(transport) => {
                app.insert_resource(transport);
            }
            Err(error) => {
                error!("Scene control v2 failed to bind UDP socket: {error}");
            }
        }
    }
}

pub fn receive_scene_control_commands(
    transport: Option<Res<SceneControlTransport>>,
    mut runtime: ResMut<SceneControlRuntime>,
    mut scene_state: ResMut<AutoAimSceneState>,
    mut range_state: ResMut<ShootingRangeControlState>,
    mut rune_control: ResMut<ManualPowerRuneControlState>,
    rune_scores: Res<BigRuneScores>,
    armor_hits: Res<ArmorHitLedger>,
    mut runes: Query<(
        &mut PowerRune,
        &mut PowerRuneMechanism,
        &mut PowerRuneRotation,
    )>,
) {
    runtime.frame_seq = runtime.frame_seq.saturating_add(1).max(1);
    let Some(transport) = transport else {
        return;
    };
    let datagrams = transport
        .incoming
        .try_iter()
        .take(MAX_COMMANDS_PER_FRAME)
        .collect::<Vec<_>>();

    for datagram in datagrams {
        let request = match parse_request(&datagram.bytes) {
            Ok(request) => request,
            Err(message) => {
                let (command_id, session_id) = recover_envelope(&datagram.bytes);
                send_response(
                    &transport,
                    datagram.peer,
                    response(
                        command_id,
                        session_id,
                        ResponseStatus::InvalidRequest,
                        runtime.frame_seq,
                        message,
                    ),
                );
                continue;
            }
        };

        if request.op == "ping" {
            send_ok(
                &transport,
                datagram.peer,
                &request,
                runtime.frame_seq,
                "pong",
            );
            continue;
        }
        if request.op == "create_session" {
            if request.session_id.trim().is_empty() {
                send_status(
                    &transport,
                    datagram.peer,
                    &request,
                    ResponseStatus::InvalidRequest,
                    runtime.frame_seq,
                    "session_id must not be empty",
                );
            } else {
                runtime.session_id = Some(request.session_id.clone());
                runtime.pending_scene = None;
                range_state.reset_target_geometry();
                send_ok(
                    &transport,
                    datagram.peer,
                    &request,
                    runtime.frame_seq,
                    "session created",
                );
            }
            continue;
        }

        match runtime.session_id.as_deref() {
            None => {
                send_status(
                    &transport,
                    datagram.peer,
                    &request,
                    ResponseStatus::NotReady,
                    runtime.frame_seq,
                    "create_session is required",
                );
                continue;
            }
            Some(active) if active != request.session_id => {
                send_status(
                    &transport,
                    datagram.peer,
                    &request,
                    ResponseStatus::InvalidRequest,
                    runtime.frame_seq,
                    "session mismatch",
                );
                continue;
            }
            Some(_) => {}
        }

        match request.op.as_str() {
            "status" => send_ok(
                &transport,
                datagram.peer,
                &request,
                runtime.frame_seq,
                format!(
                    "scene={}; generation={}; scene_change_pending={}",
                    scene_name(scene_state.current),
                    scene_state.generation(),
                    runtime.pending_scene.is_some()
                ),
            ),
            "get_big_rune_score" => match parse_rune_team(&request.args) {
                Ok(team) => {
                    let score = rune_scores.for_team(team);
                    send_ok_data(
                        &transport,
                        datagram.peer,
                        &request,
                        runtime.frame_seq,
                        "big rune score read",
                        serde_json::json!({
                            "team": match team { Team::Red => "red", Team::Blue => "blue" },
                            "run_id": score.run_id(),
                            "run_active": score.run_active(),
                            "activated_arms": score.activated_arms(),
                            "has_hit": score.has_hit(),
                            "average_ring": score.average_ring(),
                            "last_ring": score.last_ring(),
                            "last_radius_mm": score.last_radius_mm(),
                            "last_target": score.last_target(),
                        }),
                    );
                }
                Err(message) => send_status(
                    &transport,
                    datagram.peer,
                    &request,
                    ResponseStatus::InvalidRequest,
                    runtime.frame_seq,
                    message,
                ),
            },
            "get_latest_armor_hit" => send_ok_data(
                &transport,
                datagram.peer,
                &request,
                runtime.frame_seq,
                "latest armor hit read",
                armor_hit_data(&armor_hits),
            ),
            "set_scene" => {
                if runtime.pending_scene.is_some() {
                    send_status(
                        &transport,
                        datagram.peer,
                        &request,
                        ResponseStatus::NotReady,
                        runtime.frame_seq,
                        "a scene change is already pending",
                    );
                    continue;
                }
                let mode_value = string_arg(&request.args, "scene")
                    .or_else(|_| string_arg(&request.args, "mode"));
                let mode = match mode_value.and_then(parse_scene_mode) {
                    Ok(mode) => mode,
                    Err(message) => {
                        send_status(
                            &transport,
                            datagram.peer,
                            &request,
                            ResponseStatus::InvalidRequest,
                            runtime.frame_seq,
                            message,
                        );
                        continue;
                    }
                };
                if !scene_is_available_in_build(mode) {
                    send_status(
                        &transport,
                        datagram.peer,
                        &request,
                        ResponseStatus::Unsupported,
                        runtime.frame_seq,
                        "contest release supports only energy and shooting_range",
                    );
                    continue;
                }
                if scene_state.current == mode && !scene_state.is_switch_pending() {
                    send_ok(
                        &transport,
                        datagram.peer,
                        &request,
                        runtime.frame_seq,
                        "scene already active",
                    );
                } else {
                    let expected_generation = scene_state.generation().saturating_add(1);
                    range_state.reset_target_geometry();
                    scene_state.request(mode);
                    runtime.pending_scene = Some(PendingSceneResponse {
                        peer: datagram.peer,
                        command_id: request.command_id,
                        session_id: request.session_id,
                        expected_mode: mode,
                        expected_generation,
                        not_before_frame_seq: runtime.frame_seq.saturating_add(1),
                    });
                }
            }
            "reset_scene" => {
                if runtime.pending_scene.is_some() {
                    send_status(
                        &transport,
                        datagram.peer,
                        &request,
                        ResponseStatus::NotReady,
                        runtime.frame_seq,
                        "a scene change is already pending",
                    );
                    continue;
                }
                let expected_generation = scene_state.generation().saturating_add(1);
                let expected_mode = scene_state.current;
                range_state.reset_target_geometry();
                scene_state.force_rebuild_current();
                runtime.pending_scene = Some(PendingSceneResponse {
                    peer: datagram.peer,
                    command_id: request.command_id,
                    session_id: request.session_id,
                    expected_mode,
                    expected_generation,
                    not_before_frame_seq: runtime.frame_seq.saturating_add(1),
                });
            }
            "set_range_target_motion" => {
                let result = parse_range_motion(&request.args).and_then(
                    |(target, mode, direction, speed, span, spin)| {
                        range_state
                            .set_target_motion(target, mode, direction, speed, span, spin)
                            .map_err(str::to_string)
                    },
                );
                match result {
                    Ok(()) => send_ok(
                        &transport,
                        datagram.peer,
                        &request,
                        runtime.frame_seq,
                        "range target motion applied",
                    ),
                    Err(message) => send_status(
                        &transport,
                        datagram.peer,
                        &request,
                        ResponseStatus::InvalidRequest,
                        runtime.frame_seq,
                        message,
                    ),
                }
            }
            "set_range_target_geometry" => {
                let result =
                    parse_range_geometry(&request.args).and_then(|(target, radial_scale)| {
                        range_state
                            .set_target_geometry(target, radial_scale)
                            .map_err(str::to_string)
                    });
                match result {
                    Ok(()) => send_ok(
                        &transport,
                        datagram.peer,
                        &request,
                        runtime.frame_seq,
                        "range target geometry applied",
                    ),
                    Err(message) => send_status(
                        &transport,
                        datagram.peer,
                        &request,
                        ResponseStatus::InvalidRequest,
                        runtime.frame_seq,
                        message,
                    ),
                }
            }
            "set_rune_state" => match parse_rune_state(&request.args) {
                Ok((mode, pending, activated)) => {
                    if apply_scene_control_power_rune_state(
                        mode,
                        &pending,
                        &activated,
                        &mut rune_control,
                        &mut runes,
                    ) {
                        send_ok(
                            &transport,
                            datagram.peer,
                            &request,
                            runtime.frame_seq,
                            "rune state applied",
                        );
                    } else {
                        send_status(
                            &transport,
                            datagram.peer,
                            &request,
                            ResponseStatus::NotReady,
                            runtime.frame_seq,
                            "power rune entity is not ready",
                        );
                    }
                }
                Err(message) => send_status(
                    &transport,
                    datagram.peer,
                    &request,
                    ResponseStatus::InvalidRequest,
                    runtime.frame_seq,
                    message,
                ),
            },
            "set_rune_scenario" => match parse_rune_scenario(&request.args) {
                Ok((mode, rule_driven, red_face_clockwise, leaf_states)) => {
                    if apply_scene_control_power_rune_scenario(
                        mode,
                        rule_driven,
                        red_face_clockwise,
                        leaf_states,
                        &mut rune_control,
                        &mut runes,
                    ) {
                        send_ok(
                            &transport,
                            datagram.peer,
                            &request,
                            runtime.frame_seq,
                            if rule_driven {
                                "rule-driven rune scenario applied"
                            } else {
                                "static rune scenario applied"
                            },
                        );
                    } else {
                        send_status(
                            &transport,
                            datagram.peer,
                            &request,
                            ResponseStatus::NotReady,
                            runtime.frame_seq,
                            "power rune entity is not ready",
                        );
                    }
                }
                Err(message) => send_status(
                    &transport,
                    datagram.peer,
                    &request,
                    ResponseStatus::InvalidRequest,
                    runtime.frame_seq,
                    message,
                ),
            },
            _ => send_status(
                &transport,
                datagram.peer,
                &request,
                ResponseStatus::Unsupported,
                runtime.frame_seq,
                "unsupported operation",
            ),
        }
    }
}

pub fn complete_scene_control_commands(
    transport: Option<Res<SceneControlTransport>>,
    mut runtime: ResMut<SceneControlRuntime>,
    scene_state: Res<AutoAimSceneState>,
    runes: Query<(), With<PowerRune>>,
    range_targets: Query<(), With<ShootingRangeTarget>>,
) {
    let Some(transport) = transport else {
        return;
    };
    let completed = runtime.pending_scene.as_ref().is_some_and(|pending| {
        let entities_ready = match pending.expected_mode {
            AutoAimSceneMode::Energy => runes.iter().next().is_some(),
            AutoAimSceneMode::ShootingRange => range_targets.iter().next().is_some(),
            AutoAimSceneMode::Armor | AutoAimSceneMode::Outpost => true,
        };
        runtime.frame_seq >= pending.not_before_frame_seq
            && scene_state.current == pending.expected_mode
            && scene_state.generation() >= pending.expected_generation
            && entities_ready
    });
    if !completed {
        return;
    }
    let pending = runtime.pending_scene.take().expect("checked above");
    send_response(
        &transport,
        pending.peer,
        response(
            pending.command_id,
            pending.session_id,
            ResponseStatus::Ok,
            runtime.frame_seq,
            format!("scene {} applied", scene_name(pending.expected_mode)),
        ),
    );
}

fn parse_request(bytes: &[u8]) -> Result<SceneControlRequest, String> {
    let request: SceneControlRequest =
        serde_json::from_slice(bytes).map_err(|error| format!("invalid request JSON: {error}"))?;
    if request.protocol != PROTOCOL {
        return Err(format!("protocol must be {PROTOCOL}"));
    }
    if request.op.trim().is_empty() {
        return Err("op must not be empty".to_string());
    }
    Ok(request)
}

fn recover_envelope(bytes: &[u8]) -> (u64, String) {
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return (0, String::new());
    };
    (
        value.get("command_id").and_then(Value::as_u64).unwrap_or(0),
        value
            .get("session_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    )
}

fn string_arg<'a>(args: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("args.{key} must be a string"))
}

fn f32_arg(args: &Map<String, Value>, key: &str) -> Result<f32, String> {
    let value = args
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("args.{key} must be a number"))?;
    if !value.is_finite() || value < f32::MIN as f64 || value > f32::MAX as f64 {
        return Err(format!("args.{key} is out of range"));
    }
    Ok(value as f32)
}

fn parse_scene_mode(value: &str) -> Result<AutoAimSceneMode, String> {
    match value {
        "armor" => Ok(AutoAimSceneMode::Armor),
        "energy" => Ok(AutoAimSceneMode::Energy),
        "outpost" => Ok(AutoAimSceneMode::Outpost),
        "shooting_range" => Ok(AutoAimSceneMode::ShootingRange),
        _ => Err("scene must be armor, energy, outpost, or shooting_range".to_string()),
    }
}

fn scene_name(mode: AutoAimSceneMode) -> &'static str {
    match mode {
        AutoAimSceneMode::Armor => "armor",
        AutoAimSceneMode::Energy => "energy",
        AutoAimSceneMode::Outpost => "outpost",
        AutoAimSceneMode::ShootingRange => "shooting_range",
    }
}

fn parse_range_motion(
    args: &Map<String, Value>,
) -> Result<(u8, RangeTargetMotionMode, f32, f32, f32, f32), String> {
    let target = args
        .get("target")
        .and_then(Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
        .ok_or_else(|| "args.target must be uint64 1 or 3".to_string())?;
    let mode =
        RangeTargetMotionMode::from_scene_control(string_arg(args, "mode")?).ok_or_else(|| {
            "args.mode must be stationary, linear, spin, or linear_and_spin".to_string()
        })?;
    Ok((
        target,
        mode,
        f32_arg(args, "direction_deg")?,
        f32_arg(args, "linear_speed_mps")?,
        f32_arg(args, "linear_span_m")?,
        f32_arg(args, "spin_deg_s")?,
    ))
}

fn parse_range_geometry(args: &Map<String, Value>) -> Result<(u8, f32), String> {
    let target = args
        .get("target")
        .and_then(Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
        .ok_or_else(|| "args.target must be uint64 1 or 3".to_string())?;
    Ok((target, f32_arg(args, "radial_scale")?))
}

fn target_indices(args: &Map<String, Value>, key: &str) -> Result<Vec<usize>, String> {
    let values = args
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("args.{key} must be an array"))?;
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        let index = value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value < RUNE_TARGET_COUNT)
            .ok_or_else(|| format!("args.{key} entries must be integers 0..4"))?;
        if result.contains(&index) {
            return Err(format!("args.{key} contains duplicate target {index}"));
        }
        result.push(index);
    }
    Ok(result)
}

fn parse_rune_state(
    args: &Map<String, Value>,
) -> Result<(Option<RuneMode>, Vec<usize>, Vec<usize>), String> {
    let mode = match string_arg(args, "mode")? {
        "off" => None,
        "small" => Some(RuneMode::Small),
        "large" => Some(RuneMode::Large),
        _ => return Err("args.mode must be off, small, or large".to_string()),
    };
    let pending = target_indices(args, "pending_targets")?;
    let activated = target_indices(args, "activated_targets")?;
    if pending.iter().any(|target| activated.contains(target)) {
        return Err("pending_targets and activated_targets must not overlap".to_string());
    }
    let pending_limit = match mode {
        None => 0,
        Some(RuneMode::Small) => 1,
        Some(RuneMode::Large) => 2,
    };
    if pending.len() > pending_limit {
        return Err(format!(
            "mode allows at most {pending_limit} pending targets"
        ));
    }
    if mode.is_none() && (!pending.is_empty() || !activated.is_empty()) {
        return Err("off mode requires empty target lists".to_string());
    }
    Ok((mode, pending, activated))
}

fn parse_rune_scenario(
    args: &Map<String, Value>,
) -> Result<(RuneMode, bool, bool, Option<RuneTargetStates>), String> {
    let mode = match string_arg(args, "mode")? {
        "small" => RuneMode::Small,
        "large" => RuneMode::Large,
        _ => return Err("args.mode must be small or large".to_string()),
    };
    let rule_driven = match string_arg(args, "motion")? {
        "rule" => true,
        "static" => false,
        _ => return Err("args.motion must be rule or static".to_string()),
    };
    let red_face_clockwise = match string_arg(args, "direction")? {
        "clockwise" => true,
        "counter_clockwise" => false,
        _ => return Err("args.direction must be clockwise or counter_clockwise".to_string()),
    };
    let leaves = args
        .get("leaf_states")
        .and_then(Value::as_array)
        .ok_or_else(|| "args.leaf_states must be an array".to_string())?;
    if rule_driven {
        if !leaves.is_empty() {
            return Err("rule motion requires an empty leaf_states array".to_string());
        }
        return Ok((mode, true, red_face_clockwise, None));
    }
    if leaves.len() != RUNE_TARGET_COUNT {
        return Err("static motion requires exactly five leaf_states".to_string());
    }
    let mut states = [Activation::Deactivated; RUNE_TARGET_COUNT];
    for (index, state) in leaves.iter().enumerate() {
        states[index] =
            match state.as_str() {
                Some("deactivated") => Activation::Deactivated,
                Some("activating") => Activation::Activating,
                Some("activated") => Activation::Activated,
                Some("completed") => Activation::Completed,
                _ => return Err(
                    "leaf_states values must be deactivated, activating, activated, or completed"
                        .to_string(),
                ),
            };
    }
    Ok((mode, false, red_face_clockwise, Some(states)))
}

fn parse_rune_team(args: &Map<String, Value>) -> Result<Team, String> {
    match string_arg(args, "team")? {
        "red" => Ok(Team::Red),
        "blue" => Ok(Team::Blue),
        _ => Err("args.team must be red or blue".to_string()),
    }
}

fn armor_hit_data(armor_hits: &ArmorHitLedger) -> Value {
    let Some(hit) = armor_hits.latest() else {
        return serde_json::json!({
            "has_hit": false,
            "latest_event_id": armor_hits.latest_event_id(),
        });
    };
    serde_json::json!({
        "has_hit": true,
        "latest_event_id": armor_hits.latest_event_id(),
        "event_id": hit.event_id,
        "has_projectile_id": hit.projectile_id.is_some(),
        "projectile_id": hit.projectile_id,
        "target_name": hit.target.name,
        "target_team": hit.target.team,
        "target_spec": hit.target.spec,
        "target_label": hit.target.label,
        "target_class": hit.target.class,
        "accurate_count": hit.accurate_count,
    })
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(u64::MAX as u128) as u64
}

fn response(
    command_id: u64,
    session_id: String,
    status: ResponseStatus,
    frame_seq: u64,
    message: impl Into<String>,
) -> SceneControlResponse {
    SceneControlResponse {
        protocol: PROTOCOL,
        command_id,
        session_id,
        status,
        applied_frame_seq: frame_seq,
        timestamp_ns: now_ns(),
        message: message.into(),
        data: None,
    }
}

fn send_ok_data(
    transport: &SceneControlTransport,
    peer: SocketAddr,
    request: &SceneControlRequest,
    frame_seq: u64,
    message: impl Into<String>,
    data: Value,
) {
    let mut response = response(
        request.command_id,
        request.session_id.clone(),
        ResponseStatus::Ok,
        frame_seq,
        message,
    );
    response.data = Some(data);
    send_response(transport, peer, response);
}

fn send_response(
    transport: &SceneControlTransport,
    peer: SocketAddr,
    response: SceneControlResponse,
) {
    match serde_json::to_vec(&response) {
        Ok(bytes) => {
            if transport
                .outgoing
                .try_send(OutboundDatagram { peer, bytes })
                .is_err()
            {
                warn!("Scene control response queue is full; dropping ACK for {peer}");
            }
        }
        Err(error) => error!("Scene control response serialization failed: {error}"),
    }
}

fn send_status(
    transport: &SceneControlTransport,
    peer: SocketAddr,
    request: &SceneControlRequest,
    status: ResponseStatus,
    frame_seq: u64,
    message: impl Into<String>,
) {
    send_response(
        transport,
        peer,
        response(
            request.command_id,
            request.session_id.clone(),
            status,
            frame_seq,
            message,
        ),
    );
}

fn send_ok(
    transport: &SceneControlTransport,
    peer: SocketAddr,
    request: &SceneControlRequest,
    frame_seq: u64,
    message: impl Into<String>,
) {
    send_status(
        transport,
        peer,
        request,
        ResponseStatus::Ok,
        frame_seq,
        message,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_ping_request() {
        let request = parse_request(
            br#"{"protocol":"daedalus.scene-control/2","command_id":7,"session_id":"s","op":"ping","args":{}}"#,
        )
        .unwrap();
        assert_eq!(request.command_id, 7);
        assert_eq!(request.op, "ping");
    }

    #[test]
    fn rejects_wrong_protocol() {
        let error = parse_request(
            br#"{"protocol":"wrong","command_id":7,"session_id":"s","op":"ping","args":{}}"#,
        )
        .unwrap_err();
        assert!(error.contains(PROTOCOL));
    }

    #[test]
    fn validates_range_target_and_rune_arguments() {
        let range = serde_json::from_value::<Map<String, Value>>(serde_json::json!({
            "target": 3,
            "mode": "linear_and_spin",
            "direction_deg": 90.0,
            "linear_speed_mps": 1.5,
            "linear_span_m": 8.0,
            "spin_deg_s": 45.0
        }))
        .unwrap();
        assert!(parse_range_motion(&range).is_ok());

        let geometry = serde_json::from_value::<Map<String, Value>>(serde_json::json!({
            "target": 3,
            "radial_scale": 1.2
        }))
        .unwrap();
        assert_eq!(parse_range_geometry(&geometry).unwrap(), (3, 1.2));

        let rune = serde_json::from_value::<Map<String, Value>>(serde_json::json!({
            "mode": "large",
            "pending_targets": [2],
            "activated_targets": [0, 4]
        }))
        .unwrap();
        assert!(parse_rune_state(&rune).is_ok());

        let rule = serde_json::from_value::<Map<String, Value>>(serde_json::json!({
            "mode": "large", "motion": "rule", "direction": "clockwise", "leaf_states": []
        }))
        .unwrap();
        assert!(matches!(
            parse_rune_scenario(&rule),
            Ok((RuneMode::Large, true, true, None))
        ));
        let team = serde_json::from_value::<Map<String, Value>>(serde_json::json!({
            "team": "red"
        }))
        .unwrap();
        assert!(matches!(parse_rune_team(&team), Ok(Team::Red)));

        let static_frame = serde_json::from_value::<Map<String, Value>>(serde_json::json!({
            "mode": "small", "motion": "static", "direction": "counter_clockwise",
            "leaf_states": ["activating", "activated", "completed", "deactivated", "deactivated"]
        }))
        .unwrap();
        assert!(parse_rune_scenario(&static_frame).is_ok());
    }

    #[test]
    fn preserves_udp_worker_for_delayed_ack_receive_errors() {
        for kind in [
            std::io::ErrorKind::Interrupted,
            std::io::ErrorKind::ConnectionReset,
            std::io::ErrorKind::ConnectionRefused,
        ] {
            assert!(is_recoverable_udp_receive_error(&std::io::Error::from(
                kind
            )));
        }
        assert!(!is_recoverable_udp_receive_error(&std::io::Error::from(
            std::io::ErrorKind::PermissionDenied
        )));
        assert!(is_idle_udp_receive_error(&std::io::Error::from(
            std::io::ErrorKind::TimedOut
        )));
    }

    #[test]
    fn armor_hit_response_is_empty_until_a_valid_hit_is_recorded() {
        let ledger = ArmorHitLedger::default();
        let data = armor_hit_data(&ledger);
        assert_eq!(data["has_hit"], false);
        assert_eq!(data["latest_event_id"], 0);
    }
}
