pub mod depth;
pub mod driver;
pub mod view_copy;

use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::cluster::ClusterConfig;
use bevy::prelude::*;

pub use driver::CaptureBundle;

#[derive(Component)]
pub struct CaptureSource;

#[derive(Component)]
pub struct CaptureCamera;

#[derive(Component, Clone, Copy, Debug)]
pub struct AimCaptureTarget;

#[derive(Component, Clone, Copy, Debug)]
pub struct AimCaptureTargetCandidate;

pub const FULL_SCENE_RENDER_LAYER: usize = 0;
pub const AIM_CAPTURE_RENDER_LAYER: usize = 1;

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

pub fn aim_target_render_layers() -> RenderLayers {
    RenderLayers::from_layers(&[FULL_SCENE_RENDER_LAYER, AIM_CAPTURE_RENDER_LAYER])
}

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

#[derive(Resource, Clone, Copy, Debug)]
pub struct CaptureDepthPrepassEnabled(pub bool);

impl Default for CaptureDepthPrepassEnabled {
    fn default() -> Self {
        Self(true)
    }
}

pub const DEFAULT_CAPTURE_CLEAR_COLOR: Color = Color::srgb(0.47, 0.50, 0.53);

#[derive(Resource, Deref, Clone)]
pub struct ImageHandle(pub Handle<Image>);

#[derive(Resource, Clone, Copy)]
pub struct CameraFov(pub f32);

pub const CAPTURE_CAMERA_ORDER: isize = -100;

pub fn setup_capture_camera(world: &mut World) {
    let capture_camera_exists = {
        let mut query = world.query_filtered::<Entity, With<CaptureCamera>>();
        query.iter(world).next().is_some()
    };
    if capture_camera_exists {
        return;
    }

    let scene_profile = ensure_capture_scene_profile(world);
    let depth_prepass_enabled = world
        .get_resource::<CaptureDepthPrepassEnabled>()
        .copied()
        .unwrap_or_default()
        .0;
    let render_target_handle = world.resource::<ImageHandle>().0.clone();
    let fov = world.resource::<CameraFov>().0;

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

pub fn setup_preview_window(world: &mut World) {
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
) {
    copy_transform(&target, &mut our);
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

pub const IMAGE_WIDTH: u32 = 1280;
pub const IMAGE_HEIGHT: u32 = 720;

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::world::CommandQueue;

    #[test]
    fn profile_parser_defaults_fail_safe_to_full() {
        assert_eq!(
            CaptureSceneProfile::from_value(None),
            CaptureSceneProfile::Full
        );
        assert_eq!(
            CaptureSceneProfile::from_value(Some("AIM")),
            CaptureSceneProfile::Aim
        );
        assert_eq!(
            CaptureSceneProfile::from_value(Some("invalid")),
            CaptureSceneProfile::Full
        );
    }

    #[test]
    fn aim_layers_keep_targets_visible_to_full_and_aim_cameras() {
        let target = aim_target_render_layers();
        assert!(target.intersects(&RenderLayers::default()));
        assert!(target.intersects(&RenderLayers::layer(AIM_CAPTURE_RENDER_LAYER)));
    }

    #[test]
    fn target_marker_applies_layers_to_descendants() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let child = world.spawn_empty().id();
        let mut queue = CommandQueue::default();
        {
            let mut commands = Commands::new(&mut queue, &world);
            mark_aim_capture_target(&mut commands, root, [child]);
        }
        queue.apply(&mut world);
        assert!(world.get::<AimCaptureTarget>(root).is_some());
        assert_eq!(
            world.get::<RenderLayers>(root),
            world.get::<RenderLayers>(child)
        );
    }
}
