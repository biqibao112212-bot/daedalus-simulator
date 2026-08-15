use avian3d::prelude::*;
use bevy::anti_alias::fxaa::Fxaa;
use bevy::camera::Exposure;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::world_serialization::{WorldInstance, WorldInstanceReady};
use bevy_inspector_egui::bevy_egui::{EguiGlobalSettings, PrimaryEguiContext};
use std::collections::HashMap;
use std::f32::consts::PI;

use crate::capture::{
    AimCaptureTargetCandidate, CaptureSceneProfile, DEFAULT_CAPTURE_CLEAR_COLOR,
    FIXED_CAMERA_EV100, aim_target_render_layers, mark_aim_capture_target,
};
use crate::components::{
    ActiveSlapper, Controlled, DartLaunch, GameLayer, GroundRoot, Infantry, InfantryChassis,
    InfantryGimbal, InfantryLaunchOffset, InfantryViewOffset, MainCamera, PreciousCollision,
    SlapperInfantry,
};
use crate::config::ArenaBoundaryConfig;
use crate::config::SimulationConfig;
use crate::robomaster::prelude::{
    HERO_ROBOT_CONFIG, INFANTRY_THREE_CONFIG, OutpostRoot, PowerRuneRoot, Projectile, RobotConfig,
    ScanArmor, Team, TechCoreRoot,
};
use crate::robomaster::vehicle::movement::VehicleDynamic;
use crate::statistic::ProjectileStatistics;
use crate::systems::spawn_text;
use crate::util::entity_query::HierarchyQuery;

#[derive(Component)]
pub struct ScanOutpost;

#[derive(Component)]
pub struct ArenaBoundary;

#[derive(Component)]
pub struct UprightArmorDebugTarget;

#[derive(Component)]
pub struct ShootingRangeTarget {
    pub control_index: usize,
    pub kind: ShootingRangeTargetKind,
    pub origin: Vec3,
    /// Last radial geometry scale applied to this target's armor roots.
    /// `NaN` means that the geometry has not been initialized yet.
    pub applied_geometry_scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShootingRangeTargetKind {
    Armor3,
    Armor1,
}

impl ShootingRangeTargetKind {
    pub fn number(self) -> u8 {
        match self {
            Self::Armor3 => 3,
            Self::Armor1 => 1,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Armor3 => "#3 target",
            Self::Armor1 => "#1 target",
        }
    }
}

const LOCAL_TEST_SPAWN: Vec3 = Vec3::new(-4.0, 1.0, -3.8);
const LOCAL_TEST_GIMBAL_PITCH_RAD: f32 = -0.16;
// The centre approach at x=0 intersects the energy-field ramp. Keep the
// participant vehicle on the flat west apron instead.  GROUND_DENSE is an
// asynchronously imported triangle mesh, so the release scene also creates an
// immediate, invisible collider below this exact patch of apron.
const ENERGY_TEST_SPAWN: Vec3 = Vec3::new(-4.0, 0.0, -5.6);
const ENERGY_TEST_GROUND_SURFACE_Y: f32 = -0.245_722_3;
const ENERGY_TEST_SPAWN_SUPPORT_THICKNESS: f32 = 0.12;
const ENERGY_TEST_SPAWN_SUPPORT_WIDTH: f32 = 2.4;
const ENERGY_TEST_SPAWN_SUPPORT_DEPTH: f32 = 2.0;
const ENERGY_TEST_YAW_RAD: f32 = PI;
const ENERGY_TEST_GIMBAL_YAW_RAD: f32 = 0.0;
// Match the known-good range view. The former -0.16 rad pose left the energy
// camera near the horizon and could present an apparent rolled-over view.
const ENERGY_TEST_GIMBAL_PITCH_RAD: f32 = -27.0 * PI / 180.0;
const OUTPOST_TEST_SPAWN: Vec3 = Vec3::new(-3.06, 1.0, -1.2);
const OUTPOST_TEST_GIMBAL_PITCH_RAD: f32 = -0.26;
const SHOOTING_RANGE_PLAYER_SPAWN: Vec3 = Vec3::new(-5.8, 1.0, -5.4);
const SHOOTING_RANGE_PLAYER_YAW_RAD: f32 = PI;
const SHOOTING_RANGE_GIMBAL_PITCH_RAD: f32 = -27.0 * PI / 180.0;
const VEHICLE_BODY_COLLIDER_HEIGHT: f32 = 0.231298;
const VEHICLE_BODY_COLLIDER_CENTER_Y: f32 = -0.115649;
const SHOOTING_RANGE_TARGET_GROUND_Y: f32 =
    -VEHICLE_BODY_COLLIDER_CENTER_Y + VEHICLE_BODY_COLLIDER_HEIGHT * 0.5;
const SHOOTING_RANGE_FLOOR_THICKNESS: f32 = 0.08;
const SHOOTING_RANGE_MIN_HALF_WIDTH: f32 = 10.0;
const SHOOTING_RANGE_MIN_HALF_DEPTH: f32 = 7.0;
const SHOOTING_RANGE_WALL_HEIGHT: f32 = 0.95;
const SHOOTING_RANGE_WALL_THICKNESS: f32 = 0.18;
const SHOOTING_RANGE_MARKING_HEIGHT: f32 = 0.012;
const SHOOTING_RANGE_GRID_SPACING: f32 = 2.0;
const SHOOTING_RANGE_BACKSTOP_HEIGHT: f32 = 2.4;
const SHOOTING_RANGE_BACKSTOP_THICKNESS: f32 = 0.12;
const SHOOTING_RANGE_INITIAL_LOCK_CONE_DEG: f32 = 14.0;
const SHOOTING_RANGE_SIDE_LANE_MIN_BEARING_DEG: f32 = 30.0;
const SHOOTING_RANGE_TARGET_DISTANCE_Z_OFFSET_M: f32 = 0.7;
const SHOOTING_RANGE_INACTIVE_TARGET_PARK_X: f32 = 42.0;
const SHOOTING_RANGE_INACTIVE_TARGET_PARK_Z: f32 = SHOOTING_RANGE_PLAYER_SPAWN.z - 45.0;
const ENEMY_ARMOR_TEST_SPAWNS: [Vec3; 0] = [];
const UPRIGHT_ARMOR_DEBUG_SPAWN: Vec3 = Vec3::new(-4.4, 1.0, -0.6);
const ENEMY_SECONDARY_TEST_SPAWN: Vec3 = Vec3::new(8.0, 1.0, 6.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoAimSceneMode {
    Armor,
    Energy,
    Outpost,
    ShootingRange,
}

impl AutoAimSceneMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Armor => "Normal Map",
            Self::Energy => "Energy Mechanism",
            Self::Outpost => "Outpost",
            Self::ShootingRange => "Shooting Range",
        }
    }
}

pub const fn scene_is_available_in_build(mode: AutoAimSceneMode) -> bool {
    !crate::distribution::is_contest_release()
        || matches!(
            mode,
            AutoAimSceneMode::Energy | AutoAimSceneMode::ShootingRange
        )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AimCaptureTargetKind {
    ArmorVehicle,
    ShootingRangeVehicle,
    PowerRune,
    Outpost,
}

fn aim_capture_mode_includes(mode: AutoAimSceneMode, kind: AimCaptureTargetKind) -> bool {
    matches!(
        (mode, kind),
        (AutoAimSceneMode::Armor, AimCaptureTargetKind::ArmorVehicle)
            | (
                AutoAimSceneMode::ShootingRange,
                AimCaptureTargetKind::ShootingRangeVehicle
            )
            | (AutoAimSceneMode::Energy, AimCaptureTargetKind::PowerRune)
            | (AutoAimSceneMode::Outpost, AimCaptureTargetKind::Outpost)
    )
}

fn should_mark_aim_capture_target(
    profile: CaptureSceneProfile,
    mode: AutoAimSceneMode,
    kind: AimCaptureTargetKind,
) -> bool {
    profile.is_aim() && aim_capture_mode_includes(mode, kind)
}

#[derive(Resource, Clone, Debug)]
pub struct AutoAimSceneState {
    pub current: AutoAimSceneMode,
    pub requested: AutoAimSceneMode,
    force_rebuild: bool,
    generation: u64,
}

impl AutoAimSceneState {
    pub fn new(mode: AutoAimSceneMode) -> Self {
        Self {
            current: mode,
            requested: mode,
            force_rebuild: false,
            generation: 1,
        }
    }

    pub fn request(&mut self, mode: AutoAimSceneMode) -> bool {
        if !scene_is_available_in_build(mode) {
            return false;
        }
        self.requested = mode;
        true
    }

    pub fn force_rebuild_current(&mut self) {
        self.requested = self.current;
        self.force_rebuild = true;
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_switch_pending(&self) -> bool {
        self.force_rebuild || self.current != self.requested
    }
}

#[derive(Component)]
pub(crate) struct SceneScoped;

fn scene_mode_from_str(mode: &str) -> AutoAimSceneMode {
    match mode.trim().to_ascii_lowercase().as_str() {
        "outpost" | "hit_outpost" => AutoAimSceneMode::Outpost,
        "energy" | "power_rune" | "power-rune" | "rune" | "buff" | "small_buff" | "small-buff"
        | "small_rune" | "small-rune" | "big_buff" | "big-buff" | "big_rune" | "big-rune" => {
            AutoAimSceneMode::Energy
        }
        "range" | "shooting_range" | "shooting-range" | "fps_range" | "fps-range" => {
            AutoAimSceneMode::ShootingRange
        }
        "normal" | "default" | "map" | "arena" | "armor" => AutoAimSceneMode::Armor,
        _ => AutoAimSceneMode::Armor,
    }
}

pub fn initial_auto_aim_scene_mode(config: &SimulationConfig) -> AutoAimSceneMode {
    let requested = scene_mode_from_env("DAEDALUS_SCENE_MODE")
        .or_else(|| scene_mode_from_env("DAEDALUS_AUTO_AIM_MODE"))
        .map(|mode| scene_mode_from_str(&mode))
        .unwrap_or_else(|| scene_mode_from_str(&config.auto_aim.bridge_mode));
    if scene_is_available_in_build(requested) {
        requested
    } else {
        // A contest launch always starts on a permitted map.  The client can
        // then select either permitted map through Scene Control.
        AutoAimSceneMode::ShootingRange
    }
}

fn scene_mode_from_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|mode| !mode.trim().is_empty())
}

fn local_test_spawn_for_mode(mode: AutoAimSceneMode) -> Vec3 {
    match mode {
        AutoAimSceneMode::Armor => LOCAL_TEST_SPAWN,
        AutoAimSceneMode::Energy => energy_test_spawn(),
        AutoAimSceneMode::Outpost => OUTPOST_TEST_SPAWN,
        AutoAimSceneMode::ShootingRange => SHOOTING_RANGE_PLAYER_SPAWN,
    }
}

fn local_gimbal_pitch_for_mode(mode: AutoAimSceneMode) -> f32 {
    match mode {
        AutoAimSceneMode::Armor => LOCAL_TEST_GIMBAL_PITCH_RAD,
        AutoAimSceneMode::Energy => energy_gimbal_pitch_rad(),
        AutoAimSceneMode::Outpost => OUTPOST_TEST_GIMBAL_PITCH_RAD,
        AutoAimSceneMode::ShootingRange => SHOOTING_RANGE_GIMBAL_PITCH_RAD,
    }
}

fn local_gimbal_yaw_for_mode(mode: AutoAimSceneMode) -> f32 {
    match mode {
        AutoAimSceneMode::Energy => energy_gimbal_yaw_rad(),
        AutoAimSceneMode::Armor | AutoAimSceneMode::Outpost | AutoAimSceneMode::ShootingRange => {
            0.0
        }
    }
}

fn spawn_vehicle_debug_targets(mode: AutoAimSceneMode) -> bool {
    mode == AutoAimSceneMode::Armor
}

fn local_test_yaw_for_mode(mode: AutoAimSceneMode) -> f32 {
    match mode {
        AutoAimSceneMode::ShootingRange => SHOOTING_RANGE_PLAYER_YAW_RAD,
        AutoAimSceneMode::Energy => energy_test_yaw_rad(),
        AutoAimSceneMode::Armor | AutoAimSceneMode::Outpost => PI,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ShootingRangeTargetSpawn {
    name: &'static str,
    asset: &'static str,
    center_position: Vec3,
    side_position: Vec3,
    yaw: f32,
    config: RobotConfig,
    kind: ShootingRangeTargetKind,
    active: bool,
}

const SHOOTING_RANGE_TARGETS: [ShootingRangeTargetSpawn; 2] = [
    ShootingRangeTargetSpawn {
        name: "range_target_armor_3",
        asset: "vehicle.glb",
        center_position: Vec3::new(-5.8, SHOOTING_RANGE_TARGET_GROUND_Y, 0.2),
        side_position: Vec3::new(
            -SHOOTING_RANGE_INACTIVE_TARGET_PARK_X,
            SHOOTING_RANGE_TARGET_GROUND_Y,
            SHOOTING_RANGE_INACTIVE_TARGET_PARK_Z,
        ),
        yaw: 0.55,
        config: INFANTRY_THREE_CONFIG,
        kind: ShootingRangeTargetKind::Armor3,
        active: true,
    },
    ShootingRangeTargetSpawn {
        name: "range_target_armor_1",
        asset: "HERO.glb",
        center_position: Vec3::new(-5.8, SHOOTING_RANGE_TARGET_GROUND_Y, 0.2),
        side_position: Vec3::new(
            SHOOTING_RANGE_INACTIVE_TARGET_PARK_X,
            SHOOTING_RANGE_TARGET_GROUND_Y,
            SHOOTING_RANGE_INACTIVE_TARGET_PARK_Z,
        ),
        yaw: -0.55,
        config: HERO_ROBOT_CONFIG,
        kind: ShootingRangeTargetKind::Armor1,
        active: false,
    },
];

fn shooting_range_target_spawns() -> &'static [ShootingRangeTargetSpawn] {
    &SHOOTING_RANGE_TARGETS
}

fn env_f32(key: &str) -> Option<f32> {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite())
}

fn env_angle_rad(rad_key: &str, deg_key: &str) -> Option<f32> {
    env_f32(rad_key).or_else(|| env_f32(deg_key).map(f32::to_radians))
}

fn energy_test_spawn() -> Vec3 {
    Vec3::new(
        env_f32("DAEDALUS_ENERGY_PLAYER_X").unwrap_or(ENERGY_TEST_SPAWN.x),
        env_f32("DAEDALUS_ENERGY_PLAYER_Y").unwrap_or(ENERGY_TEST_SPAWN.y),
        env_f32("DAEDALUS_ENERGY_PLAYER_Z").unwrap_or(ENERGY_TEST_SPAWN.z),
    )
}

fn spawn_energy_player_support(commands: &mut Commands) {
    let spawn = energy_test_spawn();
    let center = Vec3::new(
        spawn.x,
        ENERGY_TEST_GROUND_SURFACE_Y - ENERGY_TEST_SPAWN_SUPPORT_THICKNESS * 0.5,
        spawn.z,
    );
    commands.spawn((
        SceneScoped,
        Name::new("energy_player_spawn_support"),
        RigidBody::Static,
        Collider::cuboid(
            ENERGY_TEST_SPAWN_SUPPORT_WIDTH,
            ENERGY_TEST_SPAWN_SUPPORT_THICKNESS,
            ENERGY_TEST_SPAWN_SUPPORT_DEPTH,
        ),
        GameLayer::environment_collision_layers(),
        Friction::new(0.5),
        Restitution::ZERO,
        Transform::from_translation(center),
    ));
}

fn energy_test_yaw_rad() -> f32 {
    env_angle_rad(
        "DAEDALUS_ENERGY_PLAYER_YAW_RAD",
        "DAEDALUS_ENERGY_PLAYER_YAW_DEG",
    )
    .unwrap_or(ENERGY_TEST_YAW_RAD)
}

fn energy_gimbal_pitch_rad() -> f32 {
    env_angle_rad(
        "DAEDALUS_ENERGY_GIMBAL_PITCH_RAD",
        "DAEDALUS_ENERGY_GIMBAL_PITCH_DEG",
    )
    .unwrap_or(ENERGY_TEST_GIMBAL_PITCH_RAD)
}

fn energy_gimbal_yaw_rad() -> f32 {
    env_angle_rad(
        "DAEDALUS_ENERGY_GIMBAL_YAW_RAD",
        "DAEDALUS_ENERGY_GIMBAL_YAW_DEG",
    )
    .unwrap_or(ENERGY_TEST_GIMBAL_YAW_RAD)
}

fn shooting_range_target_position(target: &ShootingRangeTargetSpawn) -> Vec3 {
    if shooting_range_active_target_kind() != target.kind {
        return target.side_position;
    }

    if let Some(distance_m) = env_f32("DAEDALUS_RANGE_TARGET_DISTANCE_M") {
        return Vec3::new(
            target.center_position.x,
            target.center_position.y,
            shooting_range_target_z_for_distance(distance_m),
        );
    }

    if let Some(z) = env_f32("DAEDALUS_RANGE_CENTER_TARGET_Z") {
        return Vec3::new(target.center_position.x, target.center_position.y, z);
    }

    target.center_position
}

fn shooting_range_target_yaw_rad(target: &ShootingRangeTargetSpawn) -> f32 {
    let target_prefix = match target.kind {
        ShootingRangeTargetKind::Armor3 => "DAEDALUS_RANGE_TARGET3_",
        ShootingRangeTargetKind::Armor1 => "DAEDALUS_RANGE_TARGET1_",
    };
    env_angle_rad(
        &format!("{target_prefix}INITIAL_YAW_RAD"),
        &format!("{target_prefix}INITIAL_YAW_DEG"),
    )
    .or_else(|| {
        env_angle_rad(
            "DAEDALUS_RANGE_INITIAL_YAW_RAD",
            "DAEDALUS_RANGE_INITIAL_YAW_DEG",
        )
    })
    .unwrap_or(target.yaw)
}

fn shooting_range_target_z_for_distance(distance_m: f32) -> f32 {
    let distance_m = distance_m.clamp(0.5, 12.0);
    SHOOTING_RANGE_PLAYER_SPAWN.z + distance_m + SHOOTING_RANGE_TARGET_DISTANCE_Z_OFFSET_M
}

fn shooting_range_active_target_kind() -> ShootingRangeTargetKind {
    match std::env::var("DAEDALUS_RANGE_ACTIVE_TARGET_NUMBER")
        .ok()
        .as_deref()
        .map(str::trim)
    {
        Some("1") => ShootingRangeTargetKind::Armor1,
        _ => ShootingRangeTargetKind::Armor3,
    }
}

fn shooting_range_target_is_active(target: &ShootingRangeTargetSpawn) -> bool {
    if std::env::var("DAEDALUS_RANGE_ACTIVE_TARGET_NUMBER")
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return target.kind == shooting_range_active_target_kind();
    }

    target.active
}

fn shooting_range_target_bearing_deg(target: &ShootingRangeTargetSpawn) -> f32 {
    let delta = shooting_range_target_position(target) - SHOOTING_RANGE_PLAYER_SPAWN;
    delta.x.atan2(delta.z).to_degrees()
}

fn shooting_range_target_locked_axes() -> LockedAxes {
    LockedAxes::new()
        .lock_translation_y()
        .lock_rotation_x()
        .lock_rotation_z()
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ArenaBoundaryWall {
    name: &'static str,
    center: Vec3,
    size: Vec3,
}

fn arena_boundary_walls(config: &ArenaBoundaryConfig) -> Option<[ArenaBoundaryWall; 4]> {
    if !config.enabled {
        return None;
    }

    let half_width = config.half_width.max(0.5);
    let half_depth = config.half_depth.max(0.5);
    let height = config.height.max(0.5);
    let thickness = config.thickness.max(0.05);
    let y = height * 0.5;
    let x_length = half_width * 2.0 + thickness * 2.0;
    let z_length = half_depth * 2.0 + thickness * 2.0;

    Some([
        ArenaBoundaryWall {
            name: "arena_boundary_x_positive",
            center: Vec3::new(half_width + thickness * 0.5, y, 0.0),
            size: Vec3::new(thickness, height, z_length),
        },
        ArenaBoundaryWall {
            name: "arena_boundary_x_negative",
            center: Vec3::new(-half_width - thickness * 0.5, y, 0.0),
            size: Vec3::new(thickness, height, z_length),
        },
        ArenaBoundaryWall {
            name: "arena_boundary_z_positive",
            center: Vec3::new(0.0, y, half_depth + thickness * 0.5),
            size: Vec3::new(x_length, height, thickness),
        },
        ArenaBoundaryWall {
            name: "arena_boundary_z_negative",
            center: Vec3::new(0.0, y, -half_depth - thickness * 0.5),
            size: Vec3::new(x_length, height, thickness),
        },
    ])
}

fn spawn_arena_boundaries(commands: &mut Commands, config: &ArenaBoundaryConfig) {
    let Some(walls) = arena_boundary_walls(config) else {
        return;
    };

    let layer_env = GameLayer::environment_collision_layers();
    for wall in walls {
        commands.spawn((
            SceneScoped,
            Name::new(wall.name),
            ArenaBoundary,
            RigidBody::Static,
            Collider::cuboid(wall.size.x, wall.size.y, wall.size.z),
            layer_env,
            Friction::new(0.5),
            Restitution::ZERO,
            Transform::from_translation(wall.center),
        ));
    }
}

pub fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    config: Res<SimulationConfig>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    egui_global_settings: Option<ResMut<EguiGlobalSettings>>,
    scene_state: Res<AutoAimSceneState>,
) {
    if let Some(mut egui_global_settings) = egui_global_settings {
        egui_global_settings.auto_create_primary_context = false;
    }
    let capture_scene_profile = CaptureSceneProfile::from_env();
    commands.insert_resource(capture_scene_profile);
    spawn_auto_aim_scene(
        &mut commands,
        &asset_server,
        &config,
        &mut meshes,
        &mut materials,
        scene_state.current,
        capture_scene_profile,
    );
    if std::env::var("DAEDALUS_PERF_DISABLE_UI")
        .ok()
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(true)
    {
        spawn_text(&mut commands);
    }
    spawn_main_camera(&mut commands, &config);
}

fn spawn_auto_aim_scene(
    commands: &mut Commands,
    asset_server: &AssetServer,
    config: &SimulationConfig,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    scene_mode: AutoAimSceneMode,
    capture_scene_profile: CaptureSceneProfile,
) {
    if scene_mode == AutoAimSceneMode::ShootingRange {
        commands.insert_resource(ClearColor(Color::srgb(0.47, 0.50, 0.53)));
        commands.insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.92, 0.95, 1.0),
            brightness: 220.0,
            affects_lightmapped_meshes: true,
        });
    } else {
        commands.insert_resource(ClearColor(DEFAULT_CAPTURE_CLEAR_COLOR));
        commands.insert_resource(GlobalAmbientLight::default());
    }
    spawn_arena_boundaries(commands, &config.arena_boundary);
    let mut directional_light = commands.spawn((
        SceneScoped,
        DirectionalLight {
            color: Color::srgb(0.9, 0.95, 1.0),
            illuminance: config.render.illuminance,
            shadow_maps_enabled: config.render.shadows,
            contact_shadows_enabled: config.render.shadows,
            ..default()
        },
        Transform::from_xyz(0.0, 4.0, 0.0).looking_at(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)),
    ));
    if capture_scene_profile.is_aim() {
        directional_light.insert(aim_target_render_layers());
    }

    let layer_env = GameLayer::environment_collision_layers();

    let trimesh = || {
        ColliderConstructorHierarchy::new(ColliderConstructor::TrimeshFromMeshWithConfig(
            TrimeshFlags::all(),
        ))
        .with_default_layers(layer_env)
    };
    let voxel = |size| {
        ColliderConstructorHierarchy::new(ColliderConstructor::VoxelizedTrimeshFromMesh {
            voxel_size: size,
            fill_mode: FillMode::FloodFill {
                detect_cavities: true,
            },
        })
        .with_default_layers(layer_env)
    };

    if scene_mode == AutoAimSceneMode::ShootingRange {
        spawn_shooting_range_floor(commands, meshes, materials, config);
    } else {
        if scene_mode == AutoAimSceneMode::Energy {
            // Do not let the dynamic participant vehicle advance a physics tick
            // before the imported GROUND_DENSE collider has finished loading.
            spawn_energy_player_support(commands);
        }
        if capture_scene_profile.is_aim() {
            commands.spawn((
                SceneScoped,
                Name::new("Aim Capture Flat Ground"),
                RigidBody::Static,
                Collider::cuboid(100.0, 0.1, 100.0),
                layer_env,
                Friction::new(0.5),
                Transform::from_xyz(0.0, -0.05, 0.0),
            ));
        } else {
            commands.spawn((
                SceneScoped,
                WorldAssetRoot(
                    asset_server.load(GltfAssetLabel::Scene(0).from_asset("GROUND.glb")),
                ),
                Transform::IDENTITY,
                GroundRoot,
                Friction::new(0.5),
                PreciousCollision(HashMap::from([(
                    "GROUND_DENSE".to_string(),
                    (
                        trimesh(),
                        layer_env,
                        Visibility::Visible,
                        Some(RigidBody::Static),
                    ),
                )])),
            ));
        }

        if !capture_scene_profile.is_aim() && scene_mode != AutoAimSceneMode::Energy {
            commands.spawn((
                SceneScoped,
                WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("CALIB.glb"))),
                Transform::IDENTITY
                    .with_scale(Vec3::splat(1.0))
                    .with_translation(Vec3::new(1.0, 2.5, 1.0)),
            ));

            commands.spawn((
                SceneScoped,
                WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("CALIB.glb"))),
                Transform::IDENTITY
                    .with_scale(Vec3::splat(1.0))
                    .with_translation(Vec3::new(2.0, 0.5, 2.0)),
            ));
        }

        if !capture_scene_profile.is_aim() || scene_mode == AutoAimSceneMode::Outpost {
            let mut outpost = commands.spawn((
                SceneScoped,
                RigidBody::Static,
                WorldAssetRoot(
                    asset_server.load(GltfAssetLabel::Scene(0).from_asset("OUTPOST.glb")),
                ),
                Transform::IDENTITY,
                ScanOutpost,
            ));
            if should_mark_aim_capture_target(
                capture_scene_profile,
                scene_mode,
                AimCaptureTargetKind::Outpost,
            ) {
                outpost.insert(AimCaptureTargetCandidate);
            }
        }

        if !capture_scene_profile.is_aim() {
            commands.spawn((
                SceneScoped,
                WorldAssetRoot(
                    asset_server.load(GltfAssetLabel::Scene(0).from_asset("TECH_CORE.glb")),
                ),
                Transform::IDENTITY,
                TechCoreRoot,
                PreciousCollision(HashMap::from([(
                    "GROUND".to_string(),
                    (
                        trimesh(),
                        layer_env,
                        Visibility::Visible,
                        Some(RigidBody::Static),
                    ),
                )])),
            ));
        }

        if !capture_scene_profile.is_aim() || scene_mode == AutoAimSceneMode::Energy {
            let mut power_rune_col = HashMap::from([(
                "BASE".to_string(),
                (
                    trimesh(),
                    layer_env,
                    Visibility::Visible,
                    Some(RigidBody::Static),
                ),
            )]);
            for i in 1..=2 {
                for j in 1..=5 {
                    for k in ["ACTIVATED", "ACTIVE", "COMPLETED", "DISABLED"] {
                        power_rune_col.insert(
                            format!("FACE_{}_TARGET_{}_{}", i, j, k).to_string(),
                            (voxel(0.015), layer_env, Visibility::Visible, None),
                        );
                    }
                }
            }
            let mut power_rune = commands.spawn((
                SceneScoped,
                RigidBody::Static,
                CollisionMargin(0.001),
                Restitution::ZERO,
                WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("POWER.glb"))),
                Transform::IDENTITY,
                PowerRuneRoot,
                PreciousCollision(power_rune_col),
            ));
            if should_mark_aim_capture_target(
                capture_scene_profile,
                scene_mode,
                AimCaptureTargetKind::PowerRune,
            ) {
                power_rune.insert(AimCaptureTargetCandidate);
            }
        }
    }

    commands.spawn((
        SceneScoped,
        WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("vehicle.glb"))),
        Transform::from_translation(local_test_spawn_for_mode(scene_mode))
            .with_rotation(Quat::from_rotation_y(local_test_yaw_for_mode(scene_mode))),
        Infantry::new(Team::Red, INFANTRY_THREE_CONFIG),
        Controlled,
    ));

    if scene_mode == AutoAimSceneMode::ShootingRange {
        spawn_shooting_range_targets(commands, asset_server, capture_scene_profile);
    } else if spawn_vehicle_debug_targets(scene_mode) {
        for spawn in ENEMY_ARMOR_TEST_SPAWNS {
            let mut target = commands.spawn((
                SceneScoped,
                WorldAssetRoot(
                    asset_server.load(GltfAssetLabel::Scene(0).from_asset("vehicle.glb")),
                ),
                Transform::from_translation(spawn),
                Infantry::new(Team::Blue, INFANTRY_THREE_CONFIG),
                SlapperInfantry,
            ));
            if should_mark_aim_capture_target(
                capture_scene_profile,
                scene_mode,
                AimCaptureTargetKind::ArmorVehicle,
            ) {
                target.insert(AimCaptureTargetCandidate);
            }
        }

        let mut upright_target = commands.spawn((
            SceneScoped,
            WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("vehicle.glb"))),
            Transform::from_translation(UPRIGHT_ARMOR_DEBUG_SPAWN),
            Infantry::new(Team::Blue, INFANTRY_THREE_CONFIG),
            UprightArmorDebugTarget,
        ));
        if should_mark_aim_capture_target(
            capture_scene_profile,
            scene_mode,
            AimCaptureTargetKind::ArmorVehicle,
        ) {
            upright_target.insert(AimCaptureTargetCandidate);
        }

        if !capture_scene_profile.is_aim() {
            let mut secondary_target = commands.spawn((
                SceneScoped,
                WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("HERO.glb"))),
                Transform::from_translation(ENEMY_SECONDARY_TEST_SPAWN),
                Infantry::new(Team::Blue, HERO_ROBOT_CONFIG),
                SlapperInfantry,
                ActiveSlapper,
            ));
            if should_mark_aim_capture_target(
                capture_scene_profile,
                scene_mode,
                AimCaptureTargetKind::ArmorVehicle,
            ) {
                secondary_target.insert(AimCaptureTargetCandidate);
            }
        }
    }
}

fn spawn_main_camera(commands: &mut Commands, config: &SimulationConfig) {
    let mut main_camera = commands.spawn((
        Camera3d::default(),
        Exposure {
            ev100: FIXED_CAMERA_EV100,
        },
        Camera {
            // When Talos/ROS2 capture is enabled, the actual on-screen preview is a UI blit of the
            // off-screen capture texture. Keep this camera inactive to avoid rendering twice.
            #[cfg(any(feature = "ros2", feature = "talos"))]
            is_active: false,
            #[cfg(not(any(feature = "ros2", feature = "talos")))]
            is_active: config.preview.enabled,
            clear_color: ClearColorConfig::Custom(DEFAULT_CAPTURE_CLEAR_COLOR),
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: config.camera.fov.to_radians(),
            near: 0.1,
            far: 500000000.0,
            ..default()
        }),
        Tonemapping::None,
        Msaa::Off,
        Transform::from_xyz(0.0, 10.0, 15.0).looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
        MainCamera {
            follow_offset: Vec3::from_array(config.camera.follow_offset),
        },
    ));
    if config.render.main_camera_fxaa {
        main_camera.insert(Fxaa::default());
    }
    #[cfg(any(feature = "ros2", feature = "talos"))]
    let primary_egui_on_main_camera = !config.preview.enabled;
    #[cfg(not(any(feature = "ros2", feature = "talos")))]
    let primary_egui_on_main_camera = true;
    if primary_egui_on_main_camera {
        main_camera.insert(PrimaryEguiContext);
    }
    #[cfg(any(feature = "ros2", feature = "talos"))]
    main_camera.insert(crate::capture::CaptureSource);
}

pub fn apply_auto_aim_scene_mode_request(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    config: Res<SimulationConfig>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut scene_state: ResMut<AutoAimSceneState>,
    scene_entities: Query<Entity, With<SceneScoped>>,
    projectiles: Query<Entity, With<Projectile>>,
    mut stats: ResMut<ProjectileStatistics>,
    capture_scene_profile: Option<Res<CaptureSceneProfile>>,
) {
    if !scene_state.is_switch_pending() {
        return;
    }

    let next_mode = scene_state.requested;
    for entity in &scene_entities {
        commands.entity(entity).try_despawn();
    }
    for projectile in &projectiles {
        commands.entity(projectile).try_despawn();
    }

    *stats = ProjectileStatistics::default();
    scene_state.current = next_mode;
    scene_state.force_rebuild = false;
    scene_state.generation = scene_state.generation.saturating_add(1);
    spawn_auto_aim_scene(
        &mut commands,
        &asset_server,
        &config,
        &mut meshes,
        &mut materials,
        next_mode,
        capture_scene_profile
            .as_deref()
            .copied()
            .unwrap_or_else(CaptureSceneProfile::from_env),
    );
    info!("Switched simulation scene to {}.", next_mode.label());
}

fn spawn_shooting_range_floor(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    config: &SimulationConfig,
) {
    let half_width = config
        .arena_boundary
        .half_width
        .max(SHOOTING_RANGE_MIN_HALF_WIDTH);
    let half_depth = config
        .arena_boundary
        .half_depth
        .max(SHOOTING_RANGE_MIN_HALF_DEPTH);
    let floor_size = Vec3::new(
        half_width * 2.0,
        SHOOTING_RANGE_FLOOR_THICKNESS,
        half_depth * 2.0,
    );
    spawn_shooting_range_box(
        commands,
        meshes,
        materials,
        "shooting_range_ground_plate",
        Vec3::new(0.0, -SHOOTING_RANGE_FLOOR_THICKNESS * 0.5, 0.0),
        floor_size,
        Color::srgb(0.39, 0.39, 0.36),
        true,
    );

    let mark_y = SHOOTING_RANGE_MARKING_HEIGHT * 0.5 + 0.002;
    let lane_color = Color::srgb(0.84, 0.84, 0.80);
    let grid_color = Color::srgb(0.26, 0.27, 0.25);
    let muted_blue = Color::srgb(0.04, 0.20, 0.74);
    let muted_orange = Color::srgb(0.82, 0.36, 0.06);
    let neutral_panel = Color::srgb(0.18, 0.20, 0.21);

    let mut x = -half_width + SHOOTING_RANGE_GRID_SPACING;
    while x < half_width {
        spawn_shooting_range_box(
            commands,
            meshes,
            materials,
            "shooting_range_floor_grid_x",
            Vec3::new(x, mark_y, 0.0),
            Vec3::new(0.025, SHOOTING_RANGE_MARKING_HEIGHT, half_depth * 2.0 - 0.9),
            grid_color,
            false,
        );
        x += SHOOTING_RANGE_GRID_SPACING;
    }

    let mut z = -half_depth + SHOOTING_RANGE_GRID_SPACING;
    while z < half_depth {
        spawn_shooting_range_box(
            commands,
            meshes,
            materials,
            "shooting_range_floor_grid_z",
            Vec3::new(0.0, mark_y, z),
            Vec3::new(half_width * 2.0 - 0.9, SHOOTING_RANGE_MARKING_HEIGHT, 0.025),
            grid_color,
            false,
        );
        z += SHOOTING_RANGE_GRID_SPACING;
    }

    for x in [-half_width * 0.5, 0.0, half_width * 0.5] {
        spawn_shooting_range_box(
            commands,
            meshes,
            materials,
            "shooting_range_lane_line",
            Vec3::new(x, mark_y, 0.0),
            Vec3::new(0.06, SHOOTING_RANGE_MARKING_HEIGHT, half_depth * 2.0 - 0.9),
            lane_color,
            false,
        );
    }

    for z in [-5.4, -2.2, 0.2, 3.6, 6.6] {
        spawn_shooting_range_box(
            commands,
            meshes,
            materials,
            "shooting_range_cross_line",
            Vec3::new(0.0, mark_y, z),
            Vec3::new(half_width * 2.0 - 1.2, SHOOTING_RANGE_MARKING_HEIGHT, 0.05),
            lane_color,
            false,
        );
    }

    for (name, z, color) in [
        ("shooting_range_red_sector", -half_depth + 0.9, muted_orange),
        ("shooting_range_blue_sector", half_depth - 0.9, muted_blue),
    ] {
        spawn_shooting_range_box(
            commands,
            meshes,
            materials,
            name,
            Vec3::new(0.0, mark_y + 0.001, z),
            Vec3::new(half_width * 2.0 - 1.4, SHOOTING_RANGE_MARKING_HEIGHT, 0.22),
            color,
            false,
        );
    }

    spawn_shooting_range_box(
        commands,
        meshes,
        materials,
        "shooting_range_player_pad",
        Vec3::new(
            SHOOTING_RANGE_PLAYER_SPAWN.x,
            mark_y + 0.003,
            SHOOTING_RANGE_PLAYER_SPAWN.z,
        ),
        Vec3::new(1.5, SHOOTING_RANGE_MARKING_HEIGHT, 1.1),
        muted_orange,
        false,
    );

    for target in shooting_range_target_spawns() {
        let position = shooting_range_target_position(target);
        spawn_shooting_range_box(
            commands,
            meshes,
            materials,
            "shooting_range_target_pad",
            Vec3::new(position.x, mark_y + 0.004, position.z),
            Vec3::new(1.25, SHOOTING_RANGE_MARKING_HEIGHT, 0.95),
            muted_blue,
            false,
        );
    }

    spawn_shooting_range_backstop(
        commands,
        meshes,
        materials,
        half_width,
        half_depth,
        neutral_panel,
        muted_blue,
        muted_orange,
    );
    spawn_shooting_range_visual_walls(commands, meshes, materials, half_width, half_depth);
}

fn spawn_shooting_range_box(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    name: &'static str,
    center: Vec3,
    size: Vec3,
    color: Color,
    collider: bool,
) {
    let mut entity = commands.spawn((
        SceneScoped,
        Name::new(name),
        Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: color,
            perceptual_roughness: 0.88,
            metallic: 0.02,
            unlit: true,
            ..default()
        })),
        Transform::from_translation(center),
    ));
    if collider {
        entity.insert((
            RigidBody::Static,
            Collider::cuboid(size.x, size.y, size.z),
            GameLayer::environment_collision_layers(),
            Friction::new(0.65),
            Restitution::ZERO,
        ));
    }
}

fn spawn_shooting_range_backstop(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    half_width: f32,
    half_depth: f32,
    panel_color: Color,
    target_color: Color,
    accent_color: Color,
) {
    let z = half_depth - 0.35;
    let y = SHOOTING_RANGE_BACKSTOP_HEIGHT * 0.5;
    let width = half_width * 2.0 - 1.1;
    spawn_shooting_range_box(
        commands,
        meshes,
        materials,
        "shooting_range_backstop_panel",
        Vec3::new(0.0, y, z),
        Vec3::new(
            width,
            SHOOTING_RANGE_BACKSTOP_HEIGHT,
            SHOOTING_RANGE_BACKSTOP_THICKNESS,
        ),
        panel_color,
        false,
    );

    spawn_shooting_range_box(
        commands,
        meshes,
        materials,
        "shooting_range_backstop_top_stripe",
        Vec3::new(0.0, SHOOTING_RANGE_BACKSTOP_HEIGHT - 0.18, z - 0.07),
        Vec3::new(width - 0.4, 0.08, 0.035),
        accent_color,
        false,
    );

    for target in shooting_range_target_spawns() {
        let position = shooting_range_target_position(target);
        if position.z <= 0.0 {
            continue;
        }
        spawn_shooting_range_box(
            commands,
            meshes,
            materials,
            "shooting_range_backstop_target_marker",
            Vec3::new(position.x, 1.05, z - 0.08),
            Vec3::new(1.15, 0.72, 0.035),
            target_color,
            false,
        );
    }
}

fn spawn_shooting_range_visual_walls(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    half_width: f32,
    half_depth: f32,
) {
    let wall_color = Color::srgb(0.16, 0.17, 0.18);
    let y = SHOOTING_RANGE_WALL_HEIGHT * 0.5;
    let x_length = half_width * 2.0 + SHOOTING_RANGE_WALL_THICKNESS * 2.0;
    let z_length = half_depth * 2.0 + SHOOTING_RANGE_WALL_THICKNESS * 2.0;
    let walls = [
        (
            "shooting_range_wall_x_positive",
            Vec3::new(half_width + SHOOTING_RANGE_WALL_THICKNESS * 0.5, y, 0.0),
            Vec3::new(
                SHOOTING_RANGE_WALL_THICKNESS,
                SHOOTING_RANGE_WALL_HEIGHT,
                z_length,
            ),
        ),
        (
            "shooting_range_wall_x_negative",
            Vec3::new(-half_width - SHOOTING_RANGE_WALL_THICKNESS * 0.5, y, 0.0),
            Vec3::new(
                SHOOTING_RANGE_WALL_THICKNESS,
                SHOOTING_RANGE_WALL_HEIGHT,
                z_length,
            ),
        ),
        (
            "shooting_range_wall_z_positive",
            Vec3::new(0.0, y, half_depth + SHOOTING_RANGE_WALL_THICKNESS * 0.5),
            Vec3::new(
                x_length,
                SHOOTING_RANGE_WALL_HEIGHT,
                SHOOTING_RANGE_WALL_THICKNESS,
            ),
        ),
        (
            "shooting_range_wall_z_negative",
            Vec3::new(0.0, y, -half_depth - SHOOTING_RANGE_WALL_THICKNESS * 0.5),
            Vec3::new(
                x_length,
                SHOOTING_RANGE_WALL_HEIGHT,
                SHOOTING_RANGE_WALL_THICKNESS,
            ),
        ),
    ];

    for (name, center, size) in walls {
        spawn_shooting_range_box(
            commands, meshes, materials, name, center, size, wall_color, false,
        );
    }
}

fn spawn_shooting_range_targets(
    commands: &mut Commands,
    asset_server: &AssetServer,
    capture_scene_profile: CaptureSceneProfile,
) {
    for (index, target) in shooting_range_target_spawns().iter().enumerate() {
        let position = shooting_range_target_position(target);
        let mut entity = commands.spawn((
            SceneScoped,
            Name::new(target.name),
            WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(target.asset))),
            Transform::from_translation(position)
                .with_rotation(Quat::from_rotation_y(shooting_range_target_yaw_rad(target))),
            Infantry::new(Team::Blue, target.config),
            SlapperInfantry,
            ShootingRangeTarget {
                control_index: index,
                kind: target.kind,
                origin: position,
                applied_geometry_scale: f32::NAN,
            },
        ));
        if shooting_range_target_is_active(target) {
            entity.insert(ActiveSlapper);
        }
        if should_mark_aim_capture_target(
            capture_scene_profile,
            AutoAimSceneMode::ShootingRange,
            AimCaptureTargetKind::ShootingRangeVehicle,
        ) {
            entity.insert(AimCaptureTargetCandidate);
        }
    }
}

pub fn setup_ground(
    events: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    name: Query<&Name>,
    ground: Query<Entity, With<ScanOutpost>>,
    capture_targets: Query<(), With<AimCaptureTargetCandidate>>,
) {
    let root = events.entity;
    if !ground.iter().any(|entity| entity == root) {
        return;
    }
    if capture_targets.contains(root) {
        let descendants: Vec<_> = children.iter_descendants(root).collect();
        mark_aim_capture_target(&mut commands, root, descendants);
    }
    children.iter_descendants(root).for_each(|e| {
        let Ok(name) = name.get(e) else {
            return;
        };
        if name.as_str() == "OUTPOST_1" {
            commands.entity(e).insert(OutpostRoot::new(Team::Red));
        }
        if name.as_str() == "OUTPOST_2" {
            commands.entity(e).insert(OutpostRoot::new(Team::Blue));
        }
    })
}

pub fn setup_dart_launch(
    events: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    name: Query<&Name>,
    ground: Query<(), With<GroundRoot>>,
) {
    if ground.get(events.entity).is_err() {
        return;
    }

    for entity in children.iter_descendants(events.entity) {
        let Ok(name) = name.get(entity) else {
            continue;
        };
        if name.as_str() == "DART_LAUNCH_DIRECTION" {
            commands.entity(entity).insert(DartLaunch);
            return;
        }
    }

    warn!("GROUND.glb is missing DART_LAUNCH_DIRECTION");
}

pub fn setup_vehicle(
    events: On<WorldInstanceReady>,
    mut commands: Commands,
    query: HierarchyQuery,
    root_query: Query<(
        Entity,
        &Infantry,
        Option<&Controlled>,
        Option<&ActiveSlapper>,
        Option<&UprightArmorDebugTarget>,
        Option<&ShootingRangeTarget>,
        Option<&AimCaptureTargetCandidate>,
    )>,
    mut transforms: Query<&mut Transform>,
    _secondary_query: Query<&ChildOf, (Without<Infantry>, Without<WorldInstance>)>,
    _node_query: Query<(&Name, &ChildOf), (Without<Infantry>, Without<WorldInstance>)>,
    sim_config: Res<SimulationConfig>,
    scene_state: Res<AutoAimSceneState>,
) {
    let root = events.entity;
    if root_query.get(root).is_err() {
        return;
    }
    let (
        root,
        infantry,
        is_local,
        is_active,
        is_upright_debug_target,
        is_range_target,
        is_capture_target,
    ) = root_query.get(root).unwrap();
    let team = infantry.team;
    let config = infantry.config;
    let is_local = is_local.is_some();
    let is_active = is_active.is_some();
    let is_upright_debug_target = is_upright_debug_target.is_some();
    let is_range_target = is_range_target.is_some();
    if is_capture_target.is_some() {
        let descendants: Vec<_> = query.children.iter_descendants(root).collect();
        mark_aim_capture_target(&mut commands, root, descendants);
    }
    if is_local {
        query.children.iter_descendants(root).for_each(|e| {
            commands.entity(e).insert(Controlled);
        });
    } else {
        query.children.iter_descendants(root).for_each(|e| {
            commands.entity(e).insert(SlapperInfantry);
            if is_active {
                commands.entity(e).insert(ActiveSlapper);
            }
        });
    }
    let vehicle_body_collision_layers = GameLayer::vehicle_body_collision_layers(is_local);
    let vehicle_armor_collision_layers = GameLayer::vehicle_armor_collision_layers(is_local);

    let body_collider = Collider::compound(vec![(
        Vec3::new(0.0, VEHICLE_BODY_COLLIDER_CENTER_Y, 0.0),
        Quat::IDENTITY,
        Collider::cylinder(0.2593615, VEHICLE_BODY_COLLIDER_HEIGHT),
    )]);
    if is_range_target {
        commands.entity(root).insert((
            // Shooting-range motion is prescribed by the acceptance harness.
            // A kinematic body keeps projectile/body collisions while preventing
            // contacts from displacing the target away from its declared path.
            RigidBody::Kinematic,
            shooting_range_target_locked_axes(),
            VehicleDynamic::new(
                sim_config.vehicle.max_speed,
                sim_config.vehicle.linear_acceleration,
                sim_config.vehicle.acceleration_exponent,
            ),
            body_collider,
            CollisionMargin(0.005),
            vehicle_body_collision_layers,
            Mass(15.0),
            Restitution::new(0.01),
            AngularDamping(0.0),
            LinearVelocity::ZERO,
            AngularVelocity::ZERO,
        ));
    } else if is_upright_debug_target {
        commands.entity(root).insert((
            RigidBody::Dynamic,
            LockedAxes::ROTATION_LOCKED,
            body_collider,
            CollisionMargin(0.005),
            vehicle_body_collision_layers,
            Mass(15.0),
            Restitution::new(0.01),
            AngularDamping(50.0),
        ));
    } else {
        let mut root_entity = commands.entity(root);
        root_entity.insert((
            RigidBody::Dynamic,
            VehicleDynamic::new(
                sim_config.vehicle.max_speed,
                sim_config.vehicle.linear_acceleration,
                sim_config.vehicle.acceleration_exponent,
            ),
            body_collider,
            CollisionMargin(0.005),
            vehicle_body_collision_layers,
            Mass(15.0),
            Restitution::new(0.01),
            AngularDamping(50.0),
        ));
        if is_local && scene_state.current == AutoAimSceneMode::Energy {
            // The participant vehicle is spawned beside the energy mechanism,
            // not as a physics obstacle. Lock root roll/pitch/yaw so a small
            // terrain contact cannot overturn it; Q/E operates the chassis
            // child and the gimbal remains fully controllable.
            root_entity.insert(LockedAxes::ROTATION_LOCKED);
        }
    }

    query.children.iter_descendants(root).for_each(|e| {
        commands.entity(e).insert(vehicle_armor_collision_layers);
    });

    let iter = query.of(root).any().exact("VEHICLE").flatten();
    let base = iter.clone().exact("BASE").one().unwrap();
    commands.entity(base).insert((
        InfantryChassis::default(),
        ScanArmor::new(team, config.armor),
    ));
    let gimbal = iter.exact("GIMBAL").one().unwrap();
    let initial_gimbal = if is_local {
        InfantryGimbal {
            local_yaw: local_gimbal_yaw_for_mode(scene_state.current),
            pitch: local_gimbal_pitch_for_mode(scene_state.current),
        }
    } else {
        InfantryGimbal::default()
    };
    if let Ok(mut gimbal_transform) = transforms.get_mut(gimbal) {
        gimbal_transform.rotation = Quat::from_euler(
            EulerRot::YXZ,
            initial_gimbal.local_yaw,
            initial_gimbal.pitch,
            0.0,
        );
    }
    commands.entity(gimbal).insert(initial_gimbal);
    if is_local {
        let q = query.of(gimbal).flatten();
        commands
            .entity(q.clone().exact("SHOT_DIRECTION").one().unwrap())
            .insert(InfantryLaunchOffset);
        commands
            .entity(q.exact("CAM_DIRECTION").one().unwrap())
            .insert(InfantryViewOffset);
    }
}

pub fn setup_collision(
    events: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    name: Query<&Name, With<Children>>,
    root_query: Query<(Entity, &PreciousCollision)>,
) {
    let Ok((_, PreciousCollision(map))) = root_query.get(events.entity) else {
        return;
    };
    for e in children.iter_descendants(events.entity) {
        let Ok(name) = name.get(e) else {
            continue;
        };
        if let Some((constructor, layer, visibility, rigid)) = map.get(&name.to_string()) {
            if let Some(rigid) = rigid {
                commands
                    .entity(e)
                    .insert((*rigid, constructor.clone(), *layer));
            } else {
                commands.entity(e).insert((constructor.clone(), *layer));
            }
            if visibility == &Visibility::Hidden {
                commands.entity(e).insert(*visibility);
            }
        }
    }
    commands.entity(events.entity).remove::<PreciousCollision>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aim_capture_scene_mapping_is_explicit_for_every_supported_mode() {
        let all_kinds = [
            AimCaptureTargetKind::ArmorVehicle,
            AimCaptureTargetKind::ShootingRangeVehicle,
            AimCaptureTargetKind::PowerRune,
            AimCaptureTargetKind::Outpost,
        ];
        let cases = [
            (AutoAimSceneMode::Armor, AimCaptureTargetKind::ArmorVehicle),
            (
                AutoAimSceneMode::ShootingRange,
                AimCaptureTargetKind::ShootingRangeVehicle,
            ),
            (AutoAimSceneMode::Energy, AimCaptureTargetKind::PowerRune),
            (AutoAimSceneMode::Outpost, AimCaptureTargetKind::Outpost),
        ];

        for (mode, expected_kind) in cases {
            for kind in all_kinds {
                assert_eq!(
                    should_mark_aim_capture_target(CaptureSceneProfile::Aim, mode, kind),
                    kind == expected_kind,
                    "unexpected aim mapping for {mode:?}/{kind:?}"
                );
                assert!(!should_mark_aim_capture_target(
                    CaptureSceneProfile::Full,
                    mode,
                    kind
                ));
            }
        }
    }

    #[test]
    fn arena_boundary_walls_surround_configured_field() {
        let config = ArenaBoundaryConfig {
            enabled: true,
            half_width: 14.0,
            half_depth: 7.5,
            height: 4.0,
            thickness: 0.5,
        };

        let walls = arena_boundary_walls(&config).unwrap();

        assert_eq!(walls.len(), 4);
        assert_eq!(walls[0].center, Vec3::new(14.25, 2.0, 0.0));
        assert_eq!(walls[0].size, Vec3::new(0.5, 4.0, 16.0));
        assert_eq!(walls[2].center, Vec3::new(0.0, 2.0, 7.75));
        assert_eq!(walls[2].size, Vec3::new(29.0, 4.0, 0.5));
    }

    #[test]
    fn disabled_arena_boundary_spawns_no_walls() {
        let config = ArenaBoundaryConfig {
            enabled: false,
            ..default()
        };

        assert!(arena_boundary_walls(&config).is_none());
    }

    #[test]
    fn outpost_scene_mode_isolates_vehicle_debug_targets() {
        let mode = scene_mode_from_str("outpost");

        assert_eq!(mode, AutoAimSceneMode::Outpost);
        assert!(!spawn_vehicle_debug_targets(mode));
        assert_eq!(local_test_spawn_for_mode(mode), OUTPOST_TEST_SPAWN);
        assert_eq!(
            local_gimbal_pitch_for_mode(mode),
            OUTPOST_TEST_GIMBAL_PITCH_RAD
        );
    }

    #[test]
    fn armor_scene_mode_keeps_vehicle_debug_targets() {
        let mode = scene_mode_from_str("armor");

        assert_eq!(mode, AutoAimSceneMode::Armor);
        assert!(spawn_vehicle_debug_targets(mode));
        assert_eq!(local_test_spawn_for_mode(mode), LOCAL_TEST_SPAWN);
        assert_eq!(
            local_gimbal_pitch_for_mode(mode),
            LOCAL_TEST_GIMBAL_PITCH_RAD
        );
    }

    #[test]
    fn energy_scene_mode_clears_vehicle_debug_targets() {
        for alias in [
            "energy",
            "power_rune",
            "power-rune",
            "rune",
            "buff",
            "small_buff",
            "big_buff",
        ] {
            let mode = scene_mode_from_str(alias);

            assert_eq!(mode, AutoAimSceneMode::Energy);
            assert!(!spawn_vehicle_debug_targets(mode));
            assert_eq!(local_test_spawn_for_mode(mode), ENERGY_TEST_SPAWN);
            assert_eq!(
                local_gimbal_pitch_for_mode(mode),
                ENERGY_TEST_GIMBAL_PITCH_RAD
            );
            assert_eq!(local_gimbal_yaw_for_mode(mode), ENERGY_TEST_GIMBAL_YAW_RAD);
            assert_eq!(local_test_yaw_for_mode(mode), ENERGY_TEST_YAW_RAD);
        }
    }

    #[test]
    fn energy_player_starts_on_flat_apron_with_range_camera_pitch() {
        let mode = AutoAimSceneMode::Energy;

        assert_eq!(local_test_spawn_for_mode(mode), Vec3::new(-4.0, 0.0, -5.6));
        assert_eq!(local_test_yaw_for_mode(mode), PI);
        assert_eq!(
            local_gimbal_pitch_for_mode(mode),
            SHOOTING_RANGE_GIMBAL_PITCH_RAD
        );
        assert_eq!(ENERGY_TEST_GROUND_SURFACE_Y, -0.245_722_3);
        assert!(ENERGY_TEST_SPAWN_SUPPORT_WIDTH >= VEHICLE_BODY_COLLIDER_HEIGHT * 4.0);
        assert!(ENERGY_TEST_SPAWN_SUPPORT_DEPTH >= VEHICLE_BODY_COLLIDER_HEIGHT * 4.0);
    }

    #[test]
    fn shooting_range_scene_mode_uses_range_layout() {
        let mode = scene_mode_from_str("shooting_range");

        assert_eq!(mode, AutoAimSceneMode::ShootingRange);
        assert_eq!(scene_mode_from_str("normal"), AutoAimSceneMode::Armor);
        assert!(!spawn_vehicle_debug_targets(mode));
        assert_eq!(local_test_spawn_for_mode(mode), SHOOTING_RANGE_PLAYER_SPAWN);
        assert_eq!(
            local_gimbal_pitch_for_mode(mode),
            SHOOTING_RANGE_GIMBAL_PITCH_RAD
        );
        assert_eq!(local_test_yaw_for_mode(mode), SHOOTING_RANGE_PLAYER_YAW_RAD);
        assert_eq!(shooting_range_target_spawns().len(), 2);
        assert_eq!(
            shooting_range_target_spawns()
                .iter()
                .filter(|target| target.active)
                .count(),
            1
        );
        assert!(
            shooting_range_target_spawns()
                .iter()
                .any(|target| target.kind == ShootingRangeTargetKind::Armor3
                    && target.config == INFANTRY_THREE_CONFIG)
        );
        assert!(
            shooting_range_target_spawns()
                .iter()
                .any(|target| target.kind == ShootingRangeTargetKind::Armor1
                    && target.config == HERO_ROBOT_CONFIG)
        );
        assert!(
            shooting_range_target_spawns()
                .iter()
                .all(
                    |target| target.center_position.y == SHOOTING_RANGE_TARGET_GROUND_Y
                        && target.side_position.y == SHOOTING_RANGE_TARGET_GROUND_Y
                )
        );
        assert!(SHOOTING_RANGE_TARGET_GROUND_Y < SHOOTING_RANGE_PLAYER_SPAWN.y);
        let initial_lock_targets: Vec<_> = shooting_range_target_spawns()
            .iter()
            .filter(|target| {
                shooting_range_target_bearing_deg(target).abs()
                    <= SHOOTING_RANGE_INITIAL_LOCK_CONE_DEG
            })
            .collect();
        assert_eq!(initial_lock_targets.len(), 1);
        assert!(initial_lock_targets[0].active);
        assert!(
            shooting_range_target_spawns()
                .iter()
                .filter(|target| !target.active)
                .all(|target| shooting_range_target_bearing_deg(target).abs()
                    >= SHOOTING_RANGE_SIDE_LANE_MIN_BEARING_DEG)
        );
        assert!(
            shooting_range_target_spawns()
                .iter()
                .filter(|target| !target.active)
                .all(|target| shooting_range_target_position(target).z
                    < SHOOTING_RANGE_PLAYER_SPAWN.z)
        );
        assert!(
            shooting_range_target_spawns()
                .iter()
                .filter(|target| !target.active)
                .all(|target| target.side_position.x.abs()
                    >= SHOOTING_RANGE_INACTIVE_TARGET_PARK_X
                    && target.side_position.z <= SHOOTING_RANGE_PLAYER_SPAWN.z - 40.0)
        );
        let target_axes = shooting_range_target_locked_axes();
        assert!(target_axes.is_translation_y_locked());
        assert!(!target_axes.is_translation_x_locked());
        assert!(!target_axes.is_translation_z_locked());
        assert!(target_axes.is_rotation_x_locked());
        assert!(!target_axes.is_rotation_y_locked());
        assert!(target_axes.is_rotation_z_locked());
    }

    #[cfg(feature = "contest-release")]
    #[test]
    fn contest_build_exposes_only_range_and_energy() {
        assert!(scene_is_available_in_build(AutoAimSceneMode::ShootingRange));
        assert!(scene_is_available_in_build(AutoAimSceneMode::Energy));
        assert!(!scene_is_available_in_build(AutoAimSceneMode::Armor));
        assert!(!scene_is_available_in_build(AutoAimSceneMode::Outpost));

        let mut state = AutoAimSceneState::new(AutoAimSceneMode::ShootingRange);
        assert!(!state.request(AutoAimSceneMode::Armor));
        assert_eq!(state.requested, AutoAimSceneMode::ShootingRange);
        assert!(state.request(AutoAimSceneMode::Energy));
        assert_eq!(state.requested, AutoAimSceneMode::Energy);
    }
}
