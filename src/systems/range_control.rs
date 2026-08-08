use avian3d::prelude::{AngularVelocity, LinearVelocity};
use bevy::prelude::*;
use bevy::window::{PresentMode, Window, WindowPlugin, WindowPosition, WindowResolution};
use bevy_inspector_egui::bevy_egui::{
    EguiContexts, EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext, egui,
};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use crate::capture::ExposureWallTimestamp;
use crate::components::{
    ActiveSlapper, Controlled, Infantry, InfantryGimbal, InfantryLaunchOffset, MainCamera,
    SubscribeAutoAim,
};
use crate::integrated_auto_aim::IntegratedAutoAimBridge;
use crate::robomaster::prelude::ArmorRoot;
use crate::setup::{
    AutoAimSceneMode, AutoAimSceneState, ShootingRangeTarget, ShootingRangeTargetKind,
};
use crate::statistic::ProjectileStatistics;

const RANGE_TARGET_MAX_LINEAR_SPAN_M: f32 = 8.0;
const RANGE_TARGET_DEFAULT_LINEAR_SPAN_M: f32 = 8.0;
const RANGE_TARGET_MIN_RADIAL_SCALE: f32 = 0.75;
const RANGE_TARGET_MAX_RADIAL_SCALE: f32 = 1.25;
const RANGE_TARGET_BOUNDARY_EPSILON_M: f32 = 1e-3;
const TRUTH_GIMBAL_ENV: &str = "DAEDALUS_RANGE_TRUTH_GIMBAL_TARGET_NUMBER";
const TRUTH_GIMBAL_SOLVE_EPSILON_RAD: f32 = 1e-5;
const TRUTH_GIMBAL_NUMERIC_STEP_RAD: f32 = 1e-4;
const TRUTH_GIMBAL_MAX_SOLVE_STEPS: usize = 12;
const TRUTH_GIMBAL_MAX_NEWTON_STEP_RAD: f32 = 1.0;

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShootingRangeTruthGimbalConfig {
    pub target_number: Option<u8>,
}

impl ShootingRangeTruthGimbalConfig {
    fn from_value(value: Option<&str>) -> Self {
        let target_number = value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .and_then(|value| value.parse::<u8>().ok())
            .filter(|number| matches!(number, 1 | 3));
        Self { target_number }
    }

    pub fn from_env() -> Self {
        Self::from_value(std::env::var(TRUTH_GIMBAL_ENV).ok().as_deref())
    }

    pub const fn enabled(self) -> bool {
        self.target_number.is_some()
    }
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct ShootingRangeTruthGimbalStats {
    pub enabled: bool,
    pub target_number: Option<u8>,
    pub command_count: u64,
    pub last_exposure_timestamp_ns: u64,
    pub last_command_yaw_rad: f32,
    pub last_command_pitch_rad: f32,
    pub last_optical_axis_error_rad: f32,
}

impl ShootingRangeTruthGimbalStats {
    pub fn from_config(config: ShootingRangeTruthGimbalConfig) -> Self {
        Self {
            enabled: config.enabled(),
            target_number: config.target_number,
            ..default()
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RangeTargetWallPose {
    translation: Vec3,
    rotation: Quat,
    last_real_elapsed_s: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RangeTargetMotionMode {
    #[default]
    Stationary,
    Linear,
    Spin,
    LinearAndSpin,
}

impl RangeTargetMotionMode {
    pub(crate) fn from_scene_control(value: &str) -> Option<Self> {
        match value {
            "stationary" => Some(Self::Stationary),
            "linear" => Some(Self::Linear),
            "spin" => Some(Self::Spin),
            "linear_and_spin" => Some(Self::LinearAndSpin),
            _ => None,
        }
    }
}

#[derive(Component)]
pub struct SceneModeUiRoot;

#[derive(Component)]
pub struct SceneModeUiButton {
    mode: AutoAimSceneMode,
}

#[derive(Component)]
pub struct SceneModeUiStatus;

#[derive(Clone, Debug, PartialEq)]
pub struct RangeTargetMotionSettings {
    pub mode: RangeTargetMotionMode,
    pub direction_deg: f32,
    pub linear_speed_mps: f32,
    pub linear_span_m: f32,
    pub spin_deg_s: f32,
    pub travel_sign: f32,
    /// Horizontal armor-root scale relative to stock target geometry.
    pub radial_scale: f32,
}

impl Default for RangeTargetMotionSettings {
    fn default() -> Self {
        Self {
            mode: RangeTargetMotionMode::Stationary,
            direction_deg: 90.0,
            linear_speed_mps: 0.0,
            linear_span_m: RANGE_TARGET_DEFAULT_LINEAR_SPAN_M,
            spin_deg_s: 0.0,
            travel_sign: 1.0,
            radial_scale: 1.0,
        }
    }
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct ShootingRangeControlState {
    pub open: bool,
    pub armor_3: RangeTargetMotionSettings,
    pub armor_1: RangeTargetMotionSettings,
}

impl Default for ShootingRangeControlState {
    fn default() -> Self {
        Self {
            open: false,
            armor_3: default_motion_for_target(ShootingRangeTargetKind::Armor3),
            armor_1: default_motion_for_target(ShootingRangeTargetKind::Armor1),
        }
    }
}

impl ShootingRangeControlState {
    pub(crate) fn set_target_motion(
        &mut self,
        target: u8,
        mode: RangeTargetMotionMode,
        direction_deg: f32,
        linear_speed_mps: f32,
        linear_span_m: f32,
        spin_deg_s: f32,
    ) -> Result<(), &'static str> {
        if !direction_deg.is_finite()
            || !linear_speed_mps.is_finite()
            || !linear_span_m.is_finite()
            || !spin_deg_s.is_finite()
            || linear_speed_mps < 0.0
            || !(0.0..=RANGE_TARGET_MAX_LINEAR_SPAN_M).contains(&linear_span_m)
        {
            return Err("invalid range target motion parameters");
        }
        let settings = match target {
            1 => &mut self.armor_1,
            3 => &mut self.armor_3,
            _ => return Err("target must be 1 or 3"),
        };
        let radial_scale = settings.radial_scale;
        *settings = RangeTargetMotionSettings {
            mode,
            direction_deg,
            linear_speed_mps,
            linear_span_m,
            spin_deg_s,
            travel_sign: 1.0,
            radial_scale,
        };
        Ok(())
    }

    pub(crate) fn set_target_geometry(
        &mut self,
        target: u8,
        radial_scale: f32,
    ) -> Result<(), &'static str> {
        if !radial_scale.is_finite()
            || !(RANGE_TARGET_MIN_RADIAL_SCALE..=RANGE_TARGET_MAX_RADIAL_SCALE)
                .contains(&radial_scale)
        {
            return Err("radial_scale must be finite and within [0.75, 1.25]");
        }
        let settings = match target {
            1 => &mut self.armor_1,
            3 => &mut self.armor_3,
            _ => return Err("target must be 1 or 3"),
        };
        if settings.mode != RangeTargetMotionMode::Stationary {
            return Err("target must be stationary before geometry changes");
        }
        settings.radial_scale = radial_scale;
        Ok(())
    }

    pub(crate) fn reset_target_geometry(&mut self) {
        self.armor_1.radial_scale = 1.0;
        self.armor_3.radial_scale = 1.0;
    }
}

fn default_motion_for_target(kind: ShootingRangeTargetKind) -> RangeTargetMotionSettings {
    let mut settings = RangeTargetMotionSettings::default();
    let active_number = std::env::var("DAEDALUS_RANGE_ACTIVE_TARGET_NUMBER")
        .ok()
        .and_then(|value| value.trim().parse::<u8>().ok())
        .unwrap_or(3);

    if active_number == kind.number() {
        apply_motion_env(&mut settings, "DAEDALUS_RANGE_");
    }

    let target_prefix = match kind {
        ShootingRangeTargetKind::Armor3 => "DAEDALUS_RANGE_TARGET3_",
        ShootingRangeTargetKind::Armor1 => "DAEDALUS_RANGE_TARGET1_",
    };
    apply_motion_env(&mut settings, target_prefix);
    settings
}

fn apply_motion_env(settings: &mut RangeTargetMotionSettings, prefix: &str) {
    let mode_key = format!("{prefix}MOTION_MODE");
    let mode_was_set = env_string(&mode_key)
        .and_then(|value| parse_motion_mode(&value))
        .map(|mode| {
            settings.mode = mode;
        })
        .is_some();

    if let Some(direction_deg) = env_f32(&format!("{prefix}LINEAR_DIRECTION_DEG")) {
        settings.direction_deg = direction_deg;
    }
    if let Some(linear_speed_mps) = env_f32(&format!("{prefix}LINEAR_SPEED_MPS")) {
        settings.linear_speed_mps = linear_speed_mps.max(0.0);
    }
    if let Some(linear_span_m) = env_f32(&format!("{prefix}LINEAR_SPAN_M")) {
        settings.linear_span_m = linear_span_m.clamp(0.0, RANGE_TARGET_MAX_LINEAR_SPAN_M);
    }
    if let Some(spin_deg_s) = env_f32(&format!("{prefix}SPIN_DEG_S")) {
        settings.spin_deg_s = spin_deg_s;
    }
    if let Some(radial_scale) = env_f32(&format!("{prefix}RADIAL_SCALE")) {
        settings.radial_scale =
            radial_scale.clamp(RANGE_TARGET_MIN_RADIAL_SCALE, RANGE_TARGET_MAX_RADIAL_SCALE);
    }

    if !mode_was_set {
        settings.mode = inferred_motion_mode(settings);
    }
}

fn env_string(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_f32(key: &str) -> Option<f32> {
    env_string(key).and_then(|value| value.parse::<f32>().ok())
}

fn parse_motion_mode(value: &str) -> Option<RangeTargetMotionMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "stationary" | "stop" | "stopped" | "none" => Some(RangeTargetMotionMode::Stationary),
        "linear" | "line" | "translate" | "translation" => Some(RangeTargetMotionMode::Linear),
        "spin" | "rotate" | "rotation" => Some(RangeTargetMotionMode::Spin),
        "linear_spin" | "linear+spin" | "line+spin" | "combined" | "linearandspin" => {
            Some(RangeTargetMotionMode::LinearAndSpin)
        }
        _ => None,
    }
}

fn inferred_motion_mode(settings: &RangeTargetMotionSettings) -> RangeTargetMotionMode {
    let has_linear = settings.linear_speed_mps > 0.0;
    let has_spin = settings.spin_deg_s.abs() > 0.0;
    match (has_linear, has_spin) {
        (true, true) => RangeTargetMotionMode::LinearAndSpin,
        (true, false) => RangeTargetMotionMode::Linear,
        (false, true) => RangeTargetMotionMode::Spin,
        (false, false) => RangeTargetMotionMode::Stationary,
    }
}

#[derive(Resource, Debug)]
pub struct ShootingRangeDebugPanelState {
    bridge_path: PathBuf,
    pipeline_path: PathBuf,
    bridge: Option<Value>,
    pipeline: Option<Value>,
    history: VecDeque<DebugHistorySample>,
    last_history_frame: Option<i64>,
    last_error: Option<String>,
    refresh_timer: Timer,
}

#[derive(Clone, Copy, Debug)]
struct DebugHistorySample {
    t_s: f64,
    image_yaw_deg: f64,
    image_pitch_deg: f64,
    command_yaw_deg: f64,
    command_pitch_deg: f64,
    distance_m: f64,
    confidence: f64,
    center_error_px: f64,
    runtime_lag_ms: f64,
    fire_advice: f64,
}

struct ChartSeries {
    label: &'static str,
    color: egui::Color32,
    value: fn(&DebugHistorySample) -> f64,
}

const DEBUG_HISTORY_CAPACITY: usize = 360;

impl Default for ShootingRangeDebugPanelState {
    fn default() -> Self {
        Self {
            bridge_path: resolve_debug_json_path(
                std::env::var_os("AIM_SIM_DEBUG_BRIDGE_JSON"),
                std::env::var_os("DAEDALUS_WORKSPACE_DIR"),
                std::env::current_dir().ok().as_deref(),
                "aim_bridge.json",
            ),
            pipeline_path: resolve_debug_json_path(
                std::env::var_os("AIM_SIM_DEBUG_PIPELINE_JSON"),
                std::env::var_os("DAEDALUS_WORKSPACE_DIR"),
                std::env::current_dir().ok().as_deref(),
                "aim_pipeline.json",
            ),
            bridge: None,
            pipeline: None,
            history: VecDeque::with_capacity(DEBUG_HISTORY_CAPACITY),
            last_history_frame: None,
            last_error: None,
            refresh_timer: Timer::from_seconds(0.25, TimerMode::Repeating),
        }
    }
}

#[derive(Resource, Default)]
pub struct ShootingRangeDebugProcessState {
    child: Option<Child>,
}

impl Drop for ShootingRangeDebugProcessState {
    fn drop(&mut self) {
        stop_debug_ui_process(&mut self.child);
    }
}

pub fn spawn_scene_mode_button_panel(mut commands: Commands) {
    commands
        .spawn((
            SceneModeUiRoot,
            GlobalZIndex(20),
            BackgroundColor(Color::srgba(0.04, 0.05, 0.06, 0.84)),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                left: Val::Px(12.0),
                width: Val::Px(470.0),
                padding: UiRect::all(Val::Px(8.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(7.0),
                ..default()
            },
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("Simulation Control"),
                TextFont {
                    font_size: FontSize::Px(15.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            panel
                .spawn((Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(8.0),
                    ..default()
                },))
                .with_children(|row| {
                    row.spawn((
                        Button,
                        SceneModeUiButton {
                            mode: AutoAimSceneMode::Armor,
                        },
                        BackgroundColor(scene_button_normal_color()),
                        Node {
                            width: Val::Px(140.0),
                            height: Val::Px(30.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                    ))
                    .with_child((
                        Text::new(AutoAimSceneMode::Armor.label()),
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                    row.spawn((
                        Button,
                        SceneModeUiButton {
                            mode: AutoAimSceneMode::Energy,
                        },
                        BackgroundColor(scene_button_normal_color()),
                        Node {
                            width: Val::Px(140.0),
                            height: Val::Px(30.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                    ))
                    .with_child((
                        Text::new(AutoAimSceneMode::Energy.label()),
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                    row.spawn((
                        Button,
                        SceneModeUiButton {
                            mode: AutoAimSceneMode::ShootingRange,
                        },
                        BackgroundColor(scene_button_normal_color()),
                        Node {
                            width: Val::Px(140.0),
                            height: Val::Px(30.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                    ))
                    .with_child((
                        Text::new(AutoAimSceneMode::ShootingRange.label()),
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
            panel.spawn((
                SceneModeUiStatus,
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::srgb(0.88, 0.91, 0.94)),
            ));
        });
}

pub fn update_scene_mode_button_panel(
    scene_state: Res<AutoAimSceneState>,
    mut buttons: Query<(&SceneModeUiButton, &mut BackgroundColor)>,
    mut status_text: Query<&mut Text, With<SceneModeUiStatus>>,
) {
    for (button, mut background) in &mut buttons {
        *background = BackgroundColor(if scene_state.requested == button.mode {
            scene_button_selected_color()
        } else {
            scene_button_normal_color()
        });
    }

    for mut text in &mut status_text {
        let pending = if scene_state.is_switch_pending() {
            format!("\npending: {}", scene_state.requested.label())
        } else {
            String::new()
        };
        *text = Text::new(format!(
            "current: {}{}",
            scene_state.current.label(),
            pending
        ));
    }
}

pub fn handle_scene_mode_button_interactions(
    mut interactions: Query<
        (&Interaction, &SceneModeUiButton),
        (Changed<Interaction>, With<Button>),
    >,
    mut scene_state: ResMut<AutoAimSceneState>,
) {
    for (interaction, button) in &mut interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if scene_state.requested != button.mode {
            scene_state.request(button.mode);
            info!("Scene switch requested from UI: {}.", button.mode.label());
        }
    }
}

fn scene_button_normal_color() -> Color {
    Color::srgba(0.16, 0.18, 0.20, 0.94)
}

fn scene_button_selected_color() -> Color {
    Color::srgba(0.05, 0.36, 0.76, 0.96)
}

pub fn toggle_shooting_range_control_window(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<ShootingRangeControlState>,
) {
    if keyboard.just_pressed(KeyCode::F6) {
        state.open = !state.open;
        info!(
            "Shooting range target control window is now {}.",
            if state.open { "open" } else { "closed" }
        );
    }
}

pub fn scene_mode_keyboard_shortcuts(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut scene_state: ResMut<AutoAimSceneState>,
) {
    if keyboard.just_pressed(KeyCode::F7) {
        scene_state.request(AutoAimSceneMode::Armor);
        info!("Scene switch requested: Normal Map.");
    }
    if keyboard.just_pressed(KeyCode::F8) {
        scene_state.request(AutoAimSceneMode::ShootingRange);
        info!("Scene switch requested: Shooting Range.");
    }
    if keyboard.just_pressed(KeyCode::F9) {
        scene_state.request(AutoAimSceneMode::Energy);
        info!("Scene switch requested: Energy Mechanism.");
    }
}

pub fn scene_mode_control_panel(
    mut contexts: EguiContexts,
    mut scene_state: ResMut<AutoAimSceneState>,
    mut range_control: ResMut<ShootingRangeControlState>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let mut requested = scene_state.requested;
    egui::Window::new("Simulation Control")
        .resizable(false)
        .collapsible(false)
        .fixed_pos(egui::pos2(12.0, 12.0))
        .fixed_size(egui::vec2(470.0, 128.0))
        .show(ctx, |ui| {
            ui.set_min_width(440.0);
            ui.horizontal(|ui| {
                mode_button(ui, &mut requested, AutoAimSceneMode::Armor);
                mode_button(ui, &mut requested, AutoAimSceneMode::Energy);
                mode_button(ui, &mut requested, AutoAimSceneMode::ShootingRange);
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("current");
                ui.monospace(scene_state.current.label());
            });
            if scene_state.is_switch_pending() || requested != scene_state.current {
                ui.horizontal(|ui| {
                    ui.label("pending");
                    ui.monospace(requested.label());
                });
            }

            ui.add_enabled_ui(
                scene_state.current == AutoAimSceneMode::ShootingRange
                    || requested == AutoAimSceneMode::ShootingRange,
                |ui| {
                    ui.checkbox(&mut range_control.open, "Range Target Control");
                },
            );
        });

    if requested != scene_state.requested {
        scene_state.request(requested);
    }
}

pub fn manage_shooting_range_debug_process(
    targets: Query<(), With<ShootingRangeTarget>>,
    mut state: ResMut<ShootingRangeDebugProcessState>,
) {
    let in_range = targets.iter().next().is_some();

    if let Some(child) = state.child.as_mut() {
        match child.try_wait() {
            Ok(Some(status)) => {
                info!("Auto-aim debug UI exited: {status}");
                state.child = None;
            }
            Ok(None) => {}
            Err(error) => {
                warn!("Could not query auto-aim debug UI process: {error}");
                state.child = None;
            }
        }
    }

    if in_range && state.child.is_none() {
        match spawn_debug_ui_process() {
            Ok(child) => {
                info!("Started auto-aim debug UI process.");
                state.child = Some(child);
            }
            Err(error) => warn!("Failed to start auto-aim debug UI process: {error}"),
        }
    } else if !in_range && state.child.is_some() {
        stop_debug_ui_process(&mut state.child);
    }
}

fn spawn_debug_ui_process() -> Result<Child, String> {
    let exe = std::env::current_exe().map_err(|error| format!("current_exe failed: {error}"))?;
    let mut command = Command::new(exe);
    command.arg("--debug-ui");
    if std::env::var_os("DAEDALUS_DEBUG_WINDOW_X").is_none()
        || std::env::var_os("DAEDALUS_DEBUG_WINDOW_Y").is_none()
    {
        if let (Some(x), Some(y)) = (env_i32("DAEDALUS_WINDOW_X"), env_i32("DAEDALUS_WINDOW_Y")) {
            command.env("DAEDALUS_DEBUG_WINDOW_X", (x + 96).to_string());
            command.env("DAEDALUS_DEBUG_WINDOW_Y", (y + 96).to_string());
        }
    }
    if std::env::var_os("DAEDALUS_WORKSPACE_DIR").is_none() {
        if let Some(workspace) = default_workspace_root_from_manifest() {
            command.env("DAEDALUS_WORKSPACE_DIR", workspace);
        }
    }
    command
        .spawn()
        .map_err(|error| format!("spawn failed: {error}"))
}

fn stop_debug_ui_process(child: &mut Option<Child>) {
    if let Some(mut child) = child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

pub fn run_auto_aim_debug_ui_app() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(debug_ui_window()),
                ..default()
            }),
            EguiPlugin::default(),
        ))
        .init_resource::<ShootingRangeDebugPanelState>()
        .add_systems(Startup, setup_debug_ui_camera)
        .add_systems(EguiPrimaryContextPass, standalone_auto_aim_debug_window)
        .run();
}

fn debug_ui_window() -> Window {
    let mut window = Window {
        title: "Daedalus Auto-Aim Debug".to_string(),
        resolution: WindowResolution::new(980, 720),
        present_mode: PresentMode::AutoVsync,
        ..default()
    };

    let position = match (
        env_i32("DAEDALUS_DEBUG_WINDOW_X"),
        env_i32("DAEDALUS_DEBUG_WINDOW_Y"),
    ) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => match (env_i32("DAEDALUS_WINDOW_X"), env_i32("DAEDALUS_WINDOW_Y")) {
            (Some(x), Some(y)) => Some((x + 96, y + 96)),
            _ => None,
        },
    };

    if let Some((x, y)) = position {
        window.position = WindowPosition::At(IVec2::new(x, y));
    }

    window
}

fn env_i32(key: &str) -> Option<i32> {
    std::env::var(key)
        .ok()
        .and_then(|value| match value.trim().parse::<i32>() {
            Ok(parsed) => Some(parsed),
            Err(error) => {
                warn!("Ignoring invalid {key}={value:?}: {error}");
                None
            }
        })
}

fn setup_debug_ui_camera(mut commands: Commands) {
    commands.spawn((Camera2d::default(), PrimaryEguiContext));
}

fn refresh_debug_telemetry(time: &Time, state: &mut ShootingRangeDebugPanelState) {
    state.refresh_timer.tick(time.delta());
    if !state.refresh_timer.just_finished() {
        return;
    }

    let bridge_path = state.bridge_path.clone();
    let pipeline_path = state.pipeline_path.clone();
    state.bridge = read_json_file(&bridge_path, &mut state.last_error);
    state.pipeline = read_json_file(&pipeline_path, &mut state.last_error);
    push_debug_history_sample(time.elapsed_secs_f64(), state);
}

fn push_debug_history_sample(now_s: f64, state: &mut ShootingRangeDebugPanelState) {
    let pipeline = state.pipeline.as_ref();
    let Some(frame_count) = path_i64(pipeline, "frame_count") else {
        return;
    };
    if state.last_history_frame == Some(frame_count) {
        return;
    }
    state.last_history_frame = Some(frame_count);

    let distance_m = path_number(pipeline, "aim_command.distance_m")
        .filter(|value| *value > 0.0)
        .or_else(|| path_number(pipeline, "first_solved.distance_mm").map(|value| value * 0.001))
        .unwrap_or(f64::NAN);
    let fire_advice = path_bool(pipeline, "aim_command.fire_advice")
        .map(|value| if value { 1.0 } else { 0.0 })
        .unwrap_or(f64::NAN);

    state.history.push_back(DebugHistorySample {
        t_s: now_s,
        image_yaw_deg: path_number(pipeline, "first_solved.image_yaw_deg").unwrap_or(f64::NAN),
        image_pitch_deg: path_number(pipeline, "first_solved.image_pitch_down_positive_deg")
            .unwrap_or(f64::NAN),
        command_yaw_deg: path_number(pipeline, "aim_command.yaw_deg").unwrap_or(f64::NAN),
        command_pitch_deg: path_number(pipeline, "aim_command.pitch_deg").unwrap_or(f64::NAN),
        distance_m,
        confidence: path_number(pipeline, "detector.first.confidence").unwrap_or(f64::NAN),
        center_error_px: path_number(pipeline, "first_solved.distance_to_image_center_px")
            .unwrap_or(f64::NAN),
        runtime_lag_ms: path_number(state.bridge.as_ref(), "gimbal.runtime_minus_image_ms")
            .unwrap_or(f64::NAN),
        fire_advice,
    });

    while state.history.len() > DEBUG_HISTORY_CAPACITY {
        state.history.pop_front();
    }
}

fn standalone_auto_aim_debug_window(
    mut contexts: EguiContexts,
    time: Res<Time>,
    mut state: ResMut<ShootingRangeDebugPanelState>,
) {
    refresh_debug_telemetry(&time, &mut state);

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let bridge = state.bridge.as_ref();
    let pipeline = state.pipeline.as_ref();
    egui::Window::new("Daedalus Auto-Aim Debug")
        .vscroll(true)
        .default_size(egui::vec2(940.0, 680.0))
        .show(ctx, |ui| {
            ui.heading("Daedalus Auto-Aim Debug");
            draw_debug_charts(ui, &state.history);
            ui.separator();
            ui.columns(3, |columns| {
                columns[0].group(|ui| {
                    ui.heading("Runtime");
                    debug_kv(
                        ui,
                        "telemetry",
                        if bridge.is_some() || pipeline.is_some() {
                            "live"
                        } else {
                            "waiting"
                        },
                    );
                    debug_kv(
                        ui,
                        "runtime yaw",
                        &deg(path_number(bridge, "gimbal.runtime_local_yaw_deg")),
                    );
                    debug_kv(
                        ui,
                        "runtime pitch",
                        &deg(path_number(bridge, "gimbal.runtime_pitch_deg")),
                    );
                    debug_kv(
                        ui,
                        "vivsionn yaw",
                        &deg(path_number(bridge, "gimbal.vivsionn_yaw_deg")),
                    );
                    debug_kv(
                        ui,
                        "input pitch",
                        &deg(path_number(pipeline, "input_gimbal_pitch_deg")),
                    );
                });
                columns[1].group(|ui| {
                    ui.heading("Target");
                    debug_kv(
                        ui,
                        "detector",
                        &int_text(path_i64(pipeline, "detector.count")),
                    );
                    debug_kv(ui, "solved", &int_text(path_i64(pipeline, "solved_count")));
                    debug_kv(
                        ui,
                        "center",
                        &point_text(path_value(pipeline, "first_solved.center_px")),
                    );
                    debug_kv(
                        ui,
                        "image yaw",
                        &deg(path_number(pipeline, "first_solved.image_yaw_deg")),
                    );
                    debug_kv(
                        ui,
                        "image pitch",
                        &deg(path_number(
                            pipeline,
                            "first_solved.image_pitch_down_positive_deg",
                        )),
                    );
                    debug_kv(
                        ui,
                        "distance",
                        &mm_text(path_number(pipeline, "first_solved.distance_mm")),
                    );
                    debug_kv(
                        ui,
                        "camera",
                        &format!(
                            "{} / {}",
                            text(path_value(pipeline, "camera.profile_id")),
                            format!(
                                "{}mm",
                                number_text(path_number(pipeline, "camera.focal_mm"), 1)
                            )
                        ),
                    );
                    debug_kv(
                        ui,
                        "dual focal",
                        &format!(
                            "{} -> {}",
                            text(path_value(pipeline, "dual_focal.reason")),
                            text(path_value(pipeline, "dual_focal.next_selected"))
                        ),
                    );
                    debug_kv(
                        ui,
                        "subpixel delta",
                        &format!(
                            "{} / {} px",
                            number_text(
                                path_number(pipeline, "subpixel_refinement.mean_delta_px"),
                                2
                            ),
                            number_text(
                                path_number(pipeline, "subpixel_refinement.max_delta_px"),
                                2
                            )
                        ),
                    );
                });
                columns[2].group(|ui| {
                    ui.heading("Fire");
                    debug_kv(
                        ui,
                        "cmd yaw",
                        &format!(
                            "{} -> {}",
                            deg(path_number(pipeline, "fire_control.raw_command_yaw_deg")),
                            deg(path_number(
                                pipeline,
                                "fire_control.filtered_command_yaw_deg"
                            ))
                        ),
                    );
                    debug_kv(
                        ui,
                        "cmd pitch",
                        &format!(
                            "{} -> {}",
                            deg(path_number(pipeline, "fire_control.raw_command_pitch_deg")),
                            deg(path_number(
                                pipeline,
                                "fire_control.filtered_command_pitch_deg"
                            ))
                        ),
                    );
                    debug_kv(
                        ui,
                        "yaw speed",
                        &format!(
                            "{} deg/s",
                            number_text(path_number(pipeline, "fire_control.yaw_speed_deg_s"), 2)
                        ),
                    );
                    debug_kv(
                        ui,
                        "fire",
                        bool_text(path_bool(pipeline, "aim_command.fire_advice")),
                    );
                    debug_kv(
                        ui,
                        "gates",
                        &format!(
                            "valid={} mcu={} stable={} follow={} preview={}",
                            bool_text(path_bool(pipeline, "fire_control.fire_gate_valid")),
                            bool_text(path_bool(pipeline, "fire_control.fire_gate_mcu_permit")),
                            bool_text(path_bool(pipeline, "fire_control.fire_gate_command_stable")),
                            bool_text(path_bool(pipeline, "fire_control.fire_gate_follow")),
                            bool_text(path_bool(pipeline, "fire_control.fire_gate_preview"))
                        ),
                    );
                });
            });
            ui.separator();
            ui.columns(2, |columns| {
                columns[0].group(|ui| {
                    ui.heading("Filter");
                    debug_kv(
                        ui,
                        "tracker",
                        &text(path_value(pipeline, "tracker.tracker_state")),
                    );
                    debug_kv(
                        ui,
                        "movement",
                        &text(path_value(pipeline, "tracker.movement")),
                    );
                    debug_kv(
                        ui,
                        "v_t / v_r / v_xy",
                        &format!(
                            "{} / {} / {}",
                            number_text(path_number(pipeline, "tracker.v_t"), 3),
                            number_text(path_number(pipeline, "tracker.v_r"), 3),
                            number_text(path_number(pipeline, "tracker.v_xy"), 3)
                        ),
                    );
                    debug_kv(
                        ui,
                        "last velocity",
                        &vec3_text(path_value(pipeline, "tracker.last_velocity")),
                    );
                    debug_kv(
                        ui,
                        "state",
                        &array_text(path_value(pipeline, "tracker.target_state"), 9),
                    );
                });
                columns[1].group(|ui| {
                    ui.heading("Calibration");
                    debug_kv(
                        ui,
                        "extrinsic",
                        &format!(
                            "enabled={} config={}",
                            bool_text(path_bool(pipeline, "calibration.extrinsic_enabled")),
                            bool_text(path_bool(pipeline, "calibration.extrinsic_from_config"))
                        ),
                    );
                    debug_kv(
                        ui,
                        "t camera->gimbal",
                        &vec3_text(path_value(pipeline, "calibration.t_camera2gimbal_m")),
                    );
                    debug_kv(
                        ui,
                        "legacy H",
                        &meters_text(path_number(pipeline, "calibration.legacy_h_m")),
                    );
                });
            });
            if let Some(error) = &state.last_error {
                ui.separator();
                ui.label(error);
            }
        });
}

fn debug_kv(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(key);
        ui.monospace(value);
    });
}

fn draw_debug_charts(ui: &mut egui::Ui, history: &VecDeque<DebugHistorySample>) {
    if history.len() < 2 {
        ui.label("waiting for chart samples");
        return;
    }

    ui.columns(2, |columns| {
        draw_chart(
            &mut columns[0],
            "image error deg",
            history,
            &[
                ChartSeries {
                    label: "yaw",
                    color: egui::Color32::from_rgb(80, 170, 255),
                    value: sample_image_yaw,
                },
                ChartSeries {
                    label: "pitch",
                    color: egui::Color32::from_rgb(255, 190, 80),
                    value: sample_image_pitch,
                },
            ],
        );
        draw_chart(
            &mut columns[1],
            "command deg",
            history,
            &[
                ChartSeries {
                    label: "yaw",
                    color: egui::Color32::from_rgb(120, 220, 150),
                    value: sample_command_yaw,
                },
                ChartSeries {
                    label: "pitch",
                    color: egui::Color32::from_rgb(255, 120, 140),
                    value: sample_command_pitch,
                },
            ],
        );
    });
    ui.columns(2, |columns| {
        draw_chart(
            &mut columns[0],
            "distance / confidence",
            history,
            &[
                ChartSeries {
                    label: "distance m",
                    color: egui::Color32::from_rgb(130, 180, 255),
                    value: sample_distance,
                },
                ChartSeries {
                    label: "confidence",
                    color: egui::Color32::from_rgb(120, 230, 170),
                    value: sample_confidence,
                },
            ],
        );
        draw_chart(
            &mut columns[1],
            "center / latency",
            history,
            &[
                ChartSeries {
                    label: "center px",
                    color: egui::Color32::from_rgb(255, 210, 90),
                    value: sample_center_error,
                },
                ChartSeries {
                    label: "lag ms",
                    color: egui::Color32::from_rgb(180, 150, 255),
                    value: sample_runtime_lag,
                },
                ChartSeries {
                    label: "fire",
                    color: egui::Color32::from_rgb(255, 100, 100),
                    value: sample_fire_advice,
                },
            ],
        );
    });
}

fn draw_chart(
    ui: &mut egui::Ui,
    title: &str,
    history: &VecDeque<DebugHistorySample>,
    series: &[ChartSeries],
) {
    ui.label(egui::RichText::new(title).strong());
    let desired_size = egui::vec2(ui.available_width().max(260.0), 132.0);
    let (rect, _) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(18, 22, 26));
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(70, 76, 82)),
        egui::StrokeKind::Inside,
    );

    let Some(first) = history.front() else {
        return;
    };
    let Some(last) = history.back() else {
        return;
    };
    let x_min = first.t_s;
    let mut x_span = last.t_s - first.t_s;
    if x_span <= 1e-6 {
        x_span = 1.0;
    }

    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    for sample in history {
        for item in series {
            let value = (item.value)(sample);
            if value.is_finite() {
                y_min = y_min.min(value);
                y_max = y_max.max(value);
            }
        }
    }
    if !y_min.is_finite() || !y_max.is_finite() {
        return;
    }
    if (y_max - y_min).abs() < 1e-6 {
        y_min -= 1.0;
        y_max += 1.0;
    } else {
        let pad = (y_max - y_min) * 0.12;
        y_min -= pad;
        y_max += pad;
    }

    let to_pos = |sample: &DebugHistorySample, value: f64| -> egui::Pos2 {
        let x = ((sample.t_s - x_min) / x_span).clamp(0.0, 1.0) as f32;
        let y = ((value - y_min) / (y_max - y_min)).clamp(0.0, 1.0) as f32;
        egui::pos2(
            egui::lerp(rect.left()..=rect.right(), x),
            egui::lerp(rect.bottom()..=rect.top(), y),
        )
    };

    for item in series {
        let mut previous: Option<egui::Pos2> = None;
        for sample in history {
            let value = (item.value)(sample);
            if !value.is_finite() {
                previous = None;
                continue;
            }
            let point = to_pos(sample, value);
            if let Some(prev) = previous {
                painter.line_segment([prev, point], egui::Stroke::new(1.6, item.color));
            }
            previous = Some(point);
        }
    }

    let latest = history.back();
    ui.horizontal_wrapped(|ui| {
        for item in series {
            let value = latest
                .map(|sample| (item.value)(sample))
                .unwrap_or(f64::NAN);
            ui.colored_label(
                item.color,
                format!("{} {}", item.label, number_text(Some(value), 2)),
            );
        }
    });
}

fn sample_image_yaw(sample: &DebugHistorySample) -> f64 {
    sample.image_yaw_deg
}

fn sample_image_pitch(sample: &DebugHistorySample) -> f64 {
    sample.image_pitch_deg
}

fn sample_command_yaw(sample: &DebugHistorySample) -> f64 {
    sample.command_yaw_deg
}

fn sample_command_pitch(sample: &DebugHistorySample) -> f64 {
    sample.command_pitch_deg
}

fn sample_distance(sample: &DebugHistorySample) -> f64 {
    sample.distance_m
}

fn sample_confidence(sample: &DebugHistorySample) -> f64 {
    sample.confidence
}

fn sample_center_error(sample: &DebugHistorySample) -> f64 {
    sample.center_error_px
}

fn sample_runtime_lag(sample: &DebugHistorySample) -> f64 {
    sample.runtime_lag_ms
}

fn sample_fire_advice(sample: &DebugHistorySample) -> f64 {
    sample.fire_advice
}

fn mode_button(ui: &mut egui::Ui, requested: &mut AutoAimSceneMode, mode: AutoAimSceneMode) {
    if ui
        .add_sized(
            [138.0, 28.0],
            egui::SelectableLabel::new(*requested == mode, mode.label()),
        )
        .clicked()
    {
        *requested = mode;
    }
}

pub fn shooting_range_control_window(
    mut contexts: EguiContexts,
    mut state: ResMut<ShootingRangeControlState>,
    targets: Query<(&ShootingRangeTarget, Option<&ActiveSlapper>)>,
) {
    if !state.open {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let mut armor_3_present = false;
    let mut armor_1_present = false;
    let mut armor_3_active = false;
    let mut armor_1_active = false;
    for (target, active) in &targets {
        match target.kind {
            ShootingRangeTargetKind::Armor3 => {
                armor_3_present = true;
                armor_3_active = active.is_some();
            }
            ShootingRangeTargetKind::Armor1 => {
                armor_1_present = true;
                armor_1_active = active.is_some();
            }
        }
    }

    let mut open = state.open;
    egui::Window::new("Range Target Control")
        .open(&mut open)
        .resizable(false)
        .default_width(360.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("targets");
                ui.monospace(format!(
                    "#3 {}{}  #1 {}{}",
                    if armor_3_present { "online" } else { "missing" },
                    if armor_3_active { " active" } else { "" },
                    if armor_1_present { "online" } else { "missing" },
                    if armor_1_active { " active" } else { "" },
                ));
            });
            ui.separator();

            target_motion_controls(
                ui,
                ShootingRangeTargetKind::Armor3.label(),
                &mut state.armor_3,
            );
            ui.separator();
            target_motion_controls(
                ui,
                ShootingRangeTargetKind::Armor1.label(),
                &mut state.armor_1,
            );

            ui.horizontal(|ui| {
                if ui.button("stop all").clicked() {
                    stop_motion(&mut state.armor_3);
                    stop_motion(&mut state.armor_1);
                }
                if ui.button("close").clicked() {
                    state.open = false;
                }
            });
        });

    state.open = open && state.open;
}

fn target_motion_controls(
    ui: &mut egui::Ui,
    label: &str,
    settings: &mut RangeTargetMotionSettings,
) {
    ui.label(egui::RichText::new(label).strong());
    ui.horizontal(|ui| {
        ui.selectable_value(
            &mut settings.mode,
            RangeTargetMotionMode::Stationary,
            "stop",
        );
        ui.selectable_value(&mut settings.mode, RangeTargetMotionMode::Linear, "line");
        ui.selectable_value(&mut settings.mode, RangeTargetMotionMode::Spin, "spin");
        ui.selectable_value(
            &mut settings.mode,
            RangeTargetMotionMode::LinearAndSpin,
            "line+spin",
        );
    });
    ui.add(egui::Slider::new(&mut settings.direction_deg, -180.0..=180.0).text("direction deg"));
    ui.add(egui::Slider::new(&mut settings.linear_speed_mps, 0.0..=6.0).text("linear m/s"));
    ui.add(
        egui::Slider::new(
            &mut settings.linear_span_m,
            0.0..=RANGE_TARGET_MAX_LINEAR_SPAN_M,
        )
        .text("span m"),
    );
    ui.add(egui::Slider::new(&mut settings.spin_deg_s, -720.0..=720.0).text("spin deg/s"));
    ui.horizontal(|ui| {
        ui.label("travel");
        ui.monospace(format!(
            "{} / {:.1} m",
            if settings.travel_sign >= 0.0 {
                "+"
            } else {
                "-"
            },
            settings
                .linear_span_m
                .clamp(0.0, RANGE_TARGET_MAX_LINEAR_SPAN_M)
        ));
        if ui.button("stop").clicked() {
            stop_motion(settings);
        }
    });
}

fn stop_motion(settings: &mut RangeTargetMotionSettings) {
    settings.mode = RangeTargetMotionMode::Stationary;
    settings.linear_speed_mps = 0.0;
    settings.spin_deg_s = 0.0;
}

pub fn shooting_range_debug_panel(
    mut contexts: EguiContexts,
    time: Res<Time>,
    auto_aim: Res<SubscribeAutoAim>,
    bridge_status: Option<Res<IntegratedAutoAimBridge>>,
    stats: Res<ProjectileStatistics>,
    targets: Query<(), With<ShootingRangeTarget>>,
    mut state: ResMut<ShootingRangeDebugPanelState>,
) {
    if targets.iter().next().is_none() {
        return;
    }

    refresh_debug_telemetry(&time, &mut state);

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let bridge = state.bridge.as_ref();
    let pipeline = state.pipeline.as_ref();
    let bridge_status = bridge_status
        .as_deref()
        .map(IntegratedAutoAimBridge::status_label)
        .unwrap_or("N/A");
    let auto_aim_on = auto_aim.load(std::sync::atomic::Ordering::Acquire);

    egui::Window::new("Auto-Aim Debug")
        .resizable(true)
        .collapsible(true)
        .default_width(430.0)
        .default_pos(egui::pos2(12.0, 72.0))
        .show(ctx, |ui| {
            egui::Grid::new("range_auto_aim_debug_grid")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    debug_row(ui, "scene", "shooting_range");
                    debug_row(ui, "auto aim", if auto_aim_on { "ON" } else { "OFF" });
                    debug_row(ui, "bridge", bridge_status);
                    debug_row(
                        ui,
                        "stats",
                        &format!(
                            "{} / {} ({:.1}%)",
                            stats.accurate_count,
                            stats.launch_count,
                            stats.accurate_pct() * 100.0
                        ),
                    );
                    debug_row(
                        ui,
                        "telemetry",
                        if bridge.is_some() || pipeline.is_some() {
                            "live"
                        } else {
                            "waiting"
                        },
                    );

                    ui.end_row();
                    debug_section(ui, "gimbal");
                    debug_row(
                        ui,
                        "runtime yaw/pitch",
                        &format!(
                            "{} / {}",
                            deg(path_number(bridge, "gimbal.runtime_local_yaw_deg")),
                            deg(path_number(bridge, "gimbal.runtime_pitch_deg"))
                        ),
                    );
                    debug_row(
                        ui,
                        "vivsionn yaw/pitch",
                        &format!(
                            "{} / {}",
                            deg(path_number(bridge, "gimbal.vivsionn_yaw_deg")),
                            deg(path_number(pipeline, "input_gimbal_pitch_deg"))
                        ),
                    );

                    ui.end_row();
                    debug_section(ui, "target");
                    debug_row(
                        ui,
                        "det / solved",
                        &format!(
                            "{} / {}",
                            int_text(path_i64(pipeline, "detector.count")),
                            int_text(path_i64(pipeline, "solved_count"))
                        ),
                    );
                    debug_row(
                        ui,
                        "center",
                        &point_text(path_value(pipeline, "first_solved.center_px")),
                    );
                    debug_row(
                        ui,
                        "image yaw/pitch",
                        &format!(
                            "{} / {}",
                            deg(path_number(pipeline, "first_solved.image_yaw_deg")),
                            deg(path_number(
                                pipeline,
                                "first_solved.image_pitch_down_positive_deg"
                            ))
                        ),
                    );
                    debug_row(
                        ui,
                        "pos yaw/pitch",
                        &format!(
                            "{} / {}",
                            deg(path_number(pipeline, "first_solved.position_yaw_deg")),
                            deg(path_number(pipeline, "first_solved.position_pitch_deg"))
                        ),
                    );
                    debug_row(
                        ui,
                        "distance",
                        &mm_text(path_number(pipeline, "first_solved.distance_mm")),
                    );
                    debug_row(
                        ui,
                        "camera",
                        &format!(
                            "{} / {}mm",
                            text(path_value(pipeline, "camera.profile_id")),
                            number_text(path_number(pipeline, "camera.focal_mm"), 1)
                        ),
                    );
                    debug_row(
                        ui,
                        "dual focal",
                        &format!(
                            "{} -> {} margin {}px",
                            text(path_value(pipeline, "dual_focal.reason")),
                            text(path_value(pipeline, "dual_focal.next_selected")),
                            number_text(
                                path_number(pipeline, "dual_focal.precision_min_margin_px"),
                                1
                            )
                        ),
                    );
                    debug_row(
                        ui,
                        "subpixel delta",
                        &format!(
                            "{} / {} px",
                            number_text(
                                path_number(pipeline, "subpixel_refinement.mean_delta_px"),
                                2
                            ),
                            number_text(
                                path_number(pipeline, "subpixel_refinement.max_delta_px"),
                                2
                            )
                        ),
                    );

                    ui.end_row();
                    debug_section(ui, "filter");
                    debug_row(
                        ui,
                        "tracker",
                        &text(path_value(pipeline, "tracker.tracker_state")),
                    );
                    debug_row(
                        ui,
                        "movement",
                        &text(path_value(pipeline, "tracker.movement")),
                    );
                    debug_row(
                        ui,
                        "v_t / v_r / v_xy",
                        &format!(
                            "{} / {} / {}",
                            number_text(path_number(pipeline, "tracker.v_t"), 3),
                            number_text(path_number(pipeline, "tracker.v_r"), 3),
                            number_text(path_number(pipeline, "tracker.v_xy"), 3)
                        ),
                    );
                    debug_row(
                        ui,
                        "last velocity",
                        &vec3_text(path_value(pipeline, "tracker.last_velocity")),
                    );
                    debug_row(
                        ui,
                        "state",
                        &array_text(path_value(pipeline, "tracker.target_state"), 9),
                    );

                    ui.end_row();
                    debug_section(ui, "fire control");
                    debug_row(
                        ui,
                        "cmd yaw",
                        &format!(
                            "{} -> {}",
                            deg(path_number(pipeline, "fire_control.raw_command_yaw_deg")),
                            deg(path_number(
                                pipeline,
                                "fire_control.filtered_command_yaw_deg"
                            ))
                        ),
                    );
                    debug_row(
                        ui,
                        "cmd pitch",
                        &format!(
                            "{} -> {}",
                            deg(path_number(pipeline, "fire_control.raw_command_pitch_deg")),
                            deg(path_number(
                                pipeline,
                                "fire_control.filtered_command_pitch_deg"
                            ))
                        ),
                    );
                    debug_row(
                        ui,
                        "yaw speed",
                        &format!(
                            "{} deg/s",
                            number_text(path_number(pipeline, "fire_control.yaw_speed_deg_s"), 2)
                        ),
                    );
                    debug_row(
                        ui,
                        "shot / fire",
                        &format!(
                            "{} / {}",
                            int_text(path_i64(pipeline, "fire_control.shot_mode")),
                            bool_text(path_bool(pipeline, "aim_command.fire_advice"))
                        ),
                    );
                    debug_row(
                        ui,
                        "gates",
                        &format!(
                            "valid={} mcu={} stable={} follow={} preview={}",
                            bool_text(path_bool(pipeline, "fire_control.fire_gate_valid")),
                            bool_text(path_bool(pipeline, "fire_control.fire_gate_mcu_permit")),
                            bool_text(path_bool(pipeline, "fire_control.fire_gate_command_stable")),
                            bool_text(path_bool(pipeline, "fire_control.fire_gate_follow")),
                            bool_text(path_bool(pipeline, "fire_control.fire_gate_preview"))
                        ),
                    );

                    ui.end_row();
                    debug_section(ui, "calibration");
                    debug_row(
                        ui,
                        "extrinsic",
                        &format!(
                            "enabled={} config={}",
                            bool_text(path_bool(pipeline, "calibration.extrinsic_enabled")),
                            bool_text(path_bool(pipeline, "calibration.extrinsic_from_config"))
                        ),
                    );
                    debug_row(
                        ui,
                        "t camera->gimbal",
                        &vec3_text(path_value(pipeline, "calibration.t_camera2gimbal_m")),
                    );
                    debug_row(
                        ui,
                        "legacy H",
                        &meters_text(path_number(pipeline, "calibration.legacy_h_m")),
                    );
                });

            ui.separator();
            draw_debug_charts(ui, &state.history);

            if let Some(error) = &state.last_error {
                ui.separator();
                ui.label(error);
            }
        });
}

fn wrap_angle_rad(angle: f32) -> f32 {
    angle.sin().atan2(angle.cos())
}

fn truth_gimbal_camera_pose(
    player: &Transform,
    launch: &Transform,
    yaw_rad: f32,
    pitch_rad: f32,
) -> Transform {
    let gimbal_local = Quat::from_euler(EulerRot::YXZ, yaw_rad, pitch_rad, 0.0);
    let gimbal_world = player.rotation * gimbal_local;
    let camera_local = launch.translation
        + launch.rotation.mul_vec3(Vec3::Y)
            * crate::systems::camera::ROBOT_CAMERA_FORWARD_CLEARANCE;
    Transform {
        translation: player.translation + gimbal_world * camera_local,
        rotation: gimbal_world
            * launch.rotation
            * Quat::from_euler(EulerRot::ZYX, 0.0, 0.0, std::f32::consts::FRAC_PI_2),
        ..default()
    }
}

fn optical_axis_error(camera: &Transform, target_world: Vec3) -> Vec2 {
    let camera_direction = camera.rotation.inverse() * (target_world - camera.translation);
    let horizontal = camera_direction.x.atan2(-camera_direction.z);
    let pitch = camera_direction.y.atan2(
        camera_direction
            .x
            .hypot(camera_direction.z)
            .max(f32::MIN_POSITIVE),
    );
    Vec2::new(horizontal, pitch)
}

fn solve_truth_gimbal_from_seed(
    player: &Transform,
    launch: &Transform,
    target_world: Vec3,
    initial_yaw_rad: f32,
    initial_pitch_rad: f32,
) -> (f32, f32, Transform, f32) {
    let mut yaw = initial_yaw_rad;
    let mut pitch = initial_pitch_rad;

    for _ in 0..TRUTH_GIMBAL_MAX_SOLVE_STEPS {
        let camera = truth_gimbal_camera_pose(player, launch, yaw, pitch);
        let error = optical_axis_error(&camera, target_world);
        if error.length() <= TRUTH_GIMBAL_SOLVE_EPSILON_RAD {
            break;
        }

        let yaw_error = optical_axis_error(
            &truth_gimbal_camera_pose(player, launch, yaw + TRUTH_GIMBAL_NUMERIC_STEP_RAD, pitch),
            target_world,
        );
        let pitch_error = optical_axis_error(
            &truth_gimbal_camera_pose(player, launch, yaw, pitch + TRUTH_GIMBAL_NUMERIC_STEP_RAD),
            target_world,
        );
        let yaw_column = (yaw_error - error) / TRUTH_GIMBAL_NUMERIC_STEP_RAD;
        let pitch_column = (pitch_error - error) / TRUTH_GIMBAL_NUMERIC_STEP_RAD;
        let determinant = yaw_column.x * pitch_column.y - pitch_column.x * yaw_column.y;
        if !determinant.is_finite() || determinant.abs() < 1e-6 {
            yaw += error.x.clamp(
                -TRUTH_GIMBAL_MAX_NEWTON_STEP_RAD,
                TRUTH_GIMBAL_MAX_NEWTON_STEP_RAD,
            );
            pitch += error.y.clamp(
                -TRUTH_GIMBAL_MAX_NEWTON_STEP_RAD,
                TRUTH_GIMBAL_MAX_NEWTON_STEP_RAD,
            );
        } else {
            let delta_yaw = (-error.x * pitch_column.y + pitch_column.x * error.y) / determinant;
            let delta_pitch = (-yaw_column.x * error.y + error.x * yaw_column.y) / determinant;
            yaw += delta_yaw.clamp(
                -TRUTH_GIMBAL_MAX_NEWTON_STEP_RAD,
                TRUTH_GIMBAL_MAX_NEWTON_STEP_RAD,
            );
            pitch += delta_pitch.clamp(
                -TRUTH_GIMBAL_MAX_NEWTON_STEP_RAD,
                TRUTH_GIMBAL_MAX_NEWTON_STEP_RAD,
            );
        }
        yaw = wrap_angle_rad(yaw);
        pitch = pitch.clamp(
            -std::f32::consts::FRAC_PI_2 + 1e-4,
            std::f32::consts::FRAC_PI_2 - 1e-4,
        );
    }

    let camera = truth_gimbal_camera_pose(player, launch, yaw, pitch);
    let error_rad = optical_axis_error(&camera, target_world).length();
    (yaw, pitch, camera, error_rad)
}

fn solve_truth_gimbal_command(
    player: &Transform,
    launch: &Transform,
    target_world: Vec3,
    initial_yaw_rad: f32,
    initial_pitch_rad: f32,
) -> (f32, f32, Transform, f32) {
    let seeds = [
        initial_yaw_rad,
        initial_yaw_rad + std::f32::consts::PI,
        initial_yaw_rad + std::f32::consts::FRAC_PI_2,
        initial_yaw_rad - std::f32::consts::FRAC_PI_2,
    ];
    let mut best =
        solve_truth_gimbal_from_seed(player, launch, target_world, seeds[0], initial_pitch_rad);
    for yaw_seed in seeds.into_iter().skip(1) {
        let candidate =
            solve_truth_gimbal_from_seed(player, launch, target_world, yaw_seed, initial_pitch_rad);
        if candidate.3 < best.3 {
            best = candidate;
        }
    }
    best
}

pub fn apply_shooting_range_truth_gimbal(
    exposure_stamp: Res<ExposureWallTimestamp>,
    scene_state: Res<AutoAimSceneState>,
    config: Res<ShootingRangeTruthGimbalConfig>,
    mut stats: ResMut<ShootingRangeTruthGimbalStats>,
    targets: Query<(&ShootingRangeTarget, &Transform), (Without<Controlled>, Without<MainCamera>)>,
    players: Query<&Transform, (With<Infantry>, With<Controlled>, Without<InfantryGimbal>)>,
    mut gimbals: Query<
        (&mut Transform, &mut InfantryGimbal),
        (With<Controlled>, With<InfantryGimbal>, Without<Infantry>),
    >,
    launches: Query<
        &Transform,
        (
            With<Controlled>,
            With<InfantryLaunchOffset>,
            Without<InfantryGimbal>,
        ),
    >,
    mut cameras: Query<&mut Transform, (With<MainCamera>, Without<Controlled>)>,
) {
    let Some(target_number) = config.target_number else {
        return;
    };
    if scene_state.current != AutoAimSceneMode::ShootingRange {
        return;
    }

    let Some((_, target_transform)) = targets
        .iter()
        .find(|(target, _)| target.kind.number() == target_number)
    else {
        return;
    };
    let (Ok(player), Ok((mut gimbal_transform, mut gimbal_data)), Ok(launch), Ok(mut camera)) = (
        players.single(),
        gimbals.single_mut(),
        launches.single(),
        cameras.single_mut(),
    ) else {
        return;
    };

    let (yaw, pitch, camera_pose, error_rad) = solve_truth_gimbal_command(
        player,
        launch,
        target_transform.translation,
        gimbal_data.local_yaw,
        gimbal_data.pitch,
    );
    gimbal_transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
    gimbal_data.local_yaw = yaw;
    gimbal_data.pitch = pitch;
    *camera = camera_pose;

    stats.enabled = true;
    stats.target_number = Some(target_number);
    stats.command_count = stats.command_count.saturating_add(1);
    stats.last_exposure_timestamp_ns = exposure_stamp.timestamp_ns;
    stats.last_command_yaw_rad = yaw;
    stats.last_command_pitch_rad = pitch;
    stats.last_optical_axis_error_rad = error_rad;
    if stats.command_count == 1 || stats.command_count % 250 == 0 {
        info!(
            "research_truth_gimbal enabled=1 target_number={} command_count={} exposure_timestamp_ns={} command_yaw_rad={:.9} command_pitch_rad={:.9} optical_axis_error_rad={:.9}",
            target_number,
            stats.command_count,
            stats.last_exposure_timestamp_ns,
            yaw,
            pitch,
            error_rad,
        );
    }
}

pub fn apply_shooting_range_target_motion(
    exposure_stamp: Res<ExposureWallTimestamp>,
    mut state: ResMut<ShootingRangeControlState>,
    mut wall_poses: Local<HashMap<Entity, RangeTargetWallPose>>,
    mut targets: Query<(
        Entity,
        &ShootingRangeTarget,
        &mut Transform,
        &mut LinearVelocity,
        &mut AngularVelocity,
    )>,
) {
    let now_s = exposure_stamp.timestamp_ns as f64 * 1e-9;
    for (entity, target, mut transform, mut linear_velocity, mut angular_velocity) in &mut targets {
        let settings = match target.kind {
            ShootingRangeTargetKind::Armor3 => &mut state.armor_3,
            ShootingRangeTargetKind::Armor1 => &mut state.armor_1,
        };
        let wall_pose = wall_poses.entry(entity).or_insert(RangeTargetWallPose {
            translation: transform.translation,
            rotation: transform.rotation,
            last_real_elapsed_s: now_s,
        });
        let dt_s = (now_s - wall_pose.last_real_elapsed_s).max(0.0) as f32;
        wall_pose.last_real_elapsed_s = now_s;

        let (commanded_linear_velocity, commanded_angular_velocity) =
            advance_range_target_wall_pose(wall_pose, settings, target.origin, dt_s);

        // FixedUpdate can execute many catch-up ticks after startup or a long stall.
        // The shooting-range contract is expressed in exposure/wall time, so expose
        // this independent wall-clock pose rather than Avian's intermediate catch-up
        // pose. The velocity components remain the declared ground-truth derivatives.
        transform.translation = wall_pose.translation;
        transform.rotation = wall_pose.rotation;
        linear_velocity.0 = commanded_linear_velocity;
        angular_velocity.0 = commanded_angular_velocity;
    }

    wall_poses.retain(|entity, _| targets.contains(*entity));
}

/// Baseline position of one armor root in its shooting-range target's local
/// frame.  Keeping this value makes repeated geometry commands absolute
/// relative to stock geometry instead of compounding the previous scale.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct RangeTargetArmorBaseline {
    target_local: Vec3,
}

/// Apply a radial scale to the four armor centers without scaling the vehicle
/// body, armor dimensions, or the target's rigid-body motion.  The system runs
/// after transform propagation so the first frame after an ACK contains the
/// updated hierarchy and the ground-truth collector observes the same geometry
/// as rendering and collision.
pub fn apply_shooting_range_target_geometry(
    state: Res<ShootingRangeControlState>,
    children: Query<&Children>,
    mut targets: Query<(Entity, &mut ShootingRangeTarget, &GlobalTransform)>,
    armor_data: Query<
        (
            Entity,
            &GlobalTransform,
            &ChildOf,
            Option<&RangeTargetArmorBaseline>,
        ),
        With<ArmorRoot>,
    >,
    parent_globals: Query<&GlobalTransform>,
    mut armor_transforms: Query<&mut Transform, With<ArmorRoot>>,
    mut commands: Commands,
) {
    for (target_entity, mut target, target_global) in &mut targets {
        let radial_scale = match target.kind {
            ShootingRangeTargetKind::Armor3 => state.armor_3.radial_scale,
            ShootingRangeTargetKind::Armor1 => state.armor_1.radial_scale,
        };
        if target.applied_geometry_scale.is_finite()
            && (target.applied_geometry_scale - radial_scale).abs() <= 1e-6
        {
            continue;
        }

        let target_inverse = target_global.affine().inverse();
        let mut plans = Vec::with_capacity(4);
        for armor_entity in children.iter_descendants(target_entity) {
            let Ok((_, armor_global, parent, baseline)) = armor_data.get(armor_entity) else {
                continue;
            };
            let baseline_local = baseline
                .map(|baseline| baseline.target_local)
                .unwrap_or_else(|| target_inverse.transform_point3(armor_global.translation()));
            let desired_local = Vec3::new(
                baseline_local.x * radial_scale,
                baseline_local.y,
                baseline_local.z * radial_scale,
            );
            let desired_world = target_global.affine().transform_point3(desired_local);
            let Ok(parent_global) = parent_globals.get(parent.parent()) else {
                plans.clear();
                break;
            };
            let local_translation = parent_global
                .affine()
                .inverse()
                .transform_point3(desired_world);
            plans.push((
                armor_entity,
                local_translation,
                baseline.is_none(),
                baseline_local,
            ));
        }

        // Target #3 has four armor roots.  Refuse partial application so a
        // scene-loading race cannot leave truth, collision, and rendering out
        // of sync.  The target remains pending and will be retried next frame.
        if plans.len() != 4 {
            continue;
        }

        for (armor_entity, local_translation, needs_baseline, baseline_local) in plans {
            if needs_baseline {
                commands
                    .entity(armor_entity)
                    .insert(RangeTargetArmorBaseline {
                        target_local: baseline_local,
                    });
            }
            if let Ok(mut transform) = armor_transforms.get_mut(armor_entity) {
                transform.translation = local_translation;
            }
        }
        target.applied_geometry_scale = radial_scale;
    }
}

fn advance_range_target_wall_pose(
    wall_pose: &mut RangeTargetWallPose,
    settings: &mut RangeTargetMotionSettings,
    origin: Vec3,
    dt_s: f32,
) -> (Vec3, Vec3) {
    let angular_velocity = target_angular_velocity(settings);
    if dt_s.is_finite() && dt_s > 0.0 && angular_velocity != Vec3::ZERO {
        wall_pose.rotation = Quat::from_rotation_y(angular_velocity.y * dt_s) * wall_pose.rotation;
    }

    let linear_velocity = match settings.mode {
        RangeTargetMotionMode::Linear | RangeTargetMotionMode::LinearAndSpin => {
            advance_bounded_wall_translation(wall_pose, settings, origin, dt_s)
        }
        RangeTargetMotionMode::Stationary | RangeTargetMotionMode::Spin => Vec3::ZERO,
    };

    (linear_velocity, angular_velocity)
}

fn advance_bounded_wall_translation(
    wall_pose: &mut RangeTargetWallPose,
    settings: &mut RangeTargetMotionSettings,
    origin: Vec3,
    dt_s: f32,
) -> Vec3 {
    let axis = target_linear_axis(settings);
    let speed = settings.linear_speed_mps.max(0.0);
    let half_extent = settings
        .linear_span_m
        .clamp(0.0, RANGE_TARGET_MAX_LINEAR_SPAN_M)
        * 0.5;
    if axis == Vec3::ZERO || speed <= 0.0 || half_extent <= RANGE_TARGET_BOUNDARY_EPSILON_M {
        return Vec3::ZERO;
    }

    let span = half_extent * 2.0;
    let offset = (wall_pose.translation - origin)
        .dot(axis)
        .clamp(-half_extent, half_extent);
    let unfolded = if normalized_travel_sign(settings) > 0.0 {
        offset + half_extent
    } else {
        span * 2.0 - (offset + half_extent)
    };
    let phase = (unfolded + speed * dt_s.max(0.0)).rem_euclid(span * 2.0);
    let (distance_from_low, sign) = if phase < span {
        (phase, 1.0)
    } else {
        (span * 2.0 - phase, -1.0)
    };
    settings.travel_sign = sign;
    let new_offset = distance_from_low - half_extent;
    wall_pose.translation = origin + axis * new_offset;
    axis * speed * sign
}

fn resolve_debug_json_path(
    explicit_path: Option<OsString>,
    workspace_dir: Option<OsString>,
    current_dir: Option<&Path>,
    file_name: &str,
) -> PathBuf {
    if let Some(path) = explicit_path.filter(|value| !value.is_empty()) {
        return PathBuf::from(path);
    }

    if let Some(workspace) = workspace_dir.filter(|value| !value.is_empty()) {
        return PathBuf::from(workspace)
            .join("aim_sim_bridge")
            .join("build")
            .join("debug")
            .join(file_name);
    }

    if let Some(current_dir) = current_dir {
        for ancestor in current_dir.ancestors() {
            let candidate = ancestor
                .join("aim_sim_bridge")
                .join("build")
                .join("debug")
                .join(file_name);
            if candidate.parent().is_some_and(Path::exists) {
                return candidate;
            }
        }
    }

    if let Some(workspace) = default_workspace_root_from_manifest() {
        let candidate = workspace
            .join("aim_sim_bridge")
            .join("build")
            .join("debug")
            .join(file_name);
        if candidate.parent().is_some_and(Path::exists) {
            return candidate;
        }
    }

    PathBuf::from("..")
        .join("..")
        .join("aim_sim_bridge")
        .join("build")
        .join("debug")
        .join(file_name)
}

fn default_workspace_root_from_manifest() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest_dir.parent()?.parent()?.to_path_buf();
    Some(workspace).filter(|path| path.join("aim_sim_bridge").is_dir())
}

fn read_json_file(path: &Path, last_error: &mut Option<String>) -> Option<Value> {
    match std::fs::read_to_string(path) {
        Ok(contents) => match serde_json::from_str::<Value>(&contents) {
            Ok(value) => {
                *last_error = None;
                Some(value)
            }
            Err(error) => {
                *last_error = Some(format!("telemetry parse error: {error}"));
                None
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            *last_error = Some(format!("telemetry read error: {error}"));
            None
        }
    }
}

fn path_value<'a>(root: Option<&'a Value>, path: &str) -> Option<&'a Value> {
    let mut current = root?;
    for key in path.split('.') {
        current = current.get(key)?;
    }
    Some(current)
}

fn path_number(root: Option<&Value>, path: &str) -> Option<f64> {
    path_value(root, path).and_then(Value::as_f64)
}

fn path_i64(root: Option<&Value>, path: &str) -> Option<i64> {
    path_value(root, path).and_then(Value::as_i64)
}

fn path_bool(root: Option<&Value>, path: &str) -> Option<bool> {
    path_value(root, path).and_then(Value::as_bool)
}

fn debug_section(ui: &mut egui::Ui, label: &str) {
    ui.label(egui::RichText::new(label).strong());
    ui.end_row();
}

fn debug_row(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.label(key);
    ui.monospace(value);
    ui.end_row();
}

fn number_text(value: Option<f64>, digits: usize) -> String {
    value
        .filter(|value| value.is_finite())
        .map(|value| format!("{value:.digits$}"))
        .unwrap_or_else(|| "-".to_string())
}

fn int_text(value: Option<i64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn bool_text(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "-",
    }
}

fn text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        _ => "-".to_string(),
    }
}

fn deg(value: Option<f64>) -> String {
    value
        .filter(|value| value.is_finite())
        .map(|value| format!("{value:.2} deg"))
        .unwrap_or_else(|| "-".to_string())
}

fn meters_text(value: Option<f64>) -> String {
    value
        .filter(|value| value.is_finite())
        .map(|value| format!("{value:.3} m"))
        .unwrap_or_else(|| "-".to_string())
}

fn mm_text(value: Option<f64>) -> String {
    value
        .filter(|value| value.is_finite())
        .map(|value| format!("{value:.1} mm"))
        .unwrap_or_else(|| "-".to_string())
}

fn point_text(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return "-".to_string();
    };
    let x = value.get("x").and_then(Value::as_f64);
    let y = value.get("y").and_then(Value::as_f64);
    match (x, y) {
        (Some(x), Some(y)) => format!("({x:.1}, {y:.1})"),
        _ => "-".to_string(),
    }
}

fn vec3_text(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return "-".to_string();
    };
    let x = value.get("x").and_then(Value::as_f64);
    let y = value.get("y").and_then(Value::as_f64);
    let z = value.get("z").and_then(Value::as_f64);
    match (x, y, z) {
        (Some(x), Some(y), Some(z)) => format!("({x:.3}, {y:.3}, {z:.3})"),
        _ => "-".to_string(),
    }
}

fn array_text(value: Option<&Value>, max_items: usize) -> String {
    let Some(Value::Array(items)) = value else {
        return "-".to_string();
    };
    let mut parts: Vec<String> = items
        .iter()
        .take(max_items)
        .map(|item| {
            item.as_f64()
                .map(|value| format!("{value:.3}"))
                .unwrap_or_else(|| "-".to_string())
        })
        .collect();
    if items.len() > max_items {
        parts.push("...".to_string());
    }
    format!("[{}]", parts.join(", "))
}

fn target_linear_velocity(settings: &RangeTargetMotionSettings) -> Vec3 {
    match settings.mode {
        RangeTargetMotionMode::Linear | RangeTargetMotionMode::LinearAndSpin => {
            target_linear_axis(settings)
                * settings.linear_speed_mps
                * normalized_travel_sign(settings)
        }
        RangeTargetMotionMode::Stationary | RangeTargetMotionMode::Spin => Vec3::ZERO,
    }
}

fn bounded_target_linear_velocity(
    settings: &mut RangeTargetMotionSettings,
    origin: Vec3,
    transform: &mut Transform,
) -> Vec3 {
    match settings.mode {
        RangeTargetMotionMode::Linear | RangeTargetMotionMode::LinearAndSpin => {}
        RangeTargetMotionMode::Stationary | RangeTargetMotionMode::Spin => {
            return Vec3::ZERO;
        }
    }

    let axis = target_linear_axis(settings);
    if axis == Vec3::ZERO || settings.linear_speed_mps <= 0.0 {
        return Vec3::ZERO;
    }

    let half_extent = settings
        .linear_span_m
        .clamp(0.0, RANGE_TARGET_MAX_LINEAR_SPAN_M)
        * 0.5;
    if half_extent <= RANGE_TARGET_BOUNDARY_EPSILON_M {
        return Vec3::ZERO;
    }

    let raw_offset = (transform.translation - origin).dot(axis);
    let offset = raw_offset.clamp(-half_extent, half_extent);

    if offset >= half_extent - RANGE_TARGET_BOUNDARY_EPSILON_M {
        settings.travel_sign = -1.0;
    } else if offset <= -half_extent + RANGE_TARGET_BOUNDARY_EPSILON_M {
        settings.travel_sign = 1.0;
    } else {
        settings.travel_sign = normalized_travel_sign(settings);
    }

    let bounded_position = origin + axis * offset;
    transform.translation.x = bounded_position.x;
    transform.translation.y = origin.y;
    transform.translation.z = bounded_position.z;

    target_linear_velocity(settings)
}

fn target_linear_axis(settings: &RangeTargetMotionSettings) -> Vec3 {
    let heading = settings.direction_deg.to_radians();
    Vec3::new(heading.sin(), 0.0, heading.cos()).normalize_or_zero()
}

fn normalized_travel_sign(settings: &RangeTargetMotionSettings) -> f32 {
    if settings.travel_sign < 0.0 {
        -1.0
    } else {
        1.0
    }
}

fn target_angular_velocity(settings: &RangeTargetMotionSettings) -> Vec3 {
    match settings.mode {
        RangeTargetMotionMode::Spin | RangeTargetMotionMode::LinearAndSpin => {
            Vec3::Y * settings.spin_deg_s.to_radians()
        }
        RangeTargetMotionMode::Stationary | RangeTargetMotionMode::Linear => Vec3::ZERO,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-5, "lhs={a}, rhs={b}");
    }

    fn assert_truth_gimbal_centers(target: Vec3) -> (f32, f32) {
        let player = Transform::from_translation(Vec3::new(-5.8, 1.0, -5.4))
            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI));
        let launch = Transform::from_translation(Vec3::new(0.0, 0.18, 0.04))
            .with_rotation(Quat::from_rotation_x(-0.48));
        let (yaw, pitch, camera, error_rad) =
            solve_truth_gimbal_command(&player, &launch, target, 0.0, -0.2);
        assert!(yaw.is_finite());
        assert!(pitch.is_finite());
        assert!(error_rad < 5e-4, "target={target:?}, error={error_rad}");
        let camera_target = (target - camera.translation).normalize();
        assert!(
            camera.forward().dot(camera_target) > 0.999_999,
            "target={target:?}, forward={:?}, target_dir={camera_target:?}",
            camera.forward()
        );
        (yaw, pitch)
    }

    #[test]
    fn truth_gimbal_is_default_off_without_explicit_valid_target() {
        assert_eq!(
            ShootingRangeTruthGimbalConfig::from_value(None),
            ShootingRangeTruthGimbalConfig::default()
        );
        assert!(!ShootingRangeTruthGimbalConfig::from_value(Some("")).enabled());
        assert!(!ShootingRangeTruthGimbalConfig::from_value(Some("2")).enabled());
        assert!(!ShootingRangeTruthGimbalConfig::from_value(Some("invalid")).enabled());
        assert_eq!(
            ShootingRangeTruthGimbalConfig::from_value(Some(" 3 ")).target_number,
            Some(3)
        );
        assert_eq!(
            ShootingRangeTruthGimbalConfig::from_value(Some("1")).target_number,
            Some(1)
        );
    }

    #[test]
    fn truth_gimbal_system_initializes_without_query_conflicts() {
        let config = ShootingRangeTruthGimbalConfig::default();
        let mut app = App::new();
        app.insert_resource(ExposureWallTimestamp::default())
            .insert_resource(AutoAimSceneState::new(AutoAimSceneMode::ShootingRange))
            .insert_resource(config)
            .insert_resource(ShootingRangeTruthGimbalStats::from_config(config))
            .add_systems(Update, apply_shooting_range_truth_gimbal);

        app.update();
        assert_eq!(
            app.world()
                .resource::<ShootingRangeTruthGimbalStats>()
                .command_count,
            0
        );
    }

    #[test]
    fn truth_gimbal_centers_stationary_front_target() {
        assert_truth_gimbal_centers(Vec3::new(-5.8, 0.25, -1.0));
    }

    #[test]
    fn truth_gimbal_centers_radially_translated_target() {
        assert_truth_gimbal_centers(Vec3::new(-5.8, 0.25, 4.0));
    }

    #[test]
    fn truth_gimbal_centers_laterally_translated_target() {
        let (yaw, _) = assert_truth_gimbal_centers(Vec3::new(-1.8, 0.25, -1.0));
        assert!(yaw.abs() > 0.2);
    }

    #[test]
    fn truth_gimbal_centers_target_behind_player() {
        let (yaw, _) = assert_truth_gimbal_centers(Vec3::new(-5.8, 0.25, -9.0));
        assert!(yaw.abs() > 2.0);
    }

    #[test]
    fn direction_zero_moves_along_positive_z() {
        let settings = RangeTargetMotionSettings {
            mode: RangeTargetMotionMode::Linear,
            direction_deg: 0.0,
            linear_speed_mps: 2.0,
            ..default()
        };

        let velocity = target_linear_velocity(&settings);

        approx_eq(velocity.x, 0.0);
        approx_eq(velocity.z, 2.0);
    }

    #[test]
    fn direction_ninety_moves_along_positive_x() {
        let settings = RangeTargetMotionSettings {
            mode: RangeTargetMotionMode::Linear,
            direction_deg: 90.0,
            linear_speed_mps: 3.0,
            ..default()
        };

        let velocity = target_linear_velocity(&settings);

        approx_eq(velocity.x, 3.0);
        approx_eq(velocity.z, 0.0);
    }

    #[test]
    fn stationary_mode_zeros_motion() {
        let settings = RangeTargetMotionSettings {
            mode: RangeTargetMotionMode::Stationary,
            direction_deg: 90.0,
            linear_speed_mps: 3.0,
            spin_deg_s: 180.0,
            ..default()
        };

        assert_eq!(target_linear_velocity(&settings), Vec3::ZERO);
        assert_eq!(target_angular_velocity(&settings), Vec3::ZERO);
    }

    #[test]
    fn spin_mode_uses_y_axis_radians_per_second() {
        let settings = RangeTargetMotionSettings {
            mode: RangeTargetMotionMode::Spin,
            spin_deg_s: 180.0,
            ..default()
        };

        let angular = target_angular_velocity(&settings);

        approx_eq(angular.x, 0.0);
        approx_eq(angular.y, std::f32::consts::PI);
        approx_eq(angular.z, 0.0);
    }

    #[test]
    fn wall_pose_spin_uses_exposure_delta_once() {
        let mut settings = RangeTargetMotionSettings {
            mode: RangeTargetMotionMode::Spin,
            spin_deg_s: 6.0_f32.to_degrees(),
            ..default()
        };
        let mut pose = RangeTargetWallPose {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            last_real_elapsed_s: 0.0,
        };

        advance_range_target_wall_pose(&mut pose, &mut settings, Vec3::ZERO, 0.044_642_7);
        let rotated = pose.rotation * Vec3::Z;
        approx_eq(rotated.x, 0.264_665);
        approx_eq(rotated.z, 0.964_341);

        let before_zero_delta = pose.rotation;
        advance_range_target_wall_pose(&mut pose, &mut settings, Vec3::ZERO, 0.0);
        assert!(pose.rotation.abs_diff_eq(before_zero_delta, 1e-6));
    }

    #[test]
    fn wall_pose_translation_reflects_without_physics_catch_up() {
        let mut settings = RangeTargetMotionSettings {
            mode: RangeTargetMotionMode::Linear,
            direction_deg: 90.0,
            linear_speed_mps: 1.0,
            linear_span_m: 8.0,
            travel_sign: 1.0,
            ..default()
        };
        let mut pose = RangeTargetWallPose {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            last_real_elapsed_s: 0.0,
        };

        let (velocity, _) =
            advance_range_target_wall_pose(&mut pose, &mut settings, Vec3::ZERO, 5.0);

        approx_eq(pose.translation.x, 3.0);
        approx_eq(velocity.x, -1.0);
        approx_eq(settings.travel_sign, -1.0);
    }

    #[test]
    fn wall_pose_translation_is_continuous_at_negative_boundary() {
        let mut settings = RangeTargetMotionSettings {
            mode: RangeTargetMotionMode::Linear,
            direction_deg: 90.0,
            linear_speed_mps: 1.0,
            linear_span_m: 8.0,
            travel_sign: -1.0,
            ..default()
        };
        let mut pose = RangeTargetWallPose {
            translation: Vec3::new(-3.995, 0.0, 0.0),
            rotation: Quat::IDENTITY,
            last_real_elapsed_s: 0.0,
        };

        let (velocity, _) =
            advance_range_target_wall_pose(&mut pose, &mut settings, Vec3::ZERO, 0.133);

        approx_eq(pose.translation.x, -3.872);
        approx_eq(velocity.x, 1.0);
        approx_eq(settings.travel_sign, 1.0);
    }

    #[test]
    fn range_target_profile_selects_independent_settings() {
        let mut state = ShootingRangeControlState {
            armor_3: RangeTargetMotionSettings {
                mode: RangeTargetMotionMode::Linear,
                linear_speed_mps: 1.0,
                ..default()
            },
            armor_1: RangeTargetMotionSettings {
                mode: RangeTargetMotionMode::Spin,
                spin_deg_s: 90.0,
                ..default()
            },
            ..default()
        };

        assert_eq!(state.armor_3.mode, RangeTargetMotionMode::Linear);
        assert_eq!(state.armor_1.mode, RangeTargetMotionMode::Spin);

        stop_motion(&mut state.armor_3);

        assert_eq!(state.armor_3.mode, RangeTargetMotionMode::Stationary);
        assert_eq!(state.armor_1.mode, RangeTargetMotionMode::Spin);
    }

    #[test]
    fn radial_geometry_is_absolute_bounded_and_resets() {
        let mut state = ShootingRangeControlState::default();
        assert!(state.set_target_geometry(3, 0.8).is_ok());
        assert_eq!(state.armor_3.radial_scale, 0.8);
        assert!(state.set_target_geometry(3, 0.7).is_err());
        assert!(state.set_target_geometry(3, 1.3).is_err());

        state.armor_3.mode = RangeTargetMotionMode::Spin;
        assert!(state.set_target_geometry(3, 1.0).is_err());
        state.reset_target_geometry();
        assert_eq!(state.armor_3.radial_scale, 1.0);
        assert_eq!(state.armor_1.radial_scale, 1.0);
    }

    #[test]
    fn reciprocal_motion_reverses_at_positive_four_meters() {
        let mut settings = RangeTargetMotionSettings {
            mode: RangeTargetMotionMode::Linear,
            direction_deg: 90.0,
            linear_speed_mps: 1.0,
            linear_span_m: 8.0,
            travel_sign: 1.0,
            ..default()
        };
        let mut transform = Transform::from_translation(Vec3::new(4.2, 0.0, 0.0));

        let velocity = bounded_target_linear_velocity(&mut settings, Vec3::ZERO, &mut transform);

        approx_eq(transform.translation.x, 4.0);
        approx_eq(velocity.x, -1.0);
        approx_eq(settings.travel_sign, -1.0);
    }

    #[test]
    fn reciprocal_motion_reverses_at_negative_four_meters() {
        let mut settings = RangeTargetMotionSettings {
            mode: RangeTargetMotionMode::Linear,
            direction_deg: 90.0,
            linear_speed_mps: 1.0,
            linear_span_m: 8.0,
            travel_sign: -1.0,
            ..default()
        };
        let mut transform = Transform::from_translation(Vec3::new(-4.2, 0.0, 0.0));

        let velocity = bounded_target_linear_velocity(&mut settings, Vec3::ZERO, &mut transform);

        approx_eq(transform.translation.x, -4.0);
        approx_eq(velocity.x, 1.0);
        approx_eq(settings.travel_sign, 1.0);
    }

    #[test]
    fn reciprocal_motion_clamps_total_travel_to_eight_meters() {
        let mut settings = RangeTargetMotionSettings {
            mode: RangeTargetMotionMode::Linear,
            direction_deg: 90.0,
            linear_speed_mps: 2.0,
            linear_span_m: 12.0,
            travel_sign: 1.0,
            ..default()
        };
        let mut transform = Transform::from_translation(Vec3::new(6.0, 0.0, 0.0));

        let velocity = bounded_target_linear_velocity(&mut settings, Vec3::ZERO, &mut transform);

        approx_eq(transform.translation.x, 4.0);
        approx_eq(velocity.x, -2.0);
    }

    #[test]
    fn debug_json_path_prefers_explicit_path() {
        let path = resolve_debug_json_path(
            Some(OsString::from(r"D:\debug\aim_pipeline.json")),
            Some(OsString::from(r"D:\workspace")),
            None,
            "aim_pipeline.json",
        );

        assert_eq!(path, PathBuf::from(r"D:\debug\aim_pipeline.json"));
    }

    #[test]
    fn debug_json_path_uses_workspace_default() {
        let path = resolve_debug_json_path(
            None,
            Some(OsString::from(r"D:\workspace")),
            None,
            "aim_bridge.json",
        );

        assert_eq!(
            path,
            PathBuf::from(r"D:\workspace")
                .join("aim_sim_bridge")
                .join("build")
                .join("debug")
                .join("aim_bridge.json")
        );
    }
}
