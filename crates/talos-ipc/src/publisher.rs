use crate::layout::*;
use crate::shm::{ShmError, ShmRegion};
use crate::triple_buffer::TripleBufferProducer;
use std::sync::atomic::{Ordering, fence};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct ShmPublisher {
    meta_region: ShmRegion,
    image_pool: ShmRegion,
    current_buffer_id: u8,
}

impl ShmPublisher {
    pub fn create() -> Result<Self, ShmError> {
        let mut meta_region = ShmRegion::create(SHM_NAME_META, size_of::<ShmMetaRegion>())?;
        let image_pool = ShmRegion::create(SHM_NAME_IMAGE_POOL, IMAGE_POOL_SIZE)?;

        unsafe {
            let meta = meta_region.as_mut::<ShmMetaRegion>();
            let producer_epoch = Self::now_ns().max(1);

            // 初始化 header
            meta.header = ShmHeader {
                magic: SHM_MAGIC,
                version: SHM_VERSION,
                created_ns: producer_epoch,
                heartbeat_ns: producer_epoch,
                image_width: IMAGE_WIDTH,
                image_height: IMAGE_HEIGHT,
                _pad: [0; 32],
            };

            // 初始化所有 TripleBuffer (CRITICAL: 零填充破坏了正确的初始状态)
            // 正确初始状态: state=1 (ready slot), write_idx=0, read_idx=2
            Self::init_triple_buffer(&mut meta.image);
            for pose in &mut meta.poses {
                Self::init_triple_buffer(pose);
            }
            Self::init_triple_buffer(&mut meta.gimbal_cmd);
        }

        Ok(Self {
            meta_region,
            image_pool,
            current_buffer_id: 0,
        })
    }

    /// Returns the exact producer epoch stored in the Talos v3 metadata header.
    pub fn producer_epoch(&self) -> u64 {
        // The mapped region has the exact `ShmMetaRegion` size and is initialized in `create`.
        unsafe { self.meta_region.as_ref::<ShmMetaRegion>() }
            .header
            .created_ns
    }

    pub fn publish_image(&mut self, data: &[u8], seq: u64, timestamp_ns: u64) {
        self.publish_sized_image(data, IMAGE_WIDTH, IMAGE_HEIGHT, seq, timestamp_ns);
    }

    /// Publishes a tightly packed RGB24 image inside an unchanged maximum-size Talos v3 slot.
    pub fn publish_sized_image(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        seq: u64,
        timestamp_ns: u64,
    ) {
        let payload_size = validate_sized_image(data, width, height);

        let buffer_id = self.current_buffer_id;
        self.current_buffer_id = (self.current_buffer_id + 1) % 3;

        unsafe {
            let pool_ptr = self.image_pool.as_ptr();
            let dst = pool_ptr.add(image_slot_offset(buffer_id));
            std::ptr::copy_nonoverlapping(data.as_ptr(), dst, payload_size);
        }

        unsafe {
            let meta = self.meta_region.as_mut::<ShmMetaRegion>();
            let mut producer = TripleBufferProducer::new(
                &meta.image.state,
                &mut meta.image.write_idx,
                &mut meta.image.slots,
            );

            let slot = producer.borrow_mut();
            *slot = sized_image_meta(seq, timestamp_ns, width, height, buffer_id);
            producer.publish();
        }
    }

    pub fn publish_pose(
        &mut self,
        index: PoseIndex,
        position: [f32; 3],
        quaternion: [f32; 4],
        frame_seq: u64,
        timestamp_ns: u64,
    ) {
        self.publish_pose_with_aux(
            index,
            position,
            quaternion,
            [0.0; 4],
            frame_seq,
            timestamp_ns,
        );
    }

    pub fn publish_pose_with_aux(
        &mut self,
        index: PoseIndex,
        position: [f32; 3],
        quaternion: [f32; 4],
        aux_f32: [f32; 4],
        frame_seq: u64,
        timestamp_ns: u64,
    ) {
        unsafe {
            let meta = self.meta_region.as_mut::<ShmMetaRegion>();
            let pose_buf = &mut meta.poses[index as usize];
            let mut producer = TripleBufferProducer::new(
                &pose_buf.state,
                &mut pose_buf.write_idx,
                &mut pose_buf.slots,
            );

            let slot = producer.borrow_mut();
            slot.frame_seq = frame_seq;
            slot.position = position;
            slot.quaternion = quaternion;
            slot.timestamp_ns = timestamp_ns;
            slot._pad = aux_f32_to_bytes(aux_f32);

            producer.publish();
        }
    }

    pub fn set_camera_info(&mut self, info: CameraInfo) {
        unsafe {
            let meta = self.meta_region.as_mut::<ShmMetaRegion>();
            meta.camera_info = info;
        }
    }

    pub fn publish_chassis_observation(&mut self, observation: ChassisObservation) {
        unsafe {
            let meta = self.meta_region.as_mut::<ShmMetaRegion>();
            meta.chassis_observation = observation;
        }
    }

    pub fn publish_ground_truth(
        &mut self,
        batch: &GroundTruthBatch,
        exposure_state: &ExposureState,
    ) {
        unsafe {
            let meta = self.meta_region.as_mut::<ShmMetaRegion>();
            // Preserve the v5 latest-value view for diagnostics and old internal
            // callers while exact exposure scoring consumes the v6 history.
            meta.ground_truth = *batch;
            let publication = meta
                .ground_truth_history
                .next_publication
                .fetch_add(1, Ordering::Relaxed)
                .wrapping_add(1);
            let index = publication.wrapping_sub(1) as usize % GROUND_TRUTH_HISTORY_SLOTS;
            publish_ground_truth_slot(
                &mut meta.ground_truth_history.slots[index],
                batch,
                exposure_state,
                publication,
            );
        }
    }

    pub fn publish_runtime_state(&mut self, state: RuntimeState) {
        unsafe {
            let meta = self.meta_region.as_mut::<ShmMetaRegion>();
            meta.runtime_state = state;
        }
    }

    pub fn update_heartbeat(&mut self) {
        unsafe {
            let meta = self.meta_region.as_mut::<ShmMetaRegion>();
            meta.header.heartbeat_ns = Self::now_ns();
        }
    }

    fn now_ns() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }

    /// 初始化 TripleBuffer 到正确的初始状态
    ///
    /// ShmRegion::create() 使用零填充，会破坏 TripleBuffer 的正确初始状态。
    /// 必须手动重新初始化。
    ///
    /// 正确初始状态:
    /// - state = 1 (ready slot 是 1, 无 FLAG_NEW)
    /// - write_idx = 0 (生产者写入 slot 0)
    /// - read_idx = 2 (消费者上次读取 slot 2)
    fn init_triple_buffer(buf: &mut impl TripleBufferInit) {
        buf.init_state();
    }
}

fn publish_ground_truth_slot(
    slot: &mut GroundTruthHistorySlot,
    batch: &GroundTruthBatch,
    exposure_state: &ExposureState,
    publication: u64,
) {
    let stable = publication.wrapping_mul(2);
    slot.commit_seq
        .store(stable.wrapping_sub(1), Ordering::SeqCst);
    fence(Ordering::SeqCst);
    slot.ground_truth = *batch;
    slot.exposure_state = *exposure_state;
    fence(Ordering::Release);
    slot.commit_seq.store(stable, Ordering::Release);
}

fn checked_image_payload_size(width: u32, height: u32) -> Option<usize> {
    if width == 0 || height == 0 || width > IMAGE_WIDTH || height > IMAGE_HEIGHT {
        return None;
    }

    (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(IMAGE_CHANNELS as usize)
}

fn validate_sized_image(data: &[u8], width: u32, height: u32) -> usize {
    let payload_size = checked_image_payload_size(width, height).unwrap_or_else(|| {
        panic!(
            "Image dimensions out of range: {width}x{height}, maximum is {IMAGE_WIDTH}x{IMAGE_HEIGHT}"
        )
    });
    assert_eq!(
        data.len(),
        payload_size,
        "Image payload size mismatch for {width}x{height} RGB24"
    );
    payload_size
}

fn image_slot_offset(buffer_id: u8) -> usize {
    assert!(buffer_id < 3, "Image buffer id out of range: {buffer_id}");
    buffer_id as usize * IMAGE_SIZE
}

fn sized_image_meta(
    seq: u64,
    timestamp_ns: u64,
    width: u32,
    height: u32,
    buffer_id: u8,
) -> ImageMeta {
    ImageMeta {
        seq,
        timestamp_ns,
        width,
        height,
        buffer_id,
        format: 0,
        _pad: [0; 6],
    }
}

fn aux_f32_to_bytes(aux_f32: [f32; 4]) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    for (i, value) in aux_f32.iter().enumerate() {
        bytes[i * 4..(i + 1) * 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// Trait for initializing triple buffer state
trait TripleBufferInit {
    fn init_state(&mut self);
}

impl TripleBufferInit for ImageTripleBuffer {
    fn init_state(&mut self) {
        self.state.store(1, Ordering::Relaxed);
        self.write_idx = 0;
        self.read_idx = 2;
    }
}

impl TripleBufferInit for PoseTripleBuffer {
    fn init_state(&mut self) {
        self.state.store(1, Ordering::Relaxed);
        self.write_idx = 0;
        self.read_idx = 2;
    }
}

impl TripleBufferInit for GimbalTripleBuffer {
    fn init_state(&mut self) {
        self.state.store(1, Ordering::Relaxed);
        self.write_idx = 0;
        self.read_idx = 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sized_image_payload_accepts_fixed_maximum_and_smaller_frames() {
        assert_eq!(
            checked_image_payload_size(IMAGE_WIDTH, IMAGE_HEIGHT),
            Some(IMAGE_SIZE)
        );
        assert_eq!(checked_image_payload_size(640, 360), Some(640 * 360 * 3));
        assert_eq!(
            validate_sized_image(&vec![0; 640 * 360 * 3], 640, 360),
            640 * 360 * 3
        );
    }

    #[test]
    fn sized_image_payload_rejects_out_of_bounds_dimensions() {
        assert_eq!(checked_image_payload_size(0, 360), None);
        assert_eq!(checked_image_payload_size(640, 0), None);
        assert_eq!(checked_image_payload_size(IMAGE_WIDTH + 1, 360), None);
        assert_eq!(checked_image_payload_size(640, IMAGE_HEIGHT + 1), None);
    }

    #[test]
    #[should_panic(expected = "Image payload size mismatch")]
    fn sized_image_payload_requires_exact_data_length() {
        validate_sized_image(&[0; 3], 2, 2);
    }

    #[test]
    fn sized_image_meta_uses_actual_dimensions_and_fixed_slot_stride() {
        let meta = sized_image_meta(42, 123, 640, 360, 2);
        assert_eq!(meta.seq, 42);
        assert_eq!(meta.timestamp_ns, 123);
        assert_eq!(meta.width, 640);
        assert_eq!(meta.height, 360);
        assert_eq!(meta.buffer_id, 2);
        assert_eq!(image_slot_offset(0), 0);
        assert_eq!(image_slot_offset(1), IMAGE_SIZE);
        assert_eq!(image_slot_offset(2), 2 * IMAGE_SIZE);
    }

    #[test]
    fn ground_truth_slot_commit_is_even_and_bundles_one_exposure() {
        let mut slot = GroundTruthHistorySlot::default();
        let mut batch = GroundTruthBatch::default();
        batch.frame_seq = 77;
        batch.timestamp_ns = 9001;
        let mut exposure = ExposureState::default();
        exposure.frame_seq = batch.frame_seq;
        exposure.timestamp_ns = batch.timestamp_ns;

        publish_ground_truth_slot(&mut slot, &batch, &exposure, 5);

        assert_eq!(slot.commit_seq.load(Ordering::Acquire), 10);
        assert_eq!(slot.ground_truth.frame_seq, 77);
        assert_eq!(slot.exposure_state.frame_seq, 77);
        assert_eq!(
            slot.ground_truth.timestamp_ns,
            slot.exposure_state.timestamp_ns
        );
    }
}
