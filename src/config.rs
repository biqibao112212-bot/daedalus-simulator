use avian3d::prelude::SubstepCount;
use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender, unbounded};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use std::path::PathBuf;

const PHYSICS_SUBSTEPS_ENV: &str = "DAEDALUS_PHYSICS_SUBSTEPS";
const MAX_PHYSICS_SUBSTEPS_OVERRIDE: u32 = 32;
const PHYSICS_POST_UPDATE_ENV: &str = "DAEDALUS_PHYSICS_POST_UPDATE";
const PHYSICS_TICK_DIVISOR_ENV: &str = "DAEDALUS_PHYSICS_TICK_DIVISOR";
const MAX_PHYSICS_TICK_DIVISOR_OVERRIDE: u32 = 16;
const PREVIEW_ENABLED_ENV: &str = "DAEDALUS_PREVIEW_ENABLED";
const PREVIEW_MAX_HZ_ENV: &str = "DAEDALUS_PREVIEW_MAX_HZ";
const MAX_PREVIEW_HZ_OVERRIDE: f64 = 240.0;

#[derive(Resource, Deserialize, Reflect, Clone)]
#[reflect(Resource)]
pub struct SimulationConfig {
    #[serde(default)]
    pub window: WindowConfig,
    #[serde(default)]
    pub debug: DebugConfig,
    #[serde(default)]
    pub preview: PreviewConfig,
    #[serde(default)]
    pub render: RenderConfig,
    #[serde(default)]
    pub capture: CapturePipelineConfig,
    #[serde(default)]
    pub arena_boundary: ArenaBoundaryConfig,
    #[serde(default)]
    pub livox_ros: LivoxRosConfig,
    #[serde(default)]
    pub network_bridge: NetworkBridgeConfig,
    #[serde(default)]
    pub auto_aim: AutoAimConfig,
    pub physics: PhysicsConfig,
    pub vehicle: VehicleConfig,
    #[serde(default)]
    pub mecanum: MecanumConfig,
    pub projectile: ProjectileConfig,
    pub camera: CameraConfig,
}

#[derive(Deserialize, Reflect, Clone)]
pub struct WindowConfig {
    pub present_mode: String,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            // Uncap rendering by default so off-screen capture (Talos/ROS2) can exceed 60Hz.
            present_mode: "auto_no_vsync".to_string(),
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct DebugConfig {
    pub egui: bool,
    pub inspector: bool,
    pub diagnostics: bool,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            egui: false,
            inspector: false,
            diagnostics: false,
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct PreviewConfig {
    pub enabled: bool,
    /// Zero means the visible preview camera is unlimited.
    pub max_hz: f64,
}

impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_hz: 0.0,
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct RenderConfig {
    pub illuminance: f32,
    pub shadows: bool,
    pub main_camera_fxaa: bool,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            illuminance: 50.0,
            shadows: false,
            main_camera_fxaa: false,
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct ArenaBoundaryConfig {
    pub enabled: bool,
    pub half_width: f32,
    pub half_depth: f32,
    pub height: f32,
    pub thickness: f32,
}

impl Default for ArenaBoundaryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            half_width: 14.0,
            half_depth: 7.5,
            height: 4.0,
            thickness: 0.5,
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct PhysicsConfig {
    pub substep_count: u32,
    pub fixed_hz: f64,
    /// Startup-only selector for variable-step Avian physics in PostUpdate.
    pub post_update: bool,
    /// Startup-only divisor for Avian simulation steps; fixed control ticks stay unchanged.
    pub tick_divisor: u32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            substep_count: 8,
            fixed_hz: 250.0,
            post_update: false,
            tick_divisor: 1,
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
pub struct VehicleConfig {
    pub rotation_speed: f32,
    pub gimbal_rotation_speed: f32,
    pub gimbal_pitch_limit: f32,
    pub max_speed: f32,
    pub linear_acceleration: f32,
    pub acceleration_exponent: f32,
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct MecanumConfig {
    pub wheel_radius_m: f32,
    pub half_wheelbase_m: f32,
    pub half_trackwidth_m: f32,
}

impl Default for MecanumConfig {
    fn default() -> Self {
        Self {
            wheel_radius_m: 0.076,
            half_wheelbase_m: 0.18,
            half_trackwidth_m: 0.15,
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
pub struct ProjectileConfig {
    pub lifetime: f32,
    pub speed: f32,
    pub cooldown: f32,
    pub diameter: f32,
    pub uav_size: f32,
    pub uav_vel: f32,
    pub mass: f32,
    pub friction: f32,
    pub linear_damping: f32,
    #[serde(default)]
    pub aerodynamics: ProjectileAerodynamicsConfig,
}

#[derive(Deserialize, Reflect, Clone)]
pub struct ProjectileAerodynamicsConfig {
    pub enabled: bool,
    pub air_density: f32,
    pub drag_coefficient: f32,
    pub wind: [f32; 3],
}

impl Default for ProjectileAerodynamicsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            // kg/m^3 - air density at sea level (15°C)
            air_density: 1.225,
            // Drag coefficient for a smooth sphere, typical Re for 17mm @ ~25m/s.
            drag_coefficient: 0.47,
            // m/s - wind velocity in world coordinates.
            wind: [0.0, 0.0, 0.0],
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
pub struct CameraConfig {
    pub fov: f32,
    pub free_move_speed: f32,
    pub follow_offset: [f32; 3],
    pub mouse_sensitivity: f32,
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct CapturePipelineConfig {
    pub color: CaptureStreamConfig,
    pub depth: DepthCaptureConfig,
}

impl Default for CapturePipelineConfig {
    fn default() -> Self {
        Self {
            color: CaptureStreamConfig::default(),
            depth: DepthCaptureConfig::default(),
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct CaptureStreamConfig {
    pub width: u32,
    pub height: u32,
}

impl Default for CaptureStreamConfig {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct DepthCaptureConfig {
    pub width: u32,
    pub height: u32,
    pub near: f32,
    pub far: f32,
}

impl Default for DepthCaptureConfig {
    fn default() -> Self {
        Self {
            width: 640,
            height: 480,
            near: 0.1,
            far: 80.0,
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct LivoxRosConfig {
    pub enabled: bool,
    pub frame_id: String,
    pub publish_freq: f32,
    pub points_per_second: u32,
    pub line_num: u8,
    pub tag_default: u8,
    pub intensity_default: f32,
}

impl Default for LivoxRosConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            frame_id: "livox_frame".to_string(),
            publish_freq: 10.0,
            points_per_second: 100_000,
            line_num: 6,
            tag_default: 0,
            intensity_default: 100.0,
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct NetworkBridgeConfig {
    pub enabled: bool,
    pub bind: String,
}

impl Default for NetworkBridgeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind: "0.0.0.0:5601".to_string(),
        }
    }
}

#[derive(Deserialize, Reflect, Clone)]
#[serde(default)]
pub struct AutoAimConfig {
    pub enabled_on_start: bool,
    pub managed_bridge_enabled: bool,
    pub command_transport: String,
    pub bridge_mode: String,
    pub wsl_distro: String,
    pub workspace_dir: String,
    pub talos_ipc_dir: String,
    pub bridge_workdir: String,
}

impl Default for AutoAimConfig {
    fn default() -> Self {
        Self {
            enabled_on_start: false,
            managed_bridge_enabled: false,
            command_transport: "udp".to_string(),
            bridge_mode: "armor".to_string(),
            wsl_distro: "Ubuntu-OSTEP".to_string(),
            workspace_dir: "../..".to_string(),
            talos_ipc_dir: "talos-ipc".to_string(),
            bridge_workdir: "aim_sim_bridge".to_string(),
        }
    }
}

impl SimulationConfig {
    pub fn load() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let path = config_path();
        let content = std::fs::read_to_string(&path)?;
        let mut config: Self = toml::from_str(&content)?;
        config.apply_env_overrides();
        Ok(config)
    }

    fn apply_env_overrides(&mut self) {
        if let Some(fixed_hz) = env_f64("DAEDALUS_FIXED_HZ") {
            self.physics.fixed_hz = fixed_hz;
        }
        self.physics.substep_count =
            physics_substeps_from_env(self.physics.substep_count, PHYSICS_SUBSTEPS_ENV);
        self.physics.post_update = bool_from_env(self.physics.post_update, PHYSICS_POST_UPDATE_ENV);
        self.physics.tick_divisor =
            physics_tick_divisor_from_env(self.physics.tick_divisor, PHYSICS_TICK_DIVISOR_ENV);
        let requested_tick_divisor = self.physics.tick_divisor;
        self.physics.tick_divisor =
            resolve_physics_tick_divisor(self.physics.post_update, requested_tick_divisor);
        if self.physics.tick_divisor != requested_tick_divisor {
            warn!(
                "{PHYSICS_POST_UPDATE_ENV}=true is mutually exclusive with \
                 {PHYSICS_TICK_DIVISOR_ENV}>1; keeping PostUpdate and forcing the divisor to 1"
            );
        }
        self.preview.enabled = bool_from_env(self.preview.enabled, PREVIEW_ENABLED_ENV);
        self.preview.max_hz = preview_max_hz_from_env(self.preview.max_hz, PREVIEW_MAX_HZ_ENV);
        if let Some(fov_deg) = env_f32("DAEDALUS_CAMERA_FOV_DEG") {
            self.camera.fov = fov_deg;
        }
        if let Some(gimbal_rad_s) = env_f32("DAEDALUS_GIMBAL_ROTATION_SPEED_RAD_S") {
            self.vehicle.gimbal_rotation_speed = gimbal_rad_s;
        }
        if let Some(gimbal_deg_s) = env_f32("DAEDALUS_GIMBAL_ROTATION_SPEED_DEG_S") {
            self.vehicle.gimbal_rotation_speed = gimbal_deg_s.to_radians();
        }
        if let Some(fire_rate_hz) = env_f32("DAEDALUS_PROJECTILE_FIRE_RATE_HZ") {
            self.projectile.cooldown = 1.0 / fire_rate_hz;
        }
        if let Some(cooldown_s) = env_f32("DAEDALUS_PROJECTILE_COOLDOWN_S") {
            self.projectile.cooldown = cooldown_s;
        }
    }
}

fn parse_physics_substeps(value: &str) -> Option<u32> {
    value
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|substeps| (1..=MAX_PHYSICS_SUBSTEPS_OVERRIDE).contains(substeps))
}

fn physics_substeps_from_env(configured: u32, key: &str) -> u32 {
    match std::env::var(key) {
        Ok(value) => match parse_physics_substeps(&value) {
            Some(substeps) => {
                info!("Applying {key}={substeps}; configured value was {configured}");
                substeps
            }
            None => {
                warn!(
                    "Ignoring invalid {key}={value:?}; expected an integer in 1..={MAX_PHYSICS_SUBSTEPS_OVERRIDE}, keeping {configured}"
                );
                configured
            }
        },
        Err(std::env::VarError::NotPresent) => configured,
        Err(std::env::VarError::NotUnicode(_)) => {
            warn!("Ignoring non-Unicode {key}; keeping {configured}");
            configured
        }
    }
}

fn parse_physics_tick_divisor(value: &str) -> Option<u32> {
    value
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|divisor| (1..=MAX_PHYSICS_TICK_DIVISOR_OVERRIDE).contains(divisor))
}

fn physics_tick_divisor_from_env(configured: u32, key: &str) -> u32 {
    let configured = match parse_physics_tick_divisor(&configured.to_string()) {
        Some(divisor) => divisor,
        None => {
            warn!(
                "Configured physics tick divisor {configured} is outside 1..={MAX_PHYSICS_TICK_DIVISOR_OVERRIDE}; using 1"
            );
            1
        }
    };

    match std::env::var(key) {
        Ok(value) => match parse_physics_tick_divisor(&value) {
            Some(divisor) => {
                info!("Applying {key}={divisor}; configured value was {configured}");
                divisor
            }
            None => {
                warn!(
                    "Ignoring invalid {key}={value:?}; expected an integer in 1..={MAX_PHYSICS_TICK_DIVISOR_OVERRIDE}, keeping {configured}"
                );
                configured
            }
        },
        Err(std::env::VarError::NotPresent) => configured,
        Err(std::env::VarError::NotUnicode(_)) => {
            warn!("Ignoring non-Unicode {key}; keeping {configured}");
            configured
        }
    }
}

fn resolve_physics_tick_divisor(post_update: bool, tick_divisor: u32) -> u32 {
    if post_update && tick_divisor > 1 {
        1
    } else {
        tick_divisor
    }
}

fn parse_env_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn bool_from_env(configured: bool, key: &str) -> bool {
    match std::env::var(key) {
        Ok(value) => match parse_env_bool(&value) {
            Some(enabled) => {
                info!("Applying {key}={enabled}; configured value was {configured}");
                enabled
            }
            None => {
                warn!(
                    "Ignoring invalid {key}={value:?}; expected true/false or 1/0, keeping {configured}"
                );
                configured
            }
        },
        Err(std::env::VarError::NotPresent) => configured,
        Err(std::env::VarError::NotUnicode(_)) => {
            warn!("Ignoring non-Unicode {key}; keeping {configured}");
            configured
        }
    }
}

fn parse_preview_max_hz(value: &str) -> Option<f64> {
    let normalized = value.trim().to_ascii_lowercase();
    if matches!(normalized.as_str(), "0" | "off" | "unlimited") {
        return Some(0.0);
    }

    normalized
        .parse::<f64>()
        .ok()
        .filter(|hz| hz.is_finite() && *hz >= 1.0 && *hz <= MAX_PREVIEW_HZ_OVERRIDE)
}

fn preview_max_hz_from_env(configured: f64, key: &str) -> f64 {
    match std::env::var(key) {
        Ok(value) => match parse_preview_max_hz(&value) {
            Some(max_hz) => {
                info!("Applying {key}={max_hz}; configured value was {configured}");
                max_hz
            }
            None => {
                warn!(
                    "Ignoring invalid {key}={value:?}; expected 0/unlimited or a number in 1..={MAX_PREVIEW_HZ_OVERRIDE}, keeping {configured}"
                );
                configured
            }
        },
        Err(std::env::VarError::NotPresent) => configured,
        Err(std::env::VarError::NotUnicode(_)) => {
            warn!("Ignoring non-Unicode {key}; keeping {configured}");
            configured
        }
    }
}

fn env_f32(key: &str) -> Option<f32> {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
}

fn env_f64(key: &str) -> Option<f64> {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
}

fn config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("DAEDALUS_CONFIG")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_file())
    {
        return path;
    }

    [
        std::env::current_dir()
            .ok()
            .map(|path| path.join("config.toml")),
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.join("config.toml"))),
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config.toml")),
    ]
    .into_iter()
    .flatten()
    .find(|path| path.is_file())
    .unwrap_or_else(|| PathBuf::from("config.toml"))
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self::load().unwrap_or_else(|e| {
            warn!("Failed to load config.toml: {}, using defaults", e);
            let mut config = Self {
                window: WindowConfig::default(),
                debug: DebugConfig::default(),
                preview: PreviewConfig::default(),
                render: RenderConfig::default(),
                capture: CapturePipelineConfig::default(),
                arena_boundary: ArenaBoundaryConfig::default(),
                livox_ros: LivoxRosConfig::default(),
                network_bridge: NetworkBridgeConfig::default(),
                auto_aim: AutoAimConfig::default(),
                physics: PhysicsConfig::default(),
                vehicle: VehicleConfig {
                    rotation_speed: 3.0,
                    gimbal_rotation_speed: 3.0,
                    gimbal_pitch_limit: 0.785,
                    max_speed: 4.0,
                    linear_acceleration: 8.0,
                    acceleration_exponent: 10.0,
                },
                mecanum: MecanumConfig::default(),
                projectile: ProjectileConfig {
                    lifetime: 5.0,
                    speed: 25.0,
                    cooldown: 0.1,
                    diameter: 0.017,
                    mass: 0.017,
                    friction: 1.1,
                    linear_damping: 0.0,
                    aerodynamics: ProjectileAerodynamicsConfig::default(),
                    uav_size: 1.0,
                    uav_vel: 2.0,
                },
                camera: CameraConfig {
                    fov: 45.0,
                    free_move_speed: 8.0,
                    follow_offset: [0.0, 3.0, 2.0],
                    mouse_sensitivity: 0.003,
                },
            };
            config.apply_env_overrides();
            config
        })
    }
}

#[derive(Resource)]
pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
    receiver: Receiver<Result<Event, notify::Error>>,
}

pub struct ConfigPlugin;

impl Plugin for ConfigPlugin {
    fn build(&self, app: &mut App) {
        let config = SimulationConfig::default();

        // Set up file watcher using crossbeam-channel for thread safety
        let (tx, rx): (
            Sender<Result<Event, notify::Error>>,
            Receiver<Result<Event, notify::Error>>,
        ) = unbounded();
        let watcher_result = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            notify::Config::default(),
        );

        match watcher_result {
            Ok(mut watcher) => {
                let path = config_path();
                if let Err(e) = watcher.watch(path.as_path(), RecursiveMode::NonRecursive) {
                    warn!("Failed to watch config.toml: {}", e);
                } else {
                    info!("Config hot-reload enabled for {}", path.display());
                    app.insert_resource(ConfigWatcher {
                        _watcher: watcher,
                        receiver: rx,
                    });
                    app.add_systems(Update, config_hot_reload);
                }
            }
            Err(e) => {
                warn!("Failed to create config watcher: {}", e);
            }
        }

        app.insert_resource(config)
            .register_type::<SimulationConfig>();
    }
}

fn config_hot_reload(
    mut config: ResMut<SimulationConfig>,
    watcher: Option<Res<ConfigWatcher>>,
    mut substeps: Option<ResMut<SubstepCount>>,
    mut fixed_time: Option<ResMut<Time<Fixed>>>,
) {
    let Some(watcher) = watcher else {
        return;
    };

    // Non-blocking check for file changes
    while let Ok(Ok(event)) = watcher.receiver.try_recv() {
        if event.kind.is_modify() {
            match SimulationConfig::load() {
                Ok(mut new_config) => {
                    info!("Config reloaded successfully");
                    preserve_startup_physics_selectors(&config.physics, &mut new_config.physics);
                    if let Some(substeps) = substeps.as_deref_mut() {
                        substeps.0 = new_config.physics.substep_count;
                    }
                    if let Some(fixed_time) = fixed_time.as_deref_mut() {
                        *fixed_time = Time::<Fixed>::from_hz(new_config.physics.fixed_hz.max(1.0));
                    }
                    *config = new_config;
                }
                Err(e) => {
                    warn!("Failed to reload config: {}", e);
                }
            }
        }
    }
}

fn preserve_startup_physics_selectors(current: &PhysicsConfig, reloaded: &mut PhysicsConfig) {
    if reloaded.post_update != current.post_update || reloaded.tick_divisor != current.tick_divisor
    {
        warn!(
            "Ignoring hot-reloaded physics schedule selectors; \
             DAEDALUS_PHYSICS_POST_UPDATE and DAEDALUS_PHYSICS_TICK_DIVISOR are startup-only"
        );
    }
    reloaded.post_update = current.post_update;
    reloaded.tick_divisor = current.tick_divisor;
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_PHYSICS_SUBSTEPS_OVERRIDE, MAX_PHYSICS_TICK_DIVISOR_OVERRIDE, MAX_PREVIEW_HZ_OVERRIDE,
        PhysicsConfig, PreviewConfig, parse_env_bool, parse_physics_substeps,
        parse_physics_tick_divisor, parse_preview_max_hz, preserve_startup_physics_selectors,
        resolve_physics_tick_divisor,
    };

    #[test]
    fn performance_overrides_do_not_change_configured_defaults() {
        assert_eq!(PhysicsConfig::default().substep_count, 8);
        assert_eq!(PhysicsConfig::default().fixed_hz, 250.0);
        assert!(!PhysicsConfig::default().post_update);
        assert_eq!(PhysicsConfig::default().tick_divisor, 1);
        assert!(PreviewConfig::default().enabled);
        assert_eq!(PreviewConfig::default().max_hz, 0.0);
    }

    #[test]
    fn physics_substep_override_is_positive_and_bounded() {
        assert_eq!(parse_physics_substeps("1"), Some(1));
        assert_eq!(parse_physics_substeps(" 8 "), Some(8));
        assert_eq!(
            parse_physics_substeps(&MAX_PHYSICS_SUBSTEPS_OVERRIDE.to_string()),
            Some(MAX_PHYSICS_SUBSTEPS_OVERRIDE)
        );
        assert_eq!(parse_physics_substeps("0"), None);
        assert_eq!(
            parse_physics_substeps(&(MAX_PHYSICS_SUBSTEPS_OVERRIDE + 1).to_string()),
            None
        );
        assert_eq!(parse_physics_substeps("invalid"), None);
    }

    #[test]
    fn physics_tick_divisor_is_positive_bounded_and_supports_target_row() {
        assert_eq!(parse_physics_tick_divisor("1"), Some(1));
        assert_eq!(parse_physics_tick_divisor(" 4 "), Some(4));
        assert_eq!(
            parse_physics_tick_divisor(&MAX_PHYSICS_TICK_DIVISOR_OVERRIDE.to_string()),
            Some(MAX_PHYSICS_TICK_DIVISOR_OVERRIDE)
        );
        assert_eq!(parse_physics_tick_divisor("0"), None);
        assert_eq!(
            parse_physics_tick_divisor(&(MAX_PHYSICS_TICK_DIVISOR_OVERRIDE + 1).to_string()),
            None
        );
        assert_eq!(parse_physics_tick_divisor("invalid"), None);
    }

    #[test]
    fn post_update_wins_mutual_exclusion_and_selectors_remain_startup_only() {
        assert_eq!(resolve_physics_tick_divisor(false, 4), 4);
        assert_eq!(resolve_physics_tick_divisor(true, 4), 1);
        assert_eq!(resolve_physics_tick_divisor(true, 1), 1);

        let current = PhysicsConfig {
            post_update: false,
            tick_divisor: 4,
            ..PhysicsConfig::default()
        };
        let mut reloaded = PhysicsConfig {
            post_update: true,
            tick_divisor: 1,
            ..PhysicsConfig::default()
        };
        preserve_startup_physics_selectors(&current, &mut reloaded);
        assert!(!reloaded.post_update);
        assert_eq!(reloaded.tick_divisor, 4);
    }

    #[test]
    fn preview_override_parser_accepts_explicit_boolean_values() {
        for value in ["1", "true", "TRUE", " yes ", "On"] {
            assert_eq!(parse_env_bool(value), Some(true));
        }
        for value in ["0", "false", "FALSE", " no ", "Off"] {
            assert_eq!(parse_env_bool(value), Some(false));
        }
        assert_eq!(parse_env_bool("invalid"), None);
    }

    #[test]
    fn preview_cadence_override_is_optional_and_bounded() {
        assert_eq!(parse_preview_max_hz("0"), Some(0.0));
        assert_eq!(parse_preview_max_hz(" unlimited "), Some(0.0));
        assert_eq!(parse_preview_max_hz("60"), Some(60.0));
        assert_eq!(
            parse_preview_max_hz(&MAX_PREVIEW_HZ_OVERRIDE.to_string()),
            Some(MAX_PREVIEW_HZ_OVERRIDE)
        );
        assert_eq!(parse_preview_max_hz("0.5"), None);
        assert_eq!(
            parse_preview_max_hz(&(MAX_PREVIEW_HZ_OVERRIDE + 1.0).to_string()),
            None
        );
        assert_eq!(parse_preview_max_hz("invalid"), None);
    }
}
