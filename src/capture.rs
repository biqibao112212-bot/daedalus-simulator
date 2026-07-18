pub mod depth;
pub mod driver;
pub mod view_copy;

use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::cluster::ClusterConfig;
use bevy::prelude::*;
use bevy_inspector_egui::bevy_egui::PrimaryEguiContext;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub use driver::CaptureBundle;

#[derive(Component)]
pub struct CaptureSource;

#[derive(Component)]
pub struct CaptureCamera;

/// One wall-clock timestamp shared by the prescribed scene pose and every
/// capture payload extracted from that pose.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct ExposureWallTimestamp {
    pub timestamp_ns: u64,
}

pub fn advance_exposure_wall_timestamp(mut stamp: ResMut<ExposureWallTimestamp>) {
    stamp.timestamp_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
        .max(1);
}

/// Marks one scene root that is expected to remain visible to the target-only
/// capture profile. The diagnostic counter counts roots, not every GLTF node.
#[derive(Component, Clone, Copy, Debug)]
pub struct AimCaptureTarget;

/// Spawn-time classification for a target GLTF root. It becomes an
/// [`AimCaptureTarget`] only after `WorldInstanceReady`, when every renderable
/// descendant can be assigned the aim layer safely.
#[derive(Component, Clone, Copy, Debug)]
pub struct AimCaptureTargetCandidate;

#[derive(Resource, Deref, Clone)]
pub struct ImageHandle(pub Handle<Image>);

#[derive(Resource, Clone, Copy)]
pub struct CameraFov(pub f32);

pub const FULL_SCENE_RENDER_LAYER: usize = 0;
pub const AIM_CAPTURE_RENDER_LAYER: usize = 1;

/// Selects which scene geometry the off-screen auto-aim capture cameras render.
///
/// `Full` preserves the historical layer-0 behavior. `Aim` is an explicit
/// performance profile whose cameras see only target entities tagged with the
/// dedicated aim layer.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CaptureSceneProfile {
    #[default]
    Full,
    Aim,
}

impl CaptureSceneProfile {
    pub fn from_value(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("aim") => Self::Aim,
            Some("full") | None => Self::Full,
            // Unknown values fail safe to the historical full scene.
            Some(_) => Self::Full,
        }
    }

    pub fn from_env() -> Self {
        Self::from_value(
            std::env::var("DAEDALUS_CAPTURE_SCENE_PROFILE")
                .ok()
                .as_deref(),
        )
    }

    pub const fn is_aim(self) -> bool {
        matches!(self, Self::Aim)
    }
}

pub fn ensure_capture_scene_profile(world: &mut World) -> CaptureSceneProfile {
    if let Some(profile) = world.get_resource::<CaptureSceneProfile>() {
        return *profile;
    }

    let profile = CaptureSceneProfile::from_env();
    world.insert_resource(profile);
    profile
}

/// Target renderables keep layer 0 for a full-scene camera and add layer 1 for
/// the target-only capture camera.
pub fn aim_target_render_layers() -> RenderLayers {
    RenderLayers::from_layers(&[FULL_SCENE_RENDER_LAYER, AIM_CAPTURE_RENDER_LAYER])
}

/// Adds the aim layer to a target root and all of its already-instantiated GLTF
/// descendants. Render layers are per render entity and do not inherit from a
/// parent, so tagging only `root` would produce an empty capture.
pub fn mark_aim_capture_target(
    commands: &mut Commands,
    root: Entity,
    descendants: impl IntoIterator<Item = Entity>,
) {
    let layers = aim_target_render_layers();
    commands
        .entity(root)
        .insert((AimCaptureTarget, layers.clone()));
    for entity in descendants {
        commands.entity(entity).insert(layers.clone());
    }
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaptureSceneDiagnostics {
    pub tagged_target_roots: usize,
}

/// Controls the optional depth prepass for the off-screen capture camera.
#[derive(Resource, Clone, Copy, Debug)]
pub struct CaptureDepthPrepassEnabled(pub bool);

impl Default for CaptureDepthPrepassEnabled {
    fn default() -> Self {
        Self(true)
    }
}

pub const CAPTURE_CAMERA_ORDER: isize = -100;
pub const DEFAULT_CAPTURE_CLEAR_COLOR: Color = Color::srgb(0.47, 0.50, 0.53);
static PREVIEW_PRESENT_TOTAL: AtomicU64 = AtomicU64::new(0);

pub fn preview_present_total() -> u64 {
    PREVIEW_PRESENT_TOTAL.load(Ordering::Relaxed)
}

pub fn setup_capture_camera(world: &mut World) {
    let capture_camera_exists = {
        let mut query = world.query_filtered::<Entity, With<CaptureCamera>>();
        query.iter(world).next().is_some()
    };
    if capture_camera_exists {
        return;
    }

    let scene_profile = ensure_capture_scene_profile(world);
    if !world.contains_resource::<CaptureSceneDiagnostics>() {
        world.insert_resource(CaptureSceneDiagnostics::default());
    }
    let render_target_handle = world.resource::<ImageHandle>().0.clone();
    let fov = world.resource::<CameraFov>().0;
    let depth_prepass_enabled = world
        .get_resource::<CaptureDepthPrepassEnabled>()
        .copied()
        .unwrap_or_default()
        .0;

    let mut capture_camera = world.spawn((
        Camera3d::default(),
        Tonemapping::None,
        RenderTarget::Image(render_target_handle.into()),
        Camera {
            order: CAPTURE_CAMERA_ORDER,
            clear_color: ClearColorConfig::Custom(DEFAULT_CAPTURE_CLEAR_COLOR),
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov,
            near: 0.1,
            far: 10000.0,
            ..default()
        }),
        Msaa::Off,
        CaptureCamera,
    ));
    if depth_prepass_enabled {
        capture_camera.insert(DepthPrepass);
    }
    if scene_profile.is_aim() {
        capture_camera.insert((
            RenderLayers::layer(AIM_CAPTURE_RENDER_LAYER),
            ClusterConfig::Single,
        ));
    }
}

#[derive(Component)]
pub struct PreviewCamera;

#[derive(Component)]
pub struct PreviewImageNode;

#[derive(Debug, Default)]
pub(crate) struct PreviewCadenceState {
    initialized: bool,
    max_hz: f64,
    next_present_s: f64,
    last_now_s: f64,
}

impl PreviewCadenceState {
    fn should_present(&mut self, now_s: f64, max_hz: f64) -> bool {
        let max_hz = if max_hz.is_finite() && max_hz > 0.0 {
            max_hz
        } else {
            0.0
        };
        let clock_restarted = self.initialized && now_s < self.last_now_s;
        let cadence_changed = !self.initialized || (self.max_hz - max_hz).abs() > f64::EPSILON;
        self.last_now_s = now_s;

        if clock_restarted || cadence_changed {
            self.initialized = true;
            self.max_hz = max_hz;
            self.next_present_s = if max_hz > 0.0 {
                now_s + 1.0 / max_hz
            } else {
                now_s
            };
            return true;
        }

        if max_hz == 0.0 {
            return true;
        }
        if now_s + f64::EPSILON < self.next_present_s {
            return false;
        }

        let interval_s = 1.0 / max_hz;
        let elapsed_intervals = ((now_s - self.next_present_s) / interval_s)
            .floor()
            .max(0.0)
            + 1.0;
        self.next_present_s += elapsed_intervals * interval_s;
        true
    }
}

pub fn setup_preview_window(world: &mut World) {
    ensure_capture_scene_profile(world);
    let preview_enabled = world
        .resource::<crate::config::SimulationConfig>()
        .preview
        .enabled;
    if !preview_enabled {
        return;
    }

    let render_target_handle = world.resource::<ImageHandle>().0.clone();
    let preview_camera_exists = {
        let mut query = world.query_filtered::<Entity, With<PreviewCamera>>();
        query.iter(world).next().is_some()
    };
    if !preview_camera_exists {
        world.spawn((
            Camera2d::default(),
            Camera {
                order: 1,
                ..default()
            },
            PrimaryEguiContext,
            PreviewCamera,
        ));
    }

    let preview_node_exists = {
        let mut query = world.query_filtered::<Entity, With<PreviewImageNode>>();
        query.iter(world).next().is_some()
    };
    if !preview_node_exists {
        world.spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            // Render as a background; help text UI remains on top.
            GlobalZIndex(-1),
            ImageNode::new(render_target_handle),
            PreviewImageNode,
        ));
    }
}

pub fn copy_transform(target: &Transform, our: &mut Transform) {
    our.translation = target.translation;
    our.scale = target.scale;
    our.rotation = target.rotation;
}

pub fn sync_capture_camera(
    target: Single<&Transform, (With<CaptureSource>, Without<CaptureCamera>)>,
    mut our: Single<&mut Transform, (With<CaptureCamera>, Without<CaptureSource>)>,
    scene_profile: Res<CaptureSceneProfile>,
    aim_targets: Query<Entity, With<AimCaptureTarget>>,
    mut diagnostics: ResMut<CaptureSceneDiagnostics>,
    mut previous_target_count: Local<Option<usize>>,
) {
    copy_transform(&target, &mut our);

    let target_count = aim_targets.iter().count();
    diagnostics.tagged_target_roots = target_count;
    if scene_profile.is_aim() && *previous_target_count != Some(target_count) {
        if target_count == 0 {
            warn!(
                "Aim capture scene has no tagged target roots; capture will contain only the clear color until a supported target instance is ready."
            );
        } else {
            info!("Aim capture scene has {target_count} tagged target root(s).");
        }
    }
    *previous_target_count = Some(target_count);
}

#[derive(Clone, Copy, Debug)]
pub struct CameraIntrinsics {
    pub fx: f64,
    pub fy: f64,
    pub cx: f64,
    pub cy: f64,
    pub width: u32,
    pub height: u32,
}

pub fn compute_camera_intrinsics(width: u32, height: u32, fov_y: f32) -> CameraIntrinsics {
    let fov_y = fov_y as f64;
    let aspect = width as f64 / height as f64;
    let fov_x = 2.0 * ((fov_y / 2.0).tan() * aspect).atan();

    let fx = width as f64 / (2.0 * (fov_x / 2.0).tan());
    let fy = height as f64 / (2.0 * (fov_y / 2.0).tan());

    let cx = width as f64 / 2.0;
    let cy = height as f64 / 2.0;

    CameraIntrinsics {
        fx,
        fy,
        cx,
        cy,
        width,
        height,
    }
}

pub const IMAGE_WIDTH: u32 = 1440;
pub const IMAGE_HEIGHT: u32 = 1080;

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::world::CommandQueue;

    fn capture_camera_has_depth_prepass(setting: Option<bool>) -> bool {
        let mut world = World::new();
        world.insert_resource(ImageHandle(Handle::default()));
        world.insert_resource(CameraFov(45.0_f32.to_radians()));
        if let Some(enabled) = setting {
            world.insert_resource(CaptureDepthPrepassEnabled(enabled));
        }

        setup_capture_camera(&mut world);
        let mut query = world.query_filtered::<Entity, (With<CaptureCamera>, With<DepthPrepass>)>();
        query.iter(&world).next().is_some()
    }

    fn capture_camera_render_layers(profile: CaptureSceneProfile) -> Option<RenderLayers> {
        let mut world = World::new();
        world.insert_resource(ImageHandle(Handle::default()));
        world.insert_resource(CameraFov(45.0_f32.to_radians()));
        world.insert_resource(profile);

        setup_capture_camera(&mut world);
        let mut query = world.query_filtered::<Option<&RenderLayers>, With<CaptureCamera>>();
        query.single(&world).ok().flatten().cloned()
    }

    #[test]
    fn capture_depth_prepass_defaults_on_and_can_be_explicitly_disabled() {
        assert!(capture_camera_has_depth_prepass(None));
        assert!(capture_camera_has_depth_prepass(Some(true)));
        assert!(!capture_camera_has_depth_prepass(Some(false)));
    }

    #[test]
    fn capture_scene_profile_defaults_and_unknown_values_fall_back_to_full() {
        assert_eq!(
            CaptureSceneProfile::from_value(None),
            CaptureSceneProfile::Full
        );
        assert_eq!(
            CaptureSceneProfile::from_value(Some(" full ")),
            CaptureSceneProfile::Full
        );
        assert_eq!(
            CaptureSceneProfile::from_value(Some("unsupported")),
            CaptureSceneProfile::Full
        );
        assert_eq!(
            CaptureSceneProfile::from_value(Some("AIM")),
            CaptureSceneProfile::Aim
        );
    }

    #[test]
    fn aim_capture_layer_keeps_targets_and_excludes_untagged_full_scene_entities() {
        assert!(capture_camera_render_layers(CaptureSceneProfile::Full).is_none());

        let capture_layers =
            capture_camera_render_layers(CaptureSceneProfile::Aim).expect("aim camera layer");
        let target_layers = aim_target_render_layers();
        assert!(capture_layers.intersects(&target_layers));
        assert!(!capture_layers.intersects(&RenderLayers::default()));
        assert!(RenderLayers::default().intersects(&target_layers));
    }

    #[test]
    fn aim_target_marker_applies_layers_to_root_and_every_supplied_descendant() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let child = world.spawn_empty().id();
        let grandchild = world.spawn_empty().id();
        let excluded = world.spawn_empty().id();
        let mut queue = CommandQueue::default();
        {
            let mut commands = Commands::new(&mut queue, &world);
            mark_aim_capture_target(&mut commands, root, [child, grandchild]);
        }
        queue.apply(&mut world);

        let expected = aim_target_render_layers();
        assert_eq!(world.get::<RenderLayers>(root), Some(&expected));
        assert_eq!(world.get::<RenderLayers>(child), Some(&expected));
        assert_eq!(world.get::<RenderLayers>(grandchild), Some(&expected));
        assert!(world.get::<RenderLayers>(excluded).is_none());
        assert!(world.get::<AimCaptureTarget>(root).is_some());
        assert!(world.get::<AimCaptureTarget>(child).is_none());
    }

    #[test]
    fn aim_capture_diagnostics_report_missing_then_ready_target_roots() {
        let mut app = App::new();
        app.insert_resource(CaptureSceneProfile::Aim)
            .insert_resource(CaptureSceneDiagnostics::default())
            .add_systems(Update, sync_capture_camera);
        app.world_mut().spawn((Transform::IDENTITY, CaptureSource));
        app.world_mut().spawn((Transform::IDENTITY, CaptureCamera));

        app.update();
        assert_eq!(
            app.world()
                .resource::<CaptureSceneDiagnostics>()
                .tagged_target_roots,
            0
        );

        app.world_mut().spawn(AimCaptureTarget);
        app.update();
        assert_eq!(
            app.world()
                .resource::<CaptureSceneDiagnostics>()
                .tagged_target_roots,
            1
        );
    }

    #[test]
    fn preview_cadence_is_unlimited_by_default_and_rate_limited_when_requested() {
        let mut unlimited = PreviewCadenceState::default();
        assert!(unlimited.should_present(0.0, 0.0));
        assert!(unlimited.should_present(0.001, 0.0));

        let mut limited = PreviewCadenceState::default();
        assert!(limited.should_present(0.0, 60.0));
        assert!(!limited.should_present(0.001, 60.0));
        assert!(limited.should_present(1.0 / 60.0, 60.0));
        assert!(!limited.should_present(1.0 / 60.0, 60.0));
        assert!(limited.should_present(2.0 / 60.0, 60.0));
    }

    #[test]
    fn preview_cadence_system_never_deactivates_offscreen_capture_camera() {
        let mut app = App::new();
        let mut config = crate::config::SimulationConfig::default();
        config.preview.enabled = true;
        config.preview.max_hz = 60.0;
        app.insert_resource(config)
            .insert_resource(Time::<Real>::default())
            .add_systems(Update, update_preview_cadence);

        let capture = app
            .world_mut()
            .spawn((Camera::default(), CaptureCamera))
            .id();
        let preview = app
            .world_mut()
            .spawn((Camera::default(), PreviewCamera))
            .id();

        let present_total_before = preview_present_total();
        app.update();
        app.update();

        assert!(app.world().get::<Camera>(capture).unwrap().is_active);
        assert!(!app.world().get::<Camera>(preview).unwrap().is_active);
        assert!(preview_present_total() > present_total_before);
    }
}

/// Schedules only the visible preview camera; the off-screen capture camera is never queried.
pub fn update_preview_cadence(
    time: Res<Time<Real>>,
    config: Res<crate::config::SimulationConfig>,
    mut state: Local<PreviewCadenceState>,
    mut preview_cameras: Query<&mut Camera, With<PreviewCamera>>,
) {
    let should_present = state.should_present(time.elapsed_secs_f64(), config.preview.max_hz);
    let mut preview_exists = false;
    for mut camera in &mut preview_cameras {
        camera.is_active = should_present;
        preview_exists = true;
    }

    if preview_exists && should_present {
        PREVIEW_PRESENT_TOTAL.fetch_add(1, Ordering::Relaxed);
    }
}
