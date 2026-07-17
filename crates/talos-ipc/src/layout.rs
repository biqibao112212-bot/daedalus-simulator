use std::sync::atomic::AtomicU8;

pub const IMAGE_WIDTH: u32 = 1280;
pub const IMAGE_HEIGHT: u32 = 720;

pub const CACHE_LINE_SIZE: usize = 64;
pub const SHM_MAGIC: u32 = 0x54414C05;
// Version 6 adds a seqlock-protected exposure history. Readers must reject
// older layouts instead of interpreting enlarged metadata with stale offsets.
pub const SHM_VERSION: u32 = 6;

pub const IMAGE_CHANNELS: u32 = 3;
pub const IMAGE_SIZE: usize = (IMAGE_WIDTH * IMAGE_HEIGHT * IMAGE_CHANNELS) as usize;
pub const IMAGE_POOL_SIZE: usize = IMAGE_SIZE * 3;
pub const SHM_NAME_META: &str = "talos_ipc_meta";
pub const SHM_NAME_IMAGE_POOL: &str = "talos_ipc_image_pool";

pub const FLAG_NEW: u8 = 0x80;
pub const INDEX_MASK: u8 = 0x03;

#[repr(C, align(32))]
#[derive(Debug, Clone, Copy, Default)]
pub struct ImageMeta {
    pub seq: u64,
    pub timestamp_ns: u64,
    pub width: u32,
    pub height: u32,
    pub buffer_id: u8,
    pub format: u8,
    pub _pad: [u8; 6],
}
const _: () = assert!(size_of::<ImageMeta>() == 32);

#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct PoseMeta {
    pub frame_seq: u64,
    pub position: [f32; 3],
    pub quaternion: [f32; 4],
    pub timestamp_ns: u64,
    pub _pad: [u8; 16],
}
const _: () = assert!(size_of::<PoseMeta>() == 64);

impl Default for PoseMeta {
    fn default() -> Self {
        Self {
            frame_seq: 0,
            position: [0.0; 3],
            quaternion: [0.0; 4],
            timestamp_ns: 0,
            _pad: [0; 16],
        }
    }
}

#[repr(C, align(32))]
#[derive(Debug, Clone, Copy, Default)]
pub struct GimbalCmd {
    pub timestamp_ns: u64,
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub distance_m: f32,
    pub fire_advice: u8,
    pub _pad: [u8; 11],
}
const _: () = assert!(size_of::<GimbalCmd>() == 32);

#[repr(C, align(64))]
#[derive(Debug, Clone, Copy, Default)]
pub struct CameraInfo {
    pub timestamp_ns: u64,
    pub fx: f64,
    pub fy: f64,
    pub cx: f64,
    pub cy: f64,
    pub distortion: [f64; 5],
    pub width: u32,
    pub height: u32,
    pub _pad: [u8; 24],
}
const _: () = assert!(size_of::<CameraInfo>() == 128);

#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct ChassisObservation {
    pub frame_seq: u64,
    pub timestamp_ns: u64,
    pub dt_s: f32,
    pub v_body: [f32; 2],
    pub wz_radps: f32,
    pub wheel_linear_mps: [f32; 4],
    pub wheel_angular_radps: [f32; 4],
    pub a_body: [f32; 2],
    pub alpha_z_radps2: f32,
    pub rpy_rad: [f32; 3],
    pub gyro_xyz_radps: [f32; 3],
    pub accel_xyz_mps2: [f32; 3],
    pub _pad: [u8; 16],
}
const _: () = assert!(size_of::<ChassisObservation>() == 128);

impl Default for ChassisObservation {
    fn default() -> Self {
        Self {
            frame_seq: 0,
            timestamp_ns: 0,
            dt_s: 0.0,
            v_body: [0.0; 2],
            wz_radps: 0.0,
            wheel_linear_mps: [0.0; 4],
            wheel_angular_radps: [0.0; 4],
            a_body: [0.0; 2],
            alpha_z_radps2: 0.0,
            rpy_rad: [0.0; 3],
            gyro_xyz_radps: [0.0; 3],
            accel_xyz_mps2: [0.0; 3],
            _pad: [0; 16],
        }
    }
}

#[repr(C, align(64))]
pub struct ImageTripleBuffer {
    pub state: AtomicU8,
    pub write_idx: u8,
    pub read_idx: u8,
    pub _pad1: [u8; 61],
    pub slots: [ImageMeta; 3],
}
const _: () = assert!(size_of::<ImageTripleBuffer>() == 192);

#[repr(C, align(64))]
pub struct PoseTripleBuffer {
    pub state: AtomicU8,
    pub write_idx: u8,
    pub read_idx: u8,
    pub _pad1: [u8; 61],
    pub slots: [PoseMeta; 3],
}
const _: () = assert!(size_of::<PoseTripleBuffer>() == 256);

#[repr(C, align(64))]
pub struct GimbalTripleBuffer {
    pub state: AtomicU8,
    pub write_idx: u8,
    pub read_idx: u8,
    pub _pad1: [u8; 61],
    pub slots: [GimbalCmd; 3],
}
const _: () = assert!(size_of::<GimbalTripleBuffer>() == 192);

#[repr(C, align(64))]
pub struct ShmHeader {
    pub magic: u32,
    pub version: u32,
    pub created_ns: u64,
    pub heartbeat_ns: u64,
    pub image_width: u32,
    pub image_height: u32,
    pub _pad: [u8; 32],
}
const _: () = assert!(size_of::<ShmHeader>() == 64);

pub const GROUND_TRUTH_MAX_TARGETS: usize = 16;
pub const GROUND_TRUTH_MAX_RUNES: usize = 4;
pub const GROUND_TRUTH_MAX_ARMORS_PER_TARGET: usize = 4;

pub const GROUND_TRUTH_VISIBILITY_UNKNOWN: u8 = 0;
pub const GROUND_TRUTH_VISIBILITY_HIDDEN: u8 = 1;

pub const GROUND_TRUTH_FRAME_UNKNOWN: u8 = 0;
pub const GROUND_TRUTH_FRAME_ROS_ODOM: u8 = 1;
pub const GROUND_TRUTH_FRAME_CHASSIS_LOCAL_ROS: u8 = 2;

pub const GROUND_TRUTH_TARGET_HAS_WORLD_STATE: u32 = 1 << 0;
pub const GROUND_TRUTH_TARGET_HAS_WORLD_ORIENTATION: u32 = 1 << 1;
pub const GROUND_TRUTH_TARGET_HAS_ARMOR_GEOMETRY: u32 = 1 << 2;

pub const EXPOSURE_STATE_HAS_CHASSIS_WORLD_POSE: u32 = 1 << 0;
pub const EXPOSURE_STATE_HAS_GIMBAL_WORLD_POSE: u32 = 1 << 1;
pub const EXPOSURE_STATE_HAS_CAMERA_WORLD_POSE: u32 = 1 << 2;

/// At 60 Hz this retains 267 ms, covering five frames at the current ~20 Hz
/// consumer rate plus scheduling jitter. Exact matches older than the ring
/// fail closed; readers must never substitute a neighboring frame.
pub const GROUND_TRUTH_HISTORY_SLOTS: usize = 16;

#[repr(C, align(32))]
#[derive(Debug, Clone, Copy, Default)]
pub struct GroundTruthArmor {
    /// Relative Z4 slot, ordered by chassis-local ROS yaw.
    pub relative_slot: u8,
    /// UNKNOWN unless the scene hierarchy explicitly marks the armor hidden.
    pub visibility: u8,
    pub _pad1: [u8; 2],
    /// Armor-center offset in chassis-local ROS axes.
    pub relative_position: [f32; 3],
    /// Radial outward unit normal in chassis-local ROS axes.
    pub outward_normal: [f32; 3],
    /// Chassis-local yaw of outward_normal.
    pub relative_yaw: f32,
}
const _: () = assert!(size_of::<GroundTruthArmor>() == 32);

#[repr(C, align(32))]
#[derive(Debug, Clone, Copy, Default)]
pub struct GroundTruthTarget {
    pub frame_seq: u64,
    pub timestamp_ns: u64,
    pub team: u8,
    pub armor_label: u8,
    pub is_outpost: u8,
    pub armor_count: u8,
    pub position: [f32; 3],
    pub vyaw: f32,
    pub yaw: f32,
    pub velocity: [f32; 3],
    /// Mean horizontal radius for even/odd relative slots.
    pub radius_even: f32,
    pub radius_odd: f32,
    /// Mean armor-center height in chassis-local ROS axes.
    pub armor_height: f32,
    pub armors: [GroundTruthArmor; GROUND_TRUTH_MAX_ARMORS_PER_TARGET],
    /// Bevy Entity bits: stable across frames within one simulator run.
    pub target_id: u64,
    /// Full chassis orientation in ROS odom, encoded as [w, x, y, z].
    pub world_quaternion_wxyz: [f32; 4],
    pub world_state_frame: u8,
    pub armor_geometry_frame: u8,
    pub _pad2: [u8; 2],
    pub state_flags: u32,
}
const _: () = assert!(size_of::<GroundTruthTarget>() == 224);

#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct GroundTruthRune {
    pub frame_seq: u64,
    pub timestamp_ns: u64,
    pub team: u8,
    pub rune_mode: u8,
    pub mechanism_state: u8,
    pub _pad1: u8,
    pub r_center_odom: [f32; 3],
    pub radius: f32,
    pub current_angle: f32,
    pub v_roll: f32,
    pub direction: i32,
    pub sin_amplitude: f32,
    pub sin_omega: f32,
    pub sin_phase: f32,
    pub sin_offset: f32,
    pub relative_time: f32,
    pub blade_id: i32,
    pub target_activations: [u8; 5],
    pub _pad: [u8; 20],
}
const _: () = assert!(size_of::<GroundTruthRune>() == 128);

impl Default for GroundTruthRune {
    fn default() -> Self {
        Self {
            frame_seq: 0,
            timestamp_ns: 0,
            team: 0,
            rune_mode: 0,
            mechanism_state: 0,
            _pad1: 0,
            r_center_odom: [0.0; 3],
            radius: 0.0,
            current_angle: 0.0,
            v_roll: 0.0,
            direction: 0,
            sin_amplitude: 0.0,
            sin_omega: 0.0,
            sin_phase: 0.0,
            sin_offset: 0.0,
            relative_time: 0.0,
            blade_id: -1,
            target_activations: [0; 5],
            _pad: [0; 20],
        }
    }
}

#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct GroundTruthBatch {
    pub frame_seq: u64,
    pub timestamp_ns: u64,
    pub target_count: u32,
    pub rune_count: u32,
    pub targets: [GroundTruthTarget; GROUND_TRUTH_MAX_TARGETS],
    pub runes: [GroundTruthRune; GROUND_TRUTH_MAX_RUNES],
    pub _pad: [u8; 64],
}
const _: () = assert!(size_of::<GroundTruthBatch>() == 4224);

impl Default for GroundTruthBatch {
    fn default() -> Self {
        Self {
            frame_seq: 0,
            timestamp_ns: 0,
            target_count: 0,
            rune_count: 0,
            targets: [GroundTruthTarget::default(); GROUND_TRUTH_MAX_TARGETS],
            runes: [GroundTruthRune::default(); GROUND_TRUTH_MAX_RUNES],
            _pad: [0; 64],
        }
    }
}

#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct ExposureState {
    pub frame_seq: u64,
    pub timestamp_ns: u64,
    pub state_flags: u32,
    pub world_frame: u8,
    pub _pad1: [u8; 3],
    pub chassis_position_world: [f32; 3],
    pub chassis_quaternion_world_wxyz: [f32; 4],
    pub chassis_rpy_world: [f32; 3],
    pub gimbal_position_world: [f32; 3],
    pub gimbal_quaternion_world_wxyz: [f32; 4],
    pub camera_position_world: [f32; 3],
    pub camera_quaternion_world_wxyz: [f32; 4],
    pub _pad: [u8; 8],
}
const _: () = assert!(size_of::<ExposureState>() == 128);

impl Default for ExposureState {
    fn default() -> Self {
        Self {
            frame_seq: 0,
            timestamp_ns: 0,
            state_flags: 0,
            world_frame: GROUND_TRUTH_FRAME_UNKNOWN,
            _pad1: [0; 3],
            chassis_position_world: [0.0; 3],
            chassis_quaternion_world_wxyz: [0.0; 4],
            chassis_rpy_world: [0.0; 3],
            gimbal_position_world: [0.0; 3],
            gimbal_quaternion_world_wxyz: [0.0; 4],
            camera_position_world: [0.0; 3],
            camera_quaternion_world_wxyz: [0.0; 4],
            _pad: [0; 8],
        }
    }
}

#[repr(C, align(64))]
pub struct GroundTruthHistorySlot {
    /// Odd means being written; equal non-zero even values before/after a copy
    /// identify one stable publication.
    pub commit_seq: std::sync::atomic::AtomicU64,
    pub _pad1: [u8; 56],
    pub ground_truth: GroundTruthBatch,
    pub exposure_state: ExposureState,
}
const _: () = assert!(size_of::<GroundTruthHistorySlot>() == 4416);

impl Default for GroundTruthHistorySlot {
    fn default() -> Self {
        Self {
            commit_seq: std::sync::atomic::AtomicU64::new(0),
            _pad1: [0; 56],
            ground_truth: GroundTruthBatch::default(),
            exposure_state: ExposureState::default(),
        }
    }
}

#[repr(C, align(64))]
pub struct GroundTruthHistory {
    pub next_publication: std::sync::atomic::AtomicU64,
    pub _pad1: [u8; 56],
    pub slots: [GroundTruthHistorySlot; GROUND_TRUTH_HISTORY_SLOTS],
}
const _: () = assert!(size_of::<GroundTruthHistory>() == 70720);

impl Default for GroundTruthHistory {
    fn default() -> Self {
        Self {
            next_publication: std::sync::atomic::AtomicU64::new(0),
            _pad1: [0; 56],
            slots: std::array::from_fn(|_| GroundTruthHistorySlot::default()),
        }
    }
}

#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct RuntimeState {
    pub timestamp_ns: u64,
    pub following: u8,
    pub _pad1: [u8; 3],
    pub gimbal_yaw_rad: f32,
    pub gimbal_pitch_rad: f32,
    pub _pad: [u8; 44],
}
const _: () = assert!(size_of::<RuntimeState>() == 64);

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            timestamp_ns: 0,
            following: 0,
            _pad1: [0; 3],
            gimbal_yaw_rad: 0.0,
            gimbal_pitch_rad: 0.0,
            _pad: [0; 44],
        }
    }
}

#[repr(C)]
pub struct ShmMetaRegion {
    pub header: ShmHeader,
    pub image: ImageTripleBuffer,
    pub poses: [PoseTripleBuffer; 5],
    pub gimbal_cmd: GimbalTripleBuffer,
    pub camera_info: CameraInfo,
    pub chassis_observation: ChassisObservation,
    pub ground_truth: GroundTruthBatch,
    pub runtime_state: RuntimeState,
    pub ground_truth_history: GroundTruthHistory,
}
const _: () = assert!(size_of::<ShmMetaRegion>() == 76992);
const _: () = assert!(std::mem::offset_of!(ShmMetaRegion, camera_info) == 1728);
const _: () = assert!(std::mem::offset_of!(ShmMetaRegion, chassis_observation) == 1856);
const _: () = assert!(std::mem::offset_of!(ShmMetaRegion, ground_truth) == 1984);
const _: () = assert!(std::mem::offset_of!(ShmMetaRegion, runtime_state) == 6208);
const _: () = assert!(std::mem::offset_of!(ShmMetaRegion, ground_truth_history) == 6272);

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoseIndex {
    Gimbal = 0,
    Odom = 1,
    Muzzle = 2,
    Camera = 3,
    // Legacy compatibility channel.
    // New integrations should consume `ShmMetaRegion::chassis_observation` instead.
    ChassisObservation = 4,
}

impl Default for ImageTripleBuffer {
    fn default() -> Self {
        Self {
            state: AtomicU8::new(1),
            write_idx: 0,
            read_idx: 2,
            _pad1: [0; 61],
            slots: [ImageMeta::default(); 3],
        }
    }
}

impl Default for PoseTripleBuffer {
    fn default() -> Self {
        Self {
            state: AtomicU8::new(1),
            write_idx: 0,
            read_idx: 2,
            _pad1: [0; 61],
            slots: [PoseMeta::default(); 3],
        }
    }
}

impl Default for GimbalTripleBuffer {
    fn default() -> Self {
        Self {
            state: AtomicU8::new(1),
            write_idx: 0,
            read_idx: 2,
            _pad1: [0; 61],
            slots: [GimbalCmd::default(); 3],
        }
    }
}

impl Default for ShmHeader {
    fn default() -> Self {
        Self {
            magic: SHM_MAGIC,
            version: SHM_VERSION,
            created_ns: 0,
            heartbeat_ns: 0,
            image_width: IMAGE_WIDTH,
            image_height: IMAGE_HEIGHT,
            _pad: [0; 32],
        }
    }
}

impl Default for ShmMetaRegion {
    fn default() -> Self {
        Self {
            header: ShmHeader::default(),
            image: ImageTripleBuffer::default(),
            poses: [
                PoseTripleBuffer::default(),
                PoseTripleBuffer::default(),
                PoseTripleBuffer::default(),
                PoseTripleBuffer::default(),
                PoseTripleBuffer::default(),
            ],
            gimbal_cmd: GimbalTripleBuffer::default(),
            camera_info: CameraInfo::default(),
            chassis_observation: ChassisObservation::default(),
            ground_truth: GroundTruthBatch::default(),
            runtime_state: RuntimeState::default(),
            ground_truth_history: GroundTruthHistory::default(),
        }
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn ground_truth_v6_layout_is_stable() {
        assert_eq!(SHM_VERSION, 6);
        assert_eq!(size_of::<GroundTruthArmor>(), 32);
        assert_eq!(std::mem::offset_of!(GroundTruthArmor, relative_slot), 0);
        assert_eq!(std::mem::offset_of!(GroundTruthArmor, relative_position), 4);
        assert_eq!(std::mem::offset_of!(GroundTruthArmor, outward_normal), 16);
        assert_eq!(std::mem::offset_of!(GroundTruthArmor, relative_yaw), 28);

        assert_eq!(size_of::<GroundTruthTarget>(), 224);
        // All v4 target fields retain their byte offsets.
        assert_eq!(std::mem::offset_of!(GroundTruthTarget, frame_seq), 0);
        assert_eq!(std::mem::offset_of!(GroundTruthTarget, timestamp_ns), 8);
        assert_eq!(std::mem::offset_of!(GroundTruthTarget, position), 20);
        assert_eq!(std::mem::offset_of!(GroundTruthTarget, vyaw), 32);
        assert_eq!(std::mem::offset_of!(GroundTruthTarget, yaw), 36);
        assert_eq!(std::mem::offset_of!(GroundTruthTarget, velocity), 40);
        assert_eq!(std::mem::offset_of!(GroundTruthTarget, radius_even), 52);
        assert_eq!(std::mem::offset_of!(GroundTruthTarget, armors), 64);
        assert_eq!(std::mem::offset_of!(GroundTruthTarget, target_id), 192);
        assert_eq!(
            std::mem::offset_of!(GroundTruthTarget, world_quaternion_wxyz),
            200
        );
        assert_eq!(
            std::mem::offset_of!(GroundTruthTarget, world_state_frame),
            216
        );
        assert_eq!(std::mem::offset_of!(GroundTruthTarget, state_flags), 220);

        assert_eq!(size_of::<GroundTruthBatch>(), 4224);
        assert_eq!(std::mem::offset_of!(GroundTruthBatch, targets), 32);
        assert_eq!(std::mem::offset_of!(GroundTruthBatch, runes), 3648);
        assert_eq!(size_of::<ExposureState>(), 128);
        assert_eq!(
            std::mem::offset_of!(ExposureState, chassis_position_world),
            24
        );
        assert_eq!(
            std::mem::offset_of!(ExposureState, gimbal_position_world),
            64
        );
        assert_eq!(
            std::mem::offset_of!(ExposureState, camera_position_world),
            92
        );
        assert_eq!(size_of::<GroundTruthHistorySlot>(), 4416);
        assert_eq!(
            std::mem::offset_of!(GroundTruthHistorySlot, ground_truth),
            64
        );
        assert_eq!(
            std::mem::offset_of!(GroundTruthHistorySlot, exposure_state),
            4288
        );
        assert_eq!(size_of::<GroundTruthHistory>(), 70720);
        assert_eq!(std::mem::offset_of!(GroundTruthHistory, slots), 64);
        assert_eq!(size_of::<ShmMetaRegion>(), 76992);
        assert_eq!(std::mem::offset_of!(ShmMetaRegion, ground_truth), 1984);
        assert_eq!(std::mem::offset_of!(ShmMetaRegion, runtime_state), 6208);
        assert_eq!(
            std::mem::offset_of!(ShmMetaRegion, ground_truth_history),
            6272
        );
    }

    #[test]
    fn new_ground_truth_fields_default_to_unknown_and_zero() {
        let target = GroundTruthTarget::default();
        assert_eq!(target.armor_count, 0);
        assert_eq!(target.radius_even, 0.0);
        assert_eq!(target.radius_odd, 0.0);
        assert_eq!(target.armor_height, 0.0);
        assert_eq!(target.target_id, 0);
        assert_eq!(target.world_quaternion_wxyz, [0.0; 4]);
        assert_eq!(target.world_state_frame, GROUND_TRUTH_FRAME_UNKNOWN);
        assert_eq!(target.armor_geometry_frame, GROUND_TRUTH_FRAME_UNKNOWN);
        assert_eq!(target.state_flags, 0);
        assert!(target.armors.iter().all(|armor| {
            armor.visibility == GROUND_TRUTH_VISIBILITY_UNKNOWN
                && armor.relative_position == [0.0; 3]
                && armor.outward_normal == [0.0; 3]
        }));
    }
}
