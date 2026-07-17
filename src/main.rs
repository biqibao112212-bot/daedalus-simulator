#![allow(dead_code)]
mod auto_gen;
mod capture;
mod components;
mod config;
mod dataset;
mod handler;
mod integrated_auto_aim;
mod network_bridge;
mod robomaster;
mod setup;
mod statistic;
mod systems;
mod telemetry;
mod util;

#[cfg(feature = "ros2")]
mod ros2;
#[cfg(feature = "talos")]
mod talos;

use avian3d::prelude::*;
use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::render::settings::{InstanceFlags, RenderCreation, WgpuSettings, WgpuSettingsPriority};
use bevy::render::{RenderPlugin, RenderSystems};
use bevy::window::{ExitCondition, PresentMode, WindowPosition, WindowResolution};
use bevy::winit::WinitSettings;
use bevy_inspector_egui::bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use clap::Parser;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::auto_gen::AutoGenPlugin;
use crate::capture::{
    ExposureWallTimestamp, advance_exposure_wall_timestamp, update_preview_cadence,
};
use crate::components::{CameraMode, FollowingType, ProjectileCooldown, SubscribeAutoAim};
use crate::config::{ConfigPlugin, SimulationConfig};
use crate::dataset::prelude::DatasetPlugin;
use crate::handler::{on_activate, on_hit};
#[cfg(feature = "talos")]
use crate::integrated_auto_aim::IntegratedAutoAimPlugin;
use crate::integrated_auto_aim::configure_integrated_auto_aim_environment;
use crate::network_bridge::NetworkBridgePlugin;
use crate::robomaster::prelude::RoboMasterPlugins;
use crate::setup::{
    AutoAimSceneState, apply_auto_aim_scene_mode_request, initial_auto_aim_scene_mode, setup,
    setup_collision, setup_dart_launch, setup_ground, setup_vehicle,
};
use crate::statistic::ProjectileStatistics;
use crate::systems::{
    ChassisObservationFrame, GameplaySystems, PreviousKinematicState, auto_aim_switch,
    change_appearance, cleanup_projectiles, dart_launch, export_debug_stats, following_controls,
    freecam_controls, gimbal_controls, mark_main_schedule_begin, mark_main_schedule_end,
    mark_physics_step, mark_physics_step_begin, mouse_gimbal_controls, projectile_aerodynamics,
    projectile_launch, remote_gimbal_controls, remote_vehicle_controls, run_auto_aim_debug_ui_app,
    scene_mode_control_panel, scene_mode_keyboard_shortcuts, screenshot_on_f2, screenshot_saving,
    setup_projectile, shooting_range_control_window, shooting_range_debug_panel,
    spawn_scene_mode_button_panel, switch_slapper_control, toggle_shooting_range_control_window,
    uav_launch, update_chassis_observation, update_frequency_metrics, update_help_text,
    vehicle_controls,
};
use crate::systems::{
    FrequencyMetrics, MainScheduleTiming, PhysicsScheduleTiming, ShootingRangeControlState,
    ShootingRangeDebugPanelState, ShootingRangeDebugProcessState, ShootingRangeTruthGimbalConfig,
    ShootingRangeTruthGimbalStats, apply_shooting_range_target_motion,
    apply_shooting_range_truth_gimbal, handle_scene_mode_button_interactions,
    manage_shooting_range_debug_process, update_scene_mode_button_panel,
};
use crate::telemetry::ProjectileTelemetry;

/// Command-line arguments for the application
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Enable auto dataset generation mode
    #[arg(long)]
    auto_gen: bool,
    /// Run the built-in auto-aim debug UI process
    #[arg(long)]
    debug_ui: bool,
}

#[cfg(feature = "ros2")]
use crate::ros2::plugin::ROS2Plugin;
#[cfg(feature = "talos")]
use talos::TalosPlugin;

fn present_mode_from_config(value: &str) -> Option<PresentMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto_vsync" | "vsync" => Some(PresentMode::AutoVsync),
        "auto_no_vsync" | "no_vsync" | "novsync" => Some(PresentMode::AutoNoVsync),
        "fifo" => Some(PresentMode::Fifo),
        "fifo_relaxed" | "fifo-relaxed" => Some(PresentMode::FifoRelaxed),
        "mailbox" => Some(PresentMode::Mailbox),
        "immediate" => Some(PresentMode::Immediate),
        _ => None,
    }
}

fn is_wsl() -> bool {
    std::env::var_os("WSL_DISTRO_NAME").is_some()
        || std::env::var_os("WSL_INTEROP").is_some()
        || std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .map(|release| release.to_ascii_lowercase().contains("microsoft"))
            .unwrap_or(false)
}

fn render_plugin_for_platform() -> RenderPlugin {
    if cfg!(target_os = "linux") && is_wsl() {
        return RenderPlugin {
            render_creation: RenderCreation::Automatic(Box::new(WgpuSettings {
                instance_flags: InstanceFlags::default()
                    | InstanceFlags::ALLOW_UNDERLYING_NONCOMPLIANT_ADAPTER,
                priority: WgpuSettingsPriority::Functionality,
                ..default()
            })),
            ..default()
        };
    }

    RenderPlugin::default()
}

fn fixed_time_from_config(config: &SimulationConfig) -> Time<Fixed> {
    Time::<Fixed>::from_hz(config.physics.fixed_hz.max(1.0))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PhysicsScheduleMode {
    FixedPostUpdate,
    PostUpdate,
}

fn physics_schedule_mode(post_update: bool) -> PhysicsScheduleMode {
    if post_update {
        PhysicsScheduleMode::PostUpdate
    } else {
        PhysicsScheduleMode::FixedPostUpdate
    }
}

fn physics_plugins_from_config(config: &SimulationConfig) -> PhysicsPlugins {
    match physics_schedule_mode(config.physics.post_update) {
        PhysicsScheduleMode::FixedPostUpdate => PhysicsPlugins::default(),
        PhysicsScheduleMode::PostUpdate => {
            info!(
                "Avian performance profile: physics runs in PostUpdate; Time<Fixed> remains {:.3}Hz",
                config.physics.fixed_hz
            );
            PhysicsPlugins::new(PostUpdate)
        }
    }
}

#[derive(Resource, Debug, Clone, Copy)]
struct FixedPhysicsTickDivider(u32);

#[derive(Resource, Debug, Default, Clone, Copy)]
struct FixedPhysicsStepDue(bool);

#[derive(Default)]
struct FixedPhysicsStepGate {
    tick_in_cycle: u32,
}

impl FixedPhysicsStepGate {
    fn should_step(&mut self, divisor: u32) -> bool {
        let divisor = divisor.max(1);
        let should_step = self.tick_in_cycle == 0;
        self.tick_in_cycle = (self.tick_in_cycle + 1) % divisor;
        should_step
    }
}

fn advance_fixed_physics_gate(
    divider: Res<FixedPhysicsTickDivider>,
    mut gate: Local<FixedPhysicsStepGate>,
    mut due: ResMut<FixedPhysicsStepDue>,
) {
    due.0 = gate.should_step(divider.0);
}

fn fixed_physics_step_due(due: Res<FixedPhysicsStepDue>) -> bool {
    due.0
}

fn configure_physics_runtime(app: &mut App, config: &SimulationConfig) {
    // Count only inner schedule executions. Avian does not run this schedule for a
    // paused or zero-delta StepSimulation pass.
    app.init_resource::<PhysicsScheduleTiming>().add_systems(
        PhysicsSchedule,
        mark_physics_step_begin.before(PhysicsStepSystems::First),
    );
    app.add_systems(
        PhysicsSchedule,
        mark_physics_step.after(PhysicsStepSystems::Last),
    );

    match physics_schedule_mode(config.physics.post_update) {
        PhysicsScheduleMode::FixedPostUpdate => {
            let divisor = config.physics.tick_divisor.max(1);
            if divisor == 1 {
                return;
            }

            app.insert_resource(FixedPhysicsTickDivider(divisor))
                .init_resource::<FixedPhysicsStepDue>()
                .add_systems(
                    FixedPostUpdate,
                    advance_fixed_physics_gate.before(PhysicsSystems::First),
                );
            // Prepare and Writeback are material at 250Hz too. Gate the complete
            // Avian phase chain in lockstep while FixedUpdate control keeps its
            // independent 4ms cadence.
            app.configure_sets(
                FixedPostUpdate,
                (
                    PhysicsSystems::First.run_if(fixed_physics_step_due),
                    PhysicsSystems::Prepare.run_if(fixed_physics_step_due),
                    PhysicsSystems::StepSimulation.run_if(fixed_physics_step_due),
                    PhysicsSystems::Writeback.run_if(fixed_physics_step_due),
                    PhysicsSystems::Last.run_if(fixed_physics_step_due),
                ),
            );
            app.world_mut()
                .resource_mut::<Time<Physics>>()
                .set_relative_speed_f64(divisor as f64);

            info!(
                "Avian fixed-physics divider enabled: one complete physics phase per {divisor} \
                 fixed ticks ({:.3}Hz at {:.3}Hz control), physics relative speed={divisor}",
                config.physics.fixed_hz / divisor as f64,
                config.physics.fixed_hz,
            );
        }
        PhysicsScheduleMode::PostUpdate => {
            if config.physics.tick_divisor > 1 {
                warn!(
                    "Ignoring physics tick divisor {} because PostUpdate physics is selected; \
                     these startup modes are mutually exclusive",
                    config.physics.tick_divisor
                );
            }
        }
    }
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

fn env_u32(key: &str) -> Option<u32> {
    std::env::var(key)
        .ok()
        .and_then(|value| match value.trim().parse::<u32>() {
            Ok(parsed) if parsed > 0 => Some(parsed),
            Ok(_) => {
                warn!("Ignoring invalid {key}={value:?}: window size must be positive");
                None
            }
            Err(error) => {
                warn!("Ignoring invalid {key}={value:?}: {error}");
                None
            }
        })
}

fn simulator_window(present_mode: PresentMode) -> Window {
    let mut window = Window {
        present_mode,
        fit_canvas_to_parent: true,
        ..default()
    };

    if let (Some(x), Some(y)) = (env_i32("DAEDALUS_WINDOW_X"), env_i32("DAEDALUS_WINDOW_Y")) {
        window.position = WindowPosition::At(IVec2::new(x, y));
    }

    if let (Some(width), Some(height)) = (
        env_u32("DAEDALUS_WINDOW_WIDTH"),
        env_u32("DAEDALUS_WINDOW_HEIGHT"),
    ) {
        window.resolution = WindowResolution::new(width, height);
    }

    window
}

fn simulator_asset_folder() -> String {
    [
        std::env::current_dir().ok().map(|path| path.join("assets")),
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.join("assets"))),
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")),
    ]
    .into_iter()
    .flatten()
    .find(|path| path.is_dir())
    .unwrap_or_else(|| PathBuf::from("assets"))
    .to_string_lossy()
    .into_owned()
}

fn auto_aim_enabled_on_start(config: &SimulationConfig) -> bool {
    match std::env::var("DAEDALUS_AUTO_AIM_ON_START") {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => config.auto_aim.enabled_on_start,
        },
        Err(_) => config.auto_aim.enabled_on_start,
    }
}

fn performance_ui_disabled() -> bool {
    std::env::var("DAEDALUS_PERF_DISABLE_UI")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[cfg(feature = "talos")]
fn should_enable_talos_plugin(app: &App) -> bool {
    #[cfg(feature = "ros2")]
    let ros_capture_active = app
        .world()
        .contains_resource::<crate::ros2::capture::RosCaptureContext>();
    #[cfg(not(feature = "ros2"))]
    let ros_capture_active = false;

    let force_talos_capture = std::env::var("DAEDALUS_FORCE_TALOS_CAPTURE")
        .map(|v| v == "1")
        .unwrap_or(false);

    !ros_capture_active || force_talos_capture
}

fn main() {
    let args = Args::parse();

    if args.debug_ui {
        run_auto_aim_debug_ui_app();
        return;
    }

    // Auto-gen mode: minimal setup
    if args.auto_gen {
        let config = SimulationConfig::default();

        let mut app = App::new();
        app.add_plugins((
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: simulator_asset_folder(),
                    ..default()
                })
                .set(render_plugin_for_platform()),
            physics_plugins_from_config(&config),
        ));
        configure_physics_runtime(&mut app, &config);
        app.add_plugins(RoboMasterPlugins)
            .add_plugins(ConfigPlugin)
            .add_observer(setup_vehicle)
            .insert_resource(Gravity(Vec3::ZERO))
            .insert_resource(SubstepCount(config.physics.substep_count))
            .insert_resource(fixed_time_from_config(&config))
            .add_plugins(AutoGenPlugin);
        app.run();
        return;
    }

    // Full simulation mode: existing functionality
    let config = SimulationConfig::default();
    configure_integrated_auto_aim_environment(&config);
    let initial_scene_mode = initial_auto_aim_scene_mode(&config);
    let truth_gimbal_config = ShootingRangeTruthGimbalConfig::from_env();
    let truth_gimbal_stats = ShootingRangeTruthGimbalStats::from_config(truth_gimbal_config);
    let disable_performance_ui = performance_ui_disabled();
    let present_mode = present_mode_from_config(&config.window.present_mode).unwrap_or_else(|| {
        warn!(
            "Unknown window.present_mode {:?}, falling back to auto_no_vsync",
            config.window.present_mode
        );
        PresentMode::AutoNoVsync
    });
    let mut app = App::new();
    // The simulator must keep advancing at full speed when a benchmark window is unfocused.
    app.insert_resource(WinitSettings::continuous());
    app.add_plugins((
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(simulator_window(present_mode)),
                ..default()
            })
            .set(AssetPlugin {
                file_path: simulator_asset_folder(),
                ..default()
            })
            .set(render_plugin_for_platform()),
        physics_plugins_from_config(&config),
    ));
    configure_physics_runtime(&mut app, &config);

    if !disable_performance_ui {
        app.add_plugins(EguiPlugin::default());
        if config.debug.inspector {
            app.add_plugins(WorldInspectorPlugin::new());
        }
    }

    app.add_plugins(RoboMasterPlugins)
        .add_plugins(DatasetPlugin)
        .add_plugins(ConfigPlugin)
        .add_plugins(NetworkBridgePlugin)
        .init_resource::<CameraMode>()
        .init_resource::<ProjectileStatistics>()
        .init_resource::<ProjectileTelemetry>()
        .init_resource::<FrequencyMetrics>()
        .init_resource::<MainScheduleTiming>()
        .init_resource::<ExposureWallTimestamp>()
        .init_resource::<ChassisObservationFrame>()
        .init_resource::<PreviousKinematicState>()
        .init_resource::<ShootingRangeControlState>()
        .init_resource::<ShootingRangeDebugPanelState>()
        .init_resource::<ShootingRangeDebugProcessState>()
        .insert_resource(truth_gimbal_config)
        .insert_resource(truth_gimbal_stats)
        .insert_resource(AutoAimSceneState::new(initial_scene_mode))
        .register_type::<ProjectileStatistics>()
        .insert_resource(Gravity(Vec3::NEG_Y * 9.81))
        .insert_resource(SubstepCount(config.physics.substep_count))
        .insert_resource(fixed_time_from_config(&config))
        .insert_resource(SubscribeAutoAim(AtomicBool::new(
            auto_aim_enabled_on_start(&config),
        )))
        .insert_resource(ProjectileCooldown(Timer::from_seconds(
            config.projectile.cooldown,
            TimerMode::Once,
        )))
        .add_systems(Startup, (setup, setup_projectile))
        .add_systems(
            Startup,
            spawn_scene_mode_button_panel.run_if(|| !performance_ui_disabled()),
        )
        .add_observer(setup_ground)
        .add_observer(setup_dart_launch)
        .add_observer(setup_vehicle)
        .add_observer(setup_collision)
        .add_systems(First, mark_main_schedule_begin)
        .add_systems(Last, mark_main_schedule_end)
        .add_observer(on_hit)
        .add_observer(on_activate)
        .configure_sets(
            Update,
            (
                GameplaySystems::Input,
                GameplaySystems::GameLogic,
                GameplaySystems::Camera,
                GameplaySystems::Cleanup,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                // Input phase
                (
                    auto_aim_switch,
                    following_controls,
                    switch_slapper_control,
                    vehicle_controls.run_if(|mode: Res<CameraMode>| mode.0 != FollowingType::Free),
                    remote_vehicle_controls,
                    gimbal_controls,
                    mouse_gimbal_controls,
                    remote_gimbal_controls,
                    scene_mode_keyboard_shortcuts,
                    handle_scene_mode_button_interactions,
                    toggle_shooting_range_control_window,
                )
                    .in_set(GameplaySystems::Input),
                // GameLogic phase
                (
                    update_frequency_metrics,
                    apply_auto_aim_scene_mode_request,
                    manage_shooting_range_debug_process,
                    change_appearance,
                    update_scene_mode_button_panel,
                    update_help_text,
                    export_debug_stats,
                )
                    .in_set(GameplaySystems::GameLogic),
                // Camera phase
                (
                    freecam_controls.run_if(|mode: Res<CameraMode>| mode.0 == FollowingType::Free),
                    systems::update_camera_follow
                        .run_if(|mode: Res<CameraMode>| mode.0 != FollowingType::Free),
                )
                    .in_set(GameplaySystems::Camera)
                    .before(RenderSystems::Render),
                // Cleanup phase
                (
                    cleanup_projectiles,
                    screenshot_on_f2
                        .run_if(|input: Res<ButtonInput<KeyCode>>| input.just_pressed(KeyCode::F2)),
                    screenshot_saving,
                )
                    .in_set(GameplaySystems::Cleanup),
            ),
        )
        .add_systems(
            Update,
            update_preview_cadence
                .before(update_frequency_metrics)
                .before(RenderSystems::Render),
        )
        .add_systems(
            PostUpdate,
            (
                advance_exposure_wall_timestamp,
                apply_shooting_range_target_motion,
                apply_shooting_range_truth_gimbal,
            )
                .chain()
                .before(TransformSystems::Propagate),
        )
        .add_systems(
            PostUpdate,
            update_chassis_observation.after(TransformSystems::Propagate),
        )
        .add_systems(
            PostUpdate,
            projectile_launch.after(TransformSystems::Propagate).run_if(
                |keyboard: Res<ButtonInput<KeyCode>>, auto_aim: Res<SubscribeAutoAim>| {
                    keyboard.pressed(KeyCode::Space) && !auto_aim.load(Ordering::Acquire)
                },
            ),
        )
        .add_systems(
            PostUpdate,
            dart_launch
                .after(TransformSystems::Propagate)
                .run_if(|keyboard: Res<ButtonInput<KeyCode>>| keyboard.just_pressed(KeyCode::KeyG)),
        )
        .add_systems(PostUpdate, uav_launch.after(TransformSystems::Propagate))
        .add_systems(FixedUpdate, projectile_aerodynamics);

    if !disable_performance_ui {
        app.add_systems(
            EguiPrimaryContextPass,
            (
                scene_mode_control_panel,
                shooting_range_control_window,
                shooting_range_debug_panel,
            )
                .chain(),
        );
    }

    if config.debug.diagnostics {
        app.add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            LogDiagnosticsPlugin::default(),
        ));
    }

    #[cfg(feature = "ros2")]
    {
        app.add_plugins(ROS2Plugin::default());
        info!("ROS2 integration enabled");
    }
    #[cfg(not(feature = "ros2"))]
    {
        info!("ROS2 integration disabled");
    }

    #[cfg(feature = "talos")]
    {
        app.add_plugins(IntegratedAutoAimPlugin);
        if should_enable_talos_plugin(&app) {
            app.add_plugins(TalosPlugin::default());
            info!("talos integration enabled");
        } else {
            info!(
                "talos integration skipped: ROS2 capture already active \
                 (set DAEDALUS_FORCE_TALOS_CAPTURE=1 to override)"
            );
        }
    }

    app.run();
}

#[cfg(test)]
mod tests {
    use avian3d::prelude::{Physics, PhysicsTime};
    use bevy::prelude::{App, Time};

    use super::{
        FixedPhysicsStepGate, FixedPhysicsTickDivider, PhysicsScheduleMode,
        configure_physics_runtime, fixed_time_from_config, physics_schedule_mode,
    };

    #[test]
    fn physics_schedule_selector_is_fixed_by_default_and_post_update_when_enabled() {
        assert_eq!(
            physics_schedule_mode(false),
            PhysicsScheduleMode::FixedPostUpdate
        );
        assert_eq!(physics_schedule_mode(true), PhysicsScheduleMode::PostUpdate);
    }

    #[test]
    fn post_update_physics_selector_does_not_change_fixed_control_timestep() {
        let mut config = crate::config::SimulationConfig::default();
        config.physics.post_update = true;
        config.physics.substep_count = 1;
        config.physics.fixed_hz = 250.0;

        let fixed_time = fixed_time_from_config(&config);
        assert!((fixed_time.timestep().as_secs_f64() - 0.004).abs() < f64::EPSILON);
    }

    #[test]
    fn fixed_physics_gate_runs_once_per_divisor_cycle() {
        let mut default_gate = FixedPhysicsStepGate::default();
        assert!((0..8).all(|_| default_gate.should_step(1)));

        let mut divided_gate = FixedPhysicsStepGate::default();
        let decisions = (0..9)
            .map(|_| divided_gate.should_step(4))
            .collect::<Vec<_>>();
        assert_eq!(
            decisions,
            vec![true, false, false, false, true, false, false, false, true]
        );
    }

    #[test]
    fn divisor_four_scales_physics_time_but_keeps_control_at_four_milliseconds() {
        let mut config = crate::config::SimulationConfig::default();
        config.physics.post_update = false;
        config.physics.tick_divisor = 4;
        config.physics.fixed_hz = 250.0;

        let mut app = App::new();
        app.init_resource::<Time<Physics>>();
        configure_physics_runtime(&mut app, &config);

        let physics_time = app.world().resource::<Time<Physics>>();
        assert_eq!(physics_time.relative_speed_f64(), 4.0);
        assert!(app.world().contains_resource::<FixedPhysicsTickDivider>());
        assert!(
            (fixed_time_from_config(&config).timestep().as_secs_f64() - 0.004).abs() < f64::EPSILON
        );
    }

    #[test]
    fn post_update_runtime_defensively_ignores_a_nondefault_divisor() {
        let mut config = crate::config::SimulationConfig::default();
        config.physics.post_update = true;
        config.physics.tick_divisor = 4;

        let mut app = App::new();
        app.init_resource::<Time<Physics>>();
        configure_physics_runtime(&mut app, &config);

        assert_eq!(
            app.world().resource::<Time<Physics>>().relative_speed_f64(),
            1.0
        );
        assert!(!app.world().contains_resource::<FixedPhysicsTickDivider>());
    }
}
