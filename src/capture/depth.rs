use crate::capture::{
    AIM_CAPTURE_RENDER_LAYER, CaptureSource, copy_transform, ensure_capture_scene_profile,
};
use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::prelude::*;

pub const DEPTH_CAPTURE_CAMERA_ORDER: isize = -101;

#[derive(Resource, Clone, Copy)]
pub struct DepthCameraSettings {
    pub width: u32,
    pub height: u32,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

#[derive(Component)]
pub struct DepthCaptureCamera;

pub fn setup_depth_capture_camera(world: &mut World) {
    let depth_camera_exists = {
        let mut query = world.query_filtered::<Entity, With<DepthCaptureCamera>>();
        query.iter(world).next().is_some()
    };
    if depth_camera_exists {
        return;
    }

    let scene_profile = ensure_capture_scene_profile(world);
    let settings = *world.resource::<DepthCameraSettings>();

    let mut depth_camera = world.spawn((
        Camera3d::default(),
        Camera {
            order: DEPTH_CAPTURE_CAMERA_ORDER,
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: settings.fov_y,
            near: settings.near,
            far: settings.far,
            ..default()
        }),
        RenderTarget::None {
            size: UVec2::new(settings.width, settings.height),
        },
        Msaa::Off,
        DepthPrepass,
        DepthCaptureCamera,
    ));
    if scene_profile.is_aim() {
        depth_camera.insert(RenderLayers::layer(AIM_CAPTURE_RENDER_LAYER));
    }
}

pub fn sync_depth_capture_camera(
    target: Single<&Transform, (With<CaptureSource>, Without<DepthCaptureCamera>)>,
    mut our: Single<&mut Transform, (With<DepthCaptureCamera>, Without<CaptureSource>)>,
) {
    copy_transform(&target, &mut our);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::CaptureSceneProfile;

    fn depth_camera_render_layers(profile: CaptureSceneProfile) -> Option<RenderLayers> {
        let mut world = World::new();
        world.insert_resource(DepthCameraSettings {
            width: 1440,
            height: 1080,
            fov_y: 45.0_f32.to_radians(),
            near: 0.1,
            far: 100.0,
        });
        world.insert_resource(profile);
        setup_depth_capture_camera(&mut world);

        let mut query = world.query_filtered::<Option<&RenderLayers>, With<DepthCaptureCamera>>();
        query.single(&world).ok().flatten().cloned()
    }

    #[test]
    fn optional_depth_camera_uses_the_same_full_or_aim_scene_profile() {
        assert!(depth_camera_render_layers(CaptureSceneProfile::Full).is_none());
        assert_eq!(
            depth_camera_render_layers(CaptureSceneProfile::Aim),
            Some(RenderLayers::layer(AIM_CAPTURE_RENDER_LAYER))
        );
    }
}
