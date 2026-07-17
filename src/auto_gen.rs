//! Automatic dataset generation mode
//! Usage: cargo run -- --auto-gen

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::{Extract, RenderApp};

use crate::capture::driver::{CaptureConfig, CapturedFrameKind};
use crate::capture::{
    CameraFov, CaptureBundle, CaptureSource, IMAGE_HEIGHT, IMAGE_WIDTH, ImageHandle,
    setup_capture_camera, sync_capture_camera,
};
use crate::components::Infantry;
use crate::dataset::prelude::{DatasetPlugin, DatasetSnapshotCreator, capture};
use crate::robomaster::prelude::*;
use crate::setup::{AutoAimSceneMode, AutoAimSceneState};

// ==================== Configuration Parameters ====================
const DIST_MIN: f32 = 2.0;
const DIST_MAX: f32 = 8.0;
const DIST_STEP: f32 = 0.5;

const YAW_MIN: f32 = -std::f32::consts::PI; // -180°
const YAW_MAX: f32 = std::f32::consts::PI; // 180°
const YAW_STEP: f32 = 0.5236; // 30° in radians

const PITCH_MIN: f32 = -0.7854; // -45° in radians
const PITCH_MAX: f32 = 0.7854; // 45° in radians
const PITCH_STEP: f32 = 0.2618; // 15° in radians

const HEIGHT_OFFSET: f32 = 0.5;
const ENERGY_CAMERA_HEIGHT: f32 = 1.1;
const ENERGY_CAMERA_HEIGHT_MIN: f32 = 0.75;
const ENERGY_CAMERA_HEIGHT_MAX: f32 = 1.65;
const ENERGY_HEIGHT_GAIN: f32 = 0.35;
const ENERGY_PITCH_MIN: f32 = -0.25;
const ENERGY_PITCH_MAX: f32 = 0.25;
const ENERGY_PITCH_STEP: f32 = 0.25;
const ENERGY_ACTIVE_TARGET_COUNT: usize = 5;
const SETTLE_FRAMES: u32 = 5;
const FOV: f32 = 45.0;
const ENERGY_TARGET_CENTER: Vec3 = Vec3::new(0.0, 2.45, 0.0);
// =================================================

#[derive(Component)]
struct AutoGenTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutoGenTargetKind {
    Armor,
    Energy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuneModeFilter {
    Small,
    Large,
    Both,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuneTeamFilter {
    Red,
    Blue,
    Both,
}

#[derive(Resource, Clone)]
struct AutoGenState {
    target_kind: AutoGenTargetKind,
    distances: Vec<f32>,
    yaws: Vec<f32>,
    pitches: Vec<f32>,
    state_sets: Vec<RuneStateSet>,
    pose_jitter: PoseJitter,
    d_idx: usize,
    y_idx: usize,
    p_idx: usize,
    t_idx: usize,
    settle_frames: u32,
    settle_counter: u32,
    frame_count: usize,
    capturing: bool,
    exit_delay_counter: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
struct PoseJitter {
    distance: f32,
    yaw: f32,
    pitch: f32,
    camera_height: f32,
    seed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuneStateSet {
    pending: Vec<usize>,
    activated: Vec<usize>,
}

pub struct AutoGenPlugin;

impl Plugin for AutoGenPlugin {
    fn build(&self, app: &mut App) {
        let scene_mode = match auto_gen_target_kind() {
            AutoGenTargetKind::Armor => AutoAimSceneMode::Armor,
            AutoGenTargetKind::Energy => AutoAimSceneMode::Energy,
        };
        let capture_config = CaptureConfig {
            width: IMAGE_WIDTH,
            height: IMAGE_HEIGHT,
            texture_format: TextureFormat::Bgra8UnormSrgb,
            frame_kind: CapturedFrameKind::Rgb8,
        };

        let capture_bundle = CaptureBundle::color_and_depth(
            app,
            capture_config.clone(),
            vec![Box::new(DatasetSnapshotCreator::default())],
            vec![Box::new(DatasetSnapshotCreator::depth())],
        );
        let image_handle = capture_bundle.color_target().unwrap().clone();

        app.add_plugins(capture_bundle)
            .add_plugins(DatasetPlugin)
            .insert_resource(AutoAimSceneState::new(scene_mode))
            .insert_resource(ImageHandle(image_handle))
            .insert_resource(CameraFov(FOV.to_radians()))
            .insert_resource(capture_config)
            .add_systems(Startup, (setup_auto_gen, setup_capture_camera))
            .add_systems(
                Update,
                (auto_gen_loop, sync_capture_camera.after(auto_gen_loop)),
            );

        // Add capture system to RenderApp's ExtractSchedule
        app.sub_app_mut(RenderApp)
            .add_systems(ExtractSchedule, write_flag)
            .insert_resource(ShouldCapture(true, usize::MAX))
            .add_systems(ExtractSchedule, capture_condition);
    }
}
fn capture_condition(world: &mut World) {
    let mut res = world.resource_mut::<ShouldCapture>();
    if res.0 {
        res.0 = false;
        world.run_system_once(capture).unwrap();
    }
}

fn auto_gen_target_kind() -> AutoGenTargetKind {
    match std::env::var("DAEDALUS_AUTO_GEN_TARGET")
        .ok()
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("energy" | "rune" | "power_rune" | "power-rune" | "buff") => AutoGenTargetKind::Energy,
        _ => AutoGenTargetKind::Armor,
    }
}

fn env_f32(key: &str, default: f32) -> f32 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite())
        .unwrap_or(default)
}

fn env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn auto_gen_pose_jitter() -> PoseJitter {
    PoseJitter {
        distance: env_f32("DAEDALUS_AUTO_GEN_DIST_JITTER", 0.0).max(0.0),
        yaw: env_f32("DAEDALUS_AUTO_GEN_YAW_JITTER", 0.0).max(0.0),
        pitch: env_f32("DAEDALUS_AUTO_GEN_PITCH_JITTER", 0.0).max(0.0),
        camera_height: env_f32("DAEDALUS_AUTO_GEN_CAMERA_HEIGHT_JITTER", 0.0).max(0.0),
        seed: env_u64("DAEDALUS_AUTO_GEN_SEED", 20260708),
    }
}

fn auto_gen_rune_mode_filter() -> RuneModeFilter {
    match std::env::var("DAEDALUS_RUNE_MODE")
        .ok()
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("small" | "small_buff" | "small-buff" | "small_rune" | "small-rune") => {
            RuneModeFilter::Small
        }
        Some(
            "large" | "big" | "big_buff" | "big-buff" | "large_rune" | "large-rune" | "big_rune"
            | "big-rune",
        ) => RuneModeFilter::Large,
        Some("closed" | "close" | "inactive" | "off") => RuneModeFilter::Closed,
        _ => RuneModeFilter::Both,
    }
}

fn auto_gen_rune_team_filter() -> RuneTeamFilter {
    match std::env::var("DAEDALUS_RUNE_TEAM")
        .ok()
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("red" | "r") => RuneTeamFilter::Red,
        Some("blue" | "b") => RuneTeamFilter::Blue,
        _ => RuneTeamFilter::Both,
    }
}

fn parse_rune_target_indices(raw: &str) -> Vec<usize> {
    let mut targets = raw
        .split(',')
        .filter_map(|part| part.trim().parse::<usize>().ok())
        .filter(|target| *target < ENERGY_ACTIVE_TARGET_COUNT)
        .collect::<Vec<_>>();
    targets.sort_unstable();
    targets.dedup();
    targets
}

fn rune_state_set(mut pending: Vec<usize>, mut activated: Vec<usize>) -> RuneStateSet {
    pending.sort_unstable();
    pending.dedup();
    activated.sort_unstable();
    activated.dedup();
    activated.retain(|target| !pending.contains(target));
    RuneStateSet { pending, activated }
}

fn all_rune_indices() -> Vec<usize> {
    (0..ENERGY_ACTIVE_TARGET_COUNT).collect()
}

fn parse_rune_target_sets(raw: &str, mode_filter: RuneModeFilter) -> Vec<Vec<usize>> {
    let expected_count = match mode_filter {
        RuneModeFilter::Large => 2,
        RuneModeFilter::Small | RuneModeFilter::Both | RuneModeFilter::Closed => 1,
    };
    let mut sets = raw
        .split(';')
        .map(parse_rune_target_indices)
        .filter(|targets| targets.len() >= expected_count)
        .map(|targets| targets.into_iter().take(expected_count).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    sets.sort_unstable();
    sets.dedup();
    sets
}

fn parse_rune_state_sets(raw: &str, mode_filter: RuneModeFilter) -> Vec<RuneStateSet> {
    let pending_limit = match mode_filter {
        RuneModeFilter::Large => 2,
        RuneModeFilter::Small | RuneModeFilter::Both | RuneModeFilter::Closed => 1,
    };
    let mut sets = raw
        .split(';')
        .filter_map(|part| {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                return None;
            }
            let (pending_raw, activated_raw) = trimmed.split_once('|').unwrap_or((trimmed, ""));
            let mut pending = parse_rune_target_indices(pending_raw);
            pending.truncate(pending_limit);
            if pending.is_empty() {
                return None;
            }
            Some(rune_state_set(
                pending,
                parse_rune_target_indices(activated_raw),
            ))
        })
        .collect::<Vec<_>>();
    sets.sort_unstable_by(|left, right| {
        left.pending
            .cmp(&right.pending)
            .then_with(|| left.activated.cmp(&right.activated))
    });
    sets.dedup();
    sets
}

fn singleton_target_sets(targets: &[usize]) -> Vec<Vec<usize>> {
    targets.iter().map(|target| vec![*target]).collect()
}

fn pair_target_sets(targets: &[usize]) -> Vec<Vec<usize>> {
    let mut pairs = Vec::new();
    for (i, first) in targets.iter().enumerate() {
        for second in targets.iter().skip(i + 1) {
            pairs.push(vec![*first, *second]);
        }
    }
    pairs
}

fn pending_only_state_sets(mode_filter: RuneModeFilter, targets: &[usize]) -> Vec<RuneStateSet> {
    match mode_filter {
        RuneModeFilter::Small | RuneModeFilter::Both | RuneModeFilter::Closed => {
            singleton_target_sets(targets)
                .into_iter()
                .map(|pending| rune_state_set(pending, Vec::new()))
                .collect()
        }
        RuneModeFilter::Large => {
            let pending_sets = if targets.len() == 1 {
                let primary = targets[0] % ENERGY_ACTIVE_TARGET_COUNT;
                vec![vec![primary, (primary + 1) % ENERGY_ACTIVE_TARGET_COUNT]]
            } else {
                pair_target_sets(targets)
            };
            pending_sets
                .into_iter()
                .map(|pending| rune_state_set(pending, Vec::new()))
                .collect()
        }
    }
}

fn subsets(values: &[usize], min_len: usize, max_len: usize) -> Vec<Vec<usize>> {
    let mut result = Vec::new();
    let total = 1usize << values.len();
    for mask in 0..total {
        let mut subset = Vec::new();
        for (idx, value) in values.iter().enumerate() {
            if (mask & (1usize << idx)) != 0 {
                subset.push(*value);
            }
        }
        if subset.len() >= min_len && subset.len() <= max_len {
            result.push(subset);
        }
    }
    result
}

fn official_state_sets_for_mode(
    mode_filter: RuneModeFilter,
    targets: &[usize],
) -> Vec<RuneStateSet> {
    let all_targets = all_rune_indices();
    let mut states = Vec::new();
    match mode_filter {
        RuneModeFilter::Small | RuneModeFilter::Both | RuneModeFilter::Closed => {
            for pending in singleton_target_sets(targets) {
                let available = all_targets
                    .iter()
                    .copied()
                    .filter(|target| !pending.contains(target))
                    .collect::<Vec<_>>();
                for activated in subsets(&available, 0, available.len()) {
                    states.push(rune_state_set(pending.clone(), activated));
                }
            }
        }
        RuneModeFilter::Large => {
            for pending in pair_target_sets(targets) {
                let available = all_targets
                    .iter()
                    .copied()
                    .filter(|target| !pending.contains(target))
                    .collect::<Vec<_>>();
                for activated in subsets(&available, 0, available.len()) {
                    states.push(rune_state_set(pending.clone(), activated));
                }
            }
            for pending in singleton_target_sets(targets) {
                let available = all_targets
                    .iter()
                    .copied()
                    .filter(|target| !pending.contains(target))
                    .collect::<Vec<_>>();
                for activated in subsets(&available, 1, available.len()) {
                    states.push(rune_state_set(pending.clone(), activated));
                }
            }
        }
    }
    states.sort_unstable_by(|left, right| {
        left.pending
            .cmp(&right.pending)
            .then_with(|| left.activated.cmp(&right.activated))
    });
    states.dedup();
    states
}

fn auto_gen_state_sets(
    target_kind: AutoGenTargetKind,
    mode_filter: RuneModeFilter,
) -> Vec<RuneStateSet> {
    if target_kind != AutoGenTargetKind::Energy {
        return vec![rune_state_set(vec![0], Vec::new())];
    }

    if let Ok(raw) = std::env::var("DAEDALUS_AUTO_GEN_RUNE_STATE_SETS") {
        let sets = parse_rune_state_sets(&raw, mode_filter);
        if !sets.is_empty() {
            return sets;
        }
    }

    if let Ok(raw) = std::env::var("DAEDALUS_AUTO_GEN_RUNE_TARGET_SETS") {
        let sets = parse_rune_target_sets(&raw, mode_filter);
        if !sets.is_empty() {
            return sets
                .into_iter()
                .map(|pending| rune_state_set(pending, Vec::new()))
                .collect();
        }
    }

    let state_sweep = std::env::var("DAEDALUS_AUTO_GEN_RUNE_STATE_SWEEP")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let Some(raw) = std::env::var("DAEDALUS_AUTO_GEN_RUNE_TARGETS").ok() else {
        let targets = all_rune_indices();
        return if matches!(state_sweep.as_str(), "official" | "all" | "three-state") {
            official_state_sets_for_mode(mode_filter, &targets)
        } else {
            pending_only_state_sets(mode_filter, &targets)
        };
    };

    let targets = if raw.trim().eq_ignore_ascii_case("all") {
        all_rune_indices()
    } else {
        let parsed = parse_rune_target_indices(&raw);
        if parsed.is_empty() {
            all_rune_indices()
        } else {
            parsed
        }
    };

    if matches!(state_sweep.as_str(), "official" | "all" | "three-state") {
        official_state_sets_for_mode(mode_filter, &targets)
    } else {
        pending_only_state_sets(mode_filter, &targets)
    }
}

fn auto_gen_ranges(target_kind: AutoGenTargetKind) -> (Vec<f32>, Vec<f32>, Vec<f32>, u32) {
    let (default_yaw_min, default_yaw_max, default_yaw_step) = match target_kind {
        AutoGenTargetKind::Armor => (YAW_MIN, YAW_MAX, YAW_STEP),
        AutoGenTargetKind::Energy => (YAW_MIN, YAW_MAX, std::f32::consts::PI / 8.0),
    };
    let (default_pitch_min, default_pitch_max, default_pitch_step) = match target_kind {
        AutoGenTargetKind::Armor => (PITCH_MIN, PITCH_MAX, PITCH_STEP),
        AutoGenTargetKind::Energy => (ENERGY_PITCH_MIN, ENERGY_PITCH_MAX, ENERGY_PITCH_STEP),
    };
    let dist_min = env_f32("DAEDALUS_AUTO_GEN_DIST_MIN", DIST_MIN);
    let dist_max = env_f32("DAEDALUS_AUTO_GEN_DIST_MAX", DIST_MAX);
    let dist_step = env_f32("DAEDALUS_AUTO_GEN_DIST_STEP", DIST_STEP).max(0.001);
    let yaw_min = env_f32("DAEDALUS_AUTO_GEN_YAW_MIN", default_yaw_min);
    let yaw_max = env_f32("DAEDALUS_AUTO_GEN_YAW_MAX", default_yaw_max);
    let yaw_step = env_f32("DAEDALUS_AUTO_GEN_YAW_STEP", default_yaw_step).max(0.001);
    let pitch_min = env_f32("DAEDALUS_AUTO_GEN_PITCH_MIN", default_pitch_min);
    let pitch_max = env_f32("DAEDALUS_AUTO_GEN_PITCH_MAX", default_pitch_max);
    let pitch_step = env_f32("DAEDALUS_AUTO_GEN_PITCH_STEP", default_pitch_step).max(0.001);
    let settle_frames = env_u32("DAEDALUS_AUTO_GEN_SETTLE_FRAMES", SETTLE_FRAMES);

    (
        gen_range(dist_min, dist_max, dist_step),
        gen_range(yaw_min, yaw_max, yaw_step),
        gen_range(pitch_min, pitch_max, pitch_step),
        settle_frames,
    )
}

fn limit_state_sets(mut state_sets: Vec<RuneStateSet>) -> Vec<RuneStateSet> {
    let max_state_sets = env_u32("DAEDALUS_AUTO_GEN_RUNE_MAX_STATE_SETS", 0) as usize;
    if max_state_sets == 0 || state_sets.len() <= max_state_sets {
        return state_sets;
    }

    let seed = env_u64("DAEDALUS_AUTO_GEN_SEED", 20260708);
    let mut keyed = state_sets
        .drain(..)
        .enumerate()
        .map(|(idx, state)| {
            (
                splitmix64(seed ^ (idx as u64).wrapping_mul(0x517C_C1B7_2722_0A95)),
                state,
            )
        })
        .collect::<Vec<_>>();
    keyed.sort_unstable_by_key(|(key, _)| *key);
    let mut limited = keyed
        .into_iter()
        .take(max_state_sets)
        .map(|(_, state)| state)
        .collect::<Vec<_>>();
    limited.sort_unstable_by(|left, right| {
        left.pending
            .cmp(&right.pending)
            .then_with(|| left.activated.cmp(&right.activated))
    });
    limited
}

fn setup_auto_gen(mut commands: Commands, asset_server: Res<AssetServer>) {
    let target_kind = auto_gen_target_kind();
    let mode_filter = auto_gen_rune_mode_filter();
    let (distances, yaws, pitches, settle_frames) = auto_gen_ranges(target_kind);
    let state_sets = limit_state_sets(auto_gen_state_sets(target_kind, mode_filter));
    let pose_jitter = auto_gen_pose_jitter();
    info!(
        "Config: target {:?}, poses {} distances x {} yaws x {} pitches x {} rune state sets, settle {} frames",
        target_kind,
        distances.len(),
        yaws.len(),
        pitches.len(),
        state_sets.len(),
        settle_frames
    );
    if pose_jitter.distance > 0.0
        || pose_jitter.yaw > 0.0
        || pose_jitter.pitch > 0.0
        || pose_jitter.camera_height > 0.0
    {
        info!(
            "Pose jitter: distance +/-{:.2}m, yaw +/-{:.1}deg, pitch +/-{:.1}deg, camera height +/-{:.2}m, seed {}",
            pose_jitter.distance,
            pose_jitter.yaw.to_degrees(),
            pose_jitter.pitch.to_degrees(),
            pose_jitter.camera_height,
            pose_jitter.seed
        );
    }

    commands.insert_resource(ClearColor(Color::srgb(0.47, 0.50, 0.53)));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.92, 0.95, 1.0),
        brightness: env_f32("DAEDALUS_AUTO_GEN_AMBIENT", 250.0),
        affects_lightmapped_meshes: true,
    });
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.9, 0.95, 1.0),
            illuminance: env_f32("DAEDALUS_AUTO_GEN_ILLUMINANCE", 1200.0),
            shadow_maps_enabled: false,
            contact_shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(1.5, 4.0, 2.5).looking_at(Vec3::ZERO, Vec3::Y),
        Name::new("AutoGenKeyLight"),
    ));

    // Create ground
    commands.spawn((
        WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("GROUND.glb"))),
        Transform::IDENTITY,
    ));

    match target_kind {
        AutoGenTargetKind::Armor => {
            commands.spawn((
                WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("HERO.glb"))),
                Transform::from_xyz(0.0, 1.0, 0.0),
                Infantry::new(Team::Blue, HERO_ROBOT_CONFIG),
                ScanArmor::new(Team::Blue, HERO_ROBOT_CONFIG.armor),
                AutoGenTarget,
            ));
        }
        AutoGenTargetKind::Energy => {
            commands.spawn((
                WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("POWER.glb"))),
                Transform::IDENTITY,
                PowerRuneRoot,
            ));
            commands.spawn((
                Transform::from_translation(ENERGY_TARGET_CENTER),
                AutoGenTarget,
                Name::new("AutoGenEnergyTargetCenter"),
            ));
        }
    }

    // Create the transform source copied by the offscreen capture camera.
    commands.spawn((
        Transform::from_xyz(3.0, 2.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
        CaptureSource,
        Name::new("AutoGenCamera"),
    ));

    let total = distances.len() * yaws.len() * pitches.len() * state_sets.len();

    info!("Total poses to capture: {}", total);

    commands.insert_resource(AutoGenState {
        target_kind,
        distances,
        yaws,
        pitches,
        state_sets,
        pose_jitter,
        d_idx: 0,
        y_idx: 0,
        p_idx: 0,
        t_idx: 0,
        settle_frames,
        settle_counter: 0,
        frame_count: 0,
        capturing: false,
        exit_delay_counter: None,
    });
}

fn auto_gen_loop(
    _commands: Commands,
    mut state: ResMut<AutoGenState>,
    mut camera: Single<&mut Transform, With<CaptureSource>>,
    target: Single<&GlobalTransform, With<AutoGenTarget>>,
    mut rune_queries: ParamSet<(
        Query<(&PowerRune, &GlobalTransform), Without<AutoGenTarget>>,
        Query<(
            &mut PowerRune,
            &mut PowerRuneMechanism,
            &mut PowerRuneRotation,
        )>,
    )>,
) {
    // Wait for screenshot to complete
    if state.capturing {
        state.capturing = false;
        next_pose(&mut state);

        return;
    }

    // Wait for camera to settle
    if state.settle_counter > 0 {
        if state.target_kind == AutoGenTargetKind::Energy {
            let rune_state = state.state_sets[state.t_idx].clone();
            apply_auto_gen_rune_state(&rune_state, &mut rune_queries.p1());
        }
        state.settle_counter -= 1;
        if state.settle_counter == 0 {
            // Capture frame - trigger the capture system
            state.capturing = true;
        }
        return;
    }

    // Check if complete
    if state.d_idx >= state.distances.len() {
        let counter = state
            .exit_delay_counter
            .get_or_insert_with(|| env_u32("DAEDALUS_AUTO_GEN_EXIT_DELAY_FRAMES", 30));
        if *counter == 0 {
            info!("=== Dataset Generation Complete! ===");
            info!("Total frames captured: {}", state.frame_count);
            std::process::exit(0);
        }
        *counter -= 1;
        return;
    }

    // Move camera to next pose
    let dist = state.distances[state.d_idx];
    let yaw = state.yaws[state.y_idx];
    let pitch = state.pitches[state.p_idx];
    let pose_index = current_pose_index(&state);
    let (dist, yaw, pitch, camera_height_offset) =
        jittered_pose(dist, yaw, pitch, state.pose_jitter, pose_index);
    let rune_state = state.state_sets[state.t_idx].clone();

    let target_pos = match state.target_kind {
        AutoGenTargetKind::Armor => target.translation(),
        AutoGenTargetKind::Energy => {
            let rune_transforms = rune_queries.p0();
            energy_focus_point(&rune_transforms).unwrap_or_else(|| target.translation())
        }
    };
    let look_at = place_auto_gen_camera(
        &mut camera,
        state.target_kind,
        target_pos,
        dist,
        yaw,
        pitch,
        camera_height_offset,
    );
    if state.target_kind == AutoGenTargetKind::Energy {
        apply_auto_gen_rune_state(&rune_state, &mut rune_queries.p1());
    }

    let done = ((state.d_idx * state.yaws.len() + state.y_idx) * state.pitches.len() + state.p_idx)
        * state.state_sets.len()
        + state.t_idx;
    let total =
        state.distances.len() * state.yaws.len() * state.pitches.len() * state.state_sets.len();

    if state.frame_count % 10 == 0
        || (state.d_idx == 0 && state.y_idx == 0 && state.p_idx == 0)
        || done == total - 1
    {
        info!(
            "Progress: {}/{} (dist={:.1}, yaw={:.1}°, pitch={:.1}°)",
            done + 1,
            total,
            dist,
            yaw.to_degrees(),
            pitch.to_degrees()
        );
        if state.target_kind == AutoGenTargetKind::Energy {
            info!(
                "Rune state: pending={:?}, activated={:?}",
                rune_state.pending, rune_state.activated
            );
        }
        if std::env::var_os("DAEDALUS_DATASET_DEBUG").is_some() {
            info!(
                "Auto-gen camera: pos=({:.2},{:.2},{:.2}) look_at=({:.2},{:.2},{:.2}) target={:?}",
                camera.translation.x,
                camera.translation.y,
                camera.translation.z,
                look_at.x,
                look_at.y,
                look_at.z,
                state.target_kind
            );
        }
    }

    state.settle_counter = state.settle_frames;
    if state.settle_counter == 0 {
        state.capturing = true;
    }
}

fn energy_focus_point(
    runes: &Query<(&PowerRune, &GlobalTransform), Without<AutoGenTarget>>,
) -> Option<Vec3> {
    let filter = auto_gen_rune_mode_filter();
    let team_filter = auto_gen_rune_team_filter();
    let mut count = 0usize;
    let mut sum = Vec3::ZERO;
    for (rune, transform) in runes.iter() {
        if filter == RuneModeFilter::Closed {
            continue;
        }
        if !rune_team_enabled(team_filter, rune.team()) {
            continue;
        }
        sum += transform.translation();
        count += 1;
    }

    (count > 0).then_some(sum / count as f32)
}

fn place_auto_gen_camera(
    camera: &mut Transform,
    target_kind: AutoGenTargetKind,
    target_pos: Vec3,
    dist: f32,
    yaw: f32,
    pitch: f32,
    camera_height_offset: f32,
) -> Vec3 {
    match target_kind {
        AutoGenTargetKind::Armor => {
            let x = dist * yaw.cos() * pitch.cos();
            let y = dist * pitch.sin() + HEIGHT_OFFSET;
            let z = dist * yaw.sin() * pitch.cos();
            camera.translation = target_pos + Vec3::new(x, y, z);
            camera.look_at(target_pos, Vec3::Y);
            target_pos
        }
        AutoGenTargetKind::Energy => {
            let base_height = env_f32("DAEDALUS_AUTO_GEN_CAMERA_HEIGHT", ENERGY_CAMERA_HEIGHT);
            let min_height = env_f32(
                "DAEDALUS_AUTO_GEN_CAMERA_HEIGHT_MIN",
                ENERGY_CAMERA_HEIGHT_MIN,
            );
            let max_height = env_f32(
                "DAEDALUS_AUTO_GEN_CAMERA_HEIGHT_MAX",
                ENERGY_CAMERA_HEIGHT_MAX,
            );
            let height_gain = env_f32("DAEDALUS_AUTO_GEN_CAMERA_HEIGHT_GAIN", ENERGY_HEIGHT_GAIN);
            let aim_y_offset = env_f32("DAEDALUS_AUTO_GEN_AIM_Y_OFFSET", 0.0);
            let camera_height =
                (base_height + dist * pitch.sin() * height_gain + camera_height_offset)
                    .clamp(min_height, max_height);
            camera.translation = Vec3::new(
                target_pos.x + dist * yaw.cos(),
                camera_height,
                target_pos.z + dist * yaw.sin(),
            );
            let look_at = target_pos + Vec3::Y * aim_y_offset;
            camera.look_at(look_at, Vec3::Y);
            look_at
        }
    }
}

fn rune_mode_enabled(filter: RuneModeFilter, mode: RuneMode) -> bool {
    match filter {
        RuneModeFilter::Small => mode == RuneMode::Small,
        RuneModeFilter::Large => mode == RuneMode::Large,
        RuneModeFilter::Both => true,
        RuneModeFilter::Closed => false,
    }
}

fn rune_team_enabled(filter: RuneTeamFilter, team: Team) -> bool {
    match filter {
        RuneTeamFilter::Red => team == Team::Red,
        RuneTeamFilter::Blue => team == Team::Blue,
        RuneTeamFilter::Both => true,
    }
}

fn selected_mode_for_filter(filter: RuneModeFilter, current_mode: RuneMode) -> Option<RuneMode> {
    match filter {
        RuneModeFilter::Small => Some(RuneMode::Small),
        RuneModeFilter::Large => Some(RuneMode::Large),
        RuneModeFilter::Both => Some(current_mode),
        RuneModeFilter::Closed => None,
    }
}

fn apply_auto_gen_rune_state(
    rune_state: &RuneStateSet,
    runes: &mut Query<(
        &mut PowerRune,
        &mut PowerRuneMechanism,
        &mut PowerRuneRotation,
    )>,
) {
    let filter = auto_gen_rune_mode_filter();
    let team_filter = auto_gen_rune_team_filter();
    let mut rng = rand::thread_rng();
    for (mut rune, mut mechanism, mut rotation) in runes.iter_mut() {
        if let Some(mode) = selected_mode_for_filter(filter, rune.mode())
            && rune_team_enabled(team_filter, rune.team())
        {
            rune.set_mode(mode);
            *mechanism.state_mut() = MechanismState::forced_activation_state(
                mode,
                &rune_state.pending,
                &rune_state.activated,
            );
            rotation.sync_activation(mode, true, &mut rng);
        } else {
            *mechanism.state_mut() = MechanismState::inactive(rune.mode());
            rotation.sync_activation(rune.mode(), false, &mut rng);
        }
    }
}

fn write_flag(q: Extract<Res<AutoGenState>>, mut r: ResMut<ShouldCapture>) {
    if q.capturing && q.frame_count != r.1 {
        r.0 = true;
        r.1 = q.frame_count;
    } else {
        r.0 = false;
    }
}

#[derive(Resource)]
struct ShouldCapture(bool, usize);

fn next_pose(state: &mut AutoGenState) {
    state.frame_count += 1;
    state.t_idx += 1;
    if state.t_idx >= state.state_sets.len() {
        state.t_idx = 0;
        state.p_idx += 1;
        if state.p_idx >= state.pitches.len() {
            state.p_idx = 0;
            state.y_idx += 1;
            if state.y_idx >= state.yaws.len() {
                state.y_idx = 0;
                state.d_idx += 1;
            }
        }
    }
}

fn current_pose_index(state: &AutoGenState) -> usize {
    ((state.d_idx * state.yaws.len() + state.y_idx) * state.pitches.len() + state.p_idx)
        * state.state_sets.len()
        + state.t_idx
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = value;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn jitter_component(seed: u64, pose_index: usize, salt: u64, amplitude: f32) -> f32 {
    if amplitude <= 0.0 {
        return 0.0;
    }
    let mixed = splitmix64(seed ^ (pose_index as u64).wrapping_mul(0xD1B5_4A32_D192_ED03) ^ salt);
    let unit = ((mixed >> 11) as f64) * (1.0 / ((1u64 << 53) as f64));
    ((unit as f32) * 2.0 - 1.0) * amplitude
}

fn jittered_pose(
    distance: f32,
    yaw: f32,
    pitch: f32,
    jitter: PoseJitter,
    pose_index: usize,
) -> (f32, f32, f32, f32) {
    (
        (distance + jitter_component(jitter.seed, pose_index, 0xA17C_E001, jitter.distance))
            .max(0.5),
        yaw + jitter_component(jitter.seed, pose_index, 0xA17C_E002, jitter.yaw),
        pitch + jitter_component(jitter.seed, pose_index, 0xA17C_E003, jitter.pitch),
        jitter_component(jitter.seed, pose_index, 0xA17C_E004, jitter.camera_height),
    )
}

fn gen_range(min: f32, max: f32, step: f32) -> Vec<f32> {
    let mut v = Vec::new();
    let mut x = min;
    while x <= max + 0.0001 {
        v.push(x);
        x += step;
    }
    v
}
