use bevy::asset::RenderAssetUsages;
use bevy::core_pipeline::schedule::camera_driver;
use bevy::ecs::world::DeferredWorld;
use bevy::render::texture::GpuImage;
use bevy::tasks::AsyncComputeTaskPool;
use bevy::{
    image::TextureFormatPixelInfo,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        render_asset::RenderAssets,
        render_resource::{
            Buffer, BufferDescriptor, BufferUsages, Extent3d, MapMode, Origin3d,
            TexelCopyBufferInfo, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect,
            TextureFormat, TextureUsages,
        },
        renderer::{RenderContext, RenderDevice, RenderGraph, RenderGraphSystems},
    },
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

// This bounds only records waiting to be mapped; detached map/convert/handler work is tracked below.
const MAX_QUEUED_FRAMES: usize = 2;
const MAX_FAST_READBACK_BUFFERS: usize = 3;
static CAPTURE_COPY_SUBMIT_TOTAL: AtomicU64 = AtomicU64::new(0);
static CAPTURE_QUEUE_DROP_TOTAL: AtomicU64 = AtomicU64::new(0);
static CAPTURE_PROCESSING_COMPLETE_TOTAL: AtomicU64 = AtomicU64::new(0);
static CAPTURE_PROCESSING_IN_FLIGHT: AtomicU64 = AtomicU64::new(0);
static CAPTURE_PROCESSING_MAX_IN_FLIGHT: AtomicU64 = AtomicU64::new(0);
static CAPTURE_OWNED_RGBA_ATTEMPT_TOTAL: AtomicU64 = AtomicU64::new(0);
static CAPTURE_OWNED_RGBA_CONSUMED_TOTAL: AtomicU64 = AtomicU64::new(0);
static CAPTURE_OWNED_RGBA_FALLBACK_TOTAL: AtomicU64 = AtomicU64::new(0);
static CAPTURE_BORROWED_CALLBACK_TOTAL: AtomicU64 = AtomicU64::new(0);
static CAPTURE_FAST_BUFFER_ALLOCATED: AtomicU64 = AtomicU64::new(0);
static CAPTURE_FAST_BUFFER_ALLOCATION_TOTAL: AtomicU64 = AtomicU64::new(0);
static CAPTURE_FAST_BUFFER_IN_FLIGHT: AtomicU64 = AtomicU64::new(0);
static CAPTURE_FAST_BUFFER_MAX_IN_FLIGHT: AtomicU64 = AtomicU64::new(0);
static CAPTURE_FAST_NO_BUFFER_DROP_TOTAL: AtomicU64 = AtomicU64::new(0);
static CAPTURE_FAST_QUEUE_PRE_SUBMIT_DROP_TOTAL: AtomicU64 = AtomicU64::new(0);
static CAPTURE_FAST_MAP_CALLBACK_TOTAL: AtomicU64 = AtomicU64::new(0);
static CAPTURE_FAST_MAP_SUCCESS_TOTAL: AtomicU64 = AtomicU64::new(0);
static CAPTURE_FAST_MAP_ERROR_TOTAL: AtomicU64 = AtomicU64::new(0);
static CAPTURE_FAST_MAPPED_COPY_TOTAL: AtomicU64 = AtomicU64::new(0);
static CAPTURE_FAST_MAP_CALLBACK_NS_TOTAL: AtomicU64 = AtomicU64::new(0);
static CAPTURE_FAST_MAP_CALLBACK_NS_MAX: AtomicU64 = AtomicU64::new(0);
static CAPTURE_FAST_MAPPED_COPY_NS_TOTAL: AtomicU64 = AtomicU64::new(0);
static CAPTURE_FAST_MAPPED_COPY_NS_MAX: AtomicU64 = AtomicU64::new(0);
static P1_COPY_TO_MAP_CALLBACK_TOTAL: AtomicU64 = AtomicU64::new(0);
static P1_COPY_TO_MAP_CALLBACK_NS_TOTAL: AtomicU64 = AtomicU64::new(0);
static P1_COPY_TO_MAP_CALLBACK_NS_MAX: AtomicU64 = AtomicU64::new(0);

fn p1_timing_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("DAEDALUS_P1_TIMING")
            .map(|value| matches!(value.trim(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false)
    })
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CapturePipelineCounters {
    pub copy_submit_total: u64,
    pub queue_drop_total: u64,
    pub processing_complete_total: u64,
    pub processing_in_flight: u64,
    pub processing_max_in_flight: u64,
    pub owned_rgba_attempt_total: u64,
    pub owned_rgba_consumed_total: u64,
    pub owned_rgba_fallback_total: u64,
    pub borrowed_callback_total: u64,
    pub fast_buffer_allocated: u64,
    pub fast_buffer_allocation_total: u64,
    pub fast_buffer_in_flight: u64,
    pub fast_buffer_max_in_flight: u64,
    pub fast_no_buffer_drop_total: u64,
    pub fast_queue_pre_submit_drop_total: u64,
    pub fast_map_callback_total: u64,
    pub fast_map_success_total: u64,
    pub fast_map_error_total: u64,
    pub fast_mapped_copy_total: u64,
    pub fast_map_callback_ns_total: u64,
    pub fast_map_callback_ns_max: u64,
    pub fast_submit_to_map_total: u64,
    pub fast_submit_to_map_ns_total: u64,
    pub fast_submit_to_map_ns_max: u64,
    pub fast_mapped_copy_ns_total: u64,
    pub fast_mapped_copy_ns_max: u64,
}

pub fn capture_pipeline_counters() -> CapturePipelineCounters {
    CapturePipelineCounters {
        copy_submit_total: CAPTURE_COPY_SUBMIT_TOTAL.load(Ordering::Relaxed),
        queue_drop_total: CAPTURE_QUEUE_DROP_TOTAL.load(Ordering::Relaxed),
        processing_complete_total: CAPTURE_PROCESSING_COMPLETE_TOTAL.load(Ordering::Relaxed),
        processing_in_flight: CAPTURE_PROCESSING_IN_FLIGHT.load(Ordering::Relaxed),
        processing_max_in_flight: CAPTURE_PROCESSING_MAX_IN_FLIGHT.load(Ordering::Relaxed),
        owned_rgba_attempt_total: CAPTURE_OWNED_RGBA_ATTEMPT_TOTAL.load(Ordering::Relaxed),
        owned_rgba_consumed_total: CAPTURE_OWNED_RGBA_CONSUMED_TOTAL.load(Ordering::Relaxed),
        owned_rgba_fallback_total: CAPTURE_OWNED_RGBA_FALLBACK_TOTAL.load(Ordering::Relaxed),
        borrowed_callback_total: CAPTURE_BORROWED_CALLBACK_TOTAL.load(Ordering::Relaxed),
        fast_buffer_allocated: CAPTURE_FAST_BUFFER_ALLOCATED.load(Ordering::Relaxed),
        fast_buffer_allocation_total: CAPTURE_FAST_BUFFER_ALLOCATION_TOTAL.load(Ordering::Relaxed),
        fast_buffer_in_flight: CAPTURE_FAST_BUFFER_IN_FLIGHT.load(Ordering::Relaxed),
        fast_buffer_max_in_flight: CAPTURE_FAST_BUFFER_MAX_IN_FLIGHT.load(Ordering::Relaxed),
        fast_no_buffer_drop_total: CAPTURE_FAST_NO_BUFFER_DROP_TOTAL.load(Ordering::Relaxed),
        fast_queue_pre_submit_drop_total: CAPTURE_FAST_QUEUE_PRE_SUBMIT_DROP_TOTAL
            .load(Ordering::Relaxed),
        fast_map_callback_total: CAPTURE_FAST_MAP_CALLBACK_TOTAL.load(Ordering::Relaxed),
        fast_map_success_total: CAPTURE_FAST_MAP_SUCCESS_TOTAL.load(Ordering::Relaxed),
        fast_map_error_total: CAPTURE_FAST_MAP_ERROR_TOTAL.load(Ordering::Relaxed),
        fast_mapped_copy_total: CAPTURE_FAST_MAPPED_COPY_TOTAL.load(Ordering::Relaxed),
        fast_map_callback_ns_total: CAPTURE_FAST_MAP_CALLBACK_NS_TOTAL.load(Ordering::Relaxed),
        fast_map_callback_ns_max: CAPTURE_FAST_MAP_CALLBACK_NS_MAX.load(Ordering::Relaxed),
        fast_submit_to_map_total: P1_COPY_TO_MAP_CALLBACK_TOTAL.load(Ordering::Relaxed),
        fast_submit_to_map_ns_total: P1_COPY_TO_MAP_CALLBACK_NS_TOTAL.load(Ordering::Relaxed),
        fast_submit_to_map_ns_max: P1_COPY_TO_MAP_CALLBACK_NS_MAX.load(Ordering::Relaxed),
        fast_mapped_copy_ns_total: CAPTURE_FAST_MAPPED_COPY_NS_TOTAL.load(Ordering::Relaxed),
        fast_mapped_copy_ns_max: CAPTURE_FAST_MAPPED_COPY_NS_MAX.load(Ordering::Relaxed),
    }
}

fn elapsed_ns(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u64::MAX as u128) as u64
}

fn record_duration(total: &AtomicU64, max: &AtomicU64, elapsed_ns: u64) {
    total.fetch_add(elapsed_ns, Ordering::Relaxed);
    max.fetch_max(elapsed_ns, Ordering::Relaxed);
}

fn record_fast_buffer_acquire(allocated_new: bool) {
    if allocated_new {
        CAPTURE_FAST_BUFFER_ALLOCATED.fetch_add(1, Ordering::Relaxed);
        CAPTURE_FAST_BUFFER_ALLOCATION_TOTAL.fetch_add(1, Ordering::Relaxed);
    }
    let in_flight = CAPTURE_FAST_BUFFER_IN_FLIGHT
        .fetch_add(1, Ordering::Relaxed)
        .saturating_add(1);
    CAPTURE_FAST_BUFFER_MAX_IN_FLIGHT.fetch_max(in_flight, Ordering::Relaxed);
}

fn record_fast_buffer_release(retired: bool) {
    let previous_in_flight = CAPTURE_FAST_BUFFER_IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
    debug_assert!(
        previous_in_flight > 0,
        "fast readback in-flight counter underflow"
    );
    if retired {
        let previous_allocated = CAPTURE_FAST_BUFFER_ALLOCATED.fetch_sub(1, Ordering::Relaxed);
        debug_assert!(
            previous_allocated > 0,
            "fast readback allocated counter underflow"
        );
    }
}

fn record_fast_no_buffer_drop() {
    CAPTURE_FAST_NO_BUFFER_DROP_TOTAL.fetch_add(1, Ordering::Relaxed);
}

fn fast_queue_admits_copy(queue_len: usize) -> bool {
    if queue_len < MAX_QUEUED_FRAMES {
        return true;
    }
    CAPTURE_QUEUE_DROP_TOTAL.fetch_add(1, Ordering::Relaxed);
    CAPTURE_FAST_QUEUE_PRE_SUBMIT_DROP_TOTAL.fetch_add(1, Ordering::Relaxed);
    false
}

/// Counts a frame from dequeue through map, conversion, and all async snapshot handlers.
struct CaptureProcessingGuard;

impl CaptureProcessingGuard {
    fn start() -> Self {
        let current = CAPTURE_PROCESSING_IN_FLIGHT
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        CAPTURE_PROCESSING_MAX_IN_FLIGHT.fetch_max(current, Ordering::Relaxed);
        Self
    }
}

impl Drop for CaptureProcessingGuard {
    fn drop(&mut self) {
        CAPTURE_PROCESSING_IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapturedFrameKind {
    Rgb8,
    Rgba8,
    Depth32F,
}

#[derive(Resource, Clone)]
pub struct CaptureConfig {
    pub width: u32,
    pub height: u32,
    pub texture_format: TextureFormat,
    pub frame_kind: CapturedFrameKind,
}

pub struct CapturedFrame<'a> {
    pub kind: CapturedFrameKind,
    pub width: u32,
    pub height: u32,
    pub data: &'a [u8],
}

pub fn create_capture_image_handle(
    app: &mut App,
    width: u32,
    height: u32,
    texture_format: TextureFormat,
    asset_usages: RenderAssetUsages,
    texture_usages: TextureUsages,
) -> Handle<Image> {
    let extent = Extent3d {
        width,
        height,
        ..Default::default()
    };

    let mut image = if matches!(
        texture_format,
        TextureFormat::Depth16Unorm
            | TextureFormat::Depth24Plus
            | TextureFormat::Depth24PlusStencil8
            | TextureFormat::Depth32Float
            | TextureFormat::Depth32FloatStencil8
    ) {
        Image::new_uninit(
            extent,
            bevy::render::render_resource::TextureDimension::D2,
            texture_format,
            asset_usages,
        )
    } else {
        Image::new_target_texture(width, height, texture_format, Some(texture_format))
    };

    image.texture_descriptor.usage |= texture_usages;
    let mut images = app.world_mut().resource_mut::<Assets<Image>>();
    images.add(image)
}

enum CapturePluginNode {
    Camera(CameraCapturePlugin),
    ViewCopy(crate::capture::view_copy::ViewTextureCopyPlugin),
}

pub struct CaptureBundle {
    plugins: Vec<CapturePluginNode>,
    color_target: Option<Handle<Image>>,
    depth_target: Option<Handle<Image>>,
}

impl CaptureBundle {
    pub fn color(
        app: &mut App,
        config: CaptureConfig,
        snapshots: Vec<Box<dyn GpuCaptureHandler>>,
    ) -> Self {
        let (plugin, color_target) = CameraCapturePlugin::new(app, config, snapshots);
        Self {
            plugins: vec![CapturePluginNode::Camera(plugin)],
            color_target: Some(color_target),
            depth_target: None,
        }
    }

    pub fn color_and_depth(
        app: &mut App,
        color_config: CaptureConfig,
        color_snapshots: Vec<Box<dyn GpuCaptureHandler>>,
        depth_snapshots: Vec<Box<dyn GpuCaptureHandler>>,
    ) -> Self {
        Self::color(app, color_config.clone(), color_snapshots).with_depth_from_camera_order(
            app,
            CaptureConfig {
                width: color_config.width,
                height: color_config.height,
                texture_format: TextureFormat::Depth32Float,
                frame_kind: CapturedFrameKind::Depth32F,
            },
            crate::capture::CAPTURE_CAMERA_ORDER,
            depth_snapshots,
        )
    }

    pub fn depth_from_camera_order(
        app: &mut App,
        config: CaptureConfig,
        camera_order: isize,
        snapshots: Vec<Box<dyn GpuCaptureHandler>>,
    ) -> Self {
        let mut bundle = Self {
            plugins: Vec::new(),
            color_target: None,
            depth_target: None,
        };
        bundle.push_depth_from_camera_order(app, config, camera_order, snapshots);
        bundle
    }

    pub fn with_depth_from_camera_order(
        mut self,
        app: &mut App,
        config: CaptureConfig,
        camera_order: isize,
        snapshots: Vec<Box<dyn GpuCaptureHandler>>,
    ) -> Self {
        self.push_depth_from_camera_order(app, config, camera_order, snapshots);
        self
    }

    pub fn color_target(&self) -> Option<&Handle<Image>> {
        self.color_target.as_ref()
    }

    pub fn depth_target(&self) -> Option<&Handle<Image>> {
        self.depth_target.as_ref()
    }

    fn push_depth_from_camera_order(
        &mut self,
        app: &mut App,
        config: CaptureConfig,
        camera_order: isize,
        snapshots: Vec<Box<dyn GpuCaptureHandler>>,
    ) {
        let (view_copy, depth_target) =
            crate::capture::view_copy::ViewTextureCopyPlugin::new_depth_for_camera_order(
                app,
                config.width,
                config.height,
                camera_order,
            );
        let depth_capture =
            CameraCapturePlugin::from_existing_handle(config, depth_target.clone(), snapshots);

        self.plugins.push(CapturePluginNode::ViewCopy(view_copy));
        self.plugins.push(CapturePluginNode::Camera(depth_capture));
        self.depth_target = Some(depth_target);
    }
}

impl Plugin for CaptureBundle {
    fn is_unique(&self) -> bool {
        false
    }

    fn build(&self, app: &mut App) {
        for plugin in &self.plugins {
            match plugin {
                CapturePluginNode::Camera(plugin) => plugin.build(app),
                CapturePluginNode::ViewCopy(plugin) => plugin.build(app),
            }
        }
    }
}

type ToSyncSnapshot = Box<dyn GpuCaptureHandler>;
type DynSnapshotSync = Box<dyn SnapshotSync>;

#[derive(Resource, Default, Deref, DerefMut)]
struct ImageCopiers(Vec<ImageCopier>);

#[derive(Resource, Default)]
struct ImageCopyDriverInstalled(bool);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FastReadbackReservation {
    Reuse,
    Allocate,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FastReadbackSlotState {
    allocated: usize,
    in_flight: usize,
    max_in_flight: usize,
}

impl FastReadbackSlotState {
    fn try_reserve(&mut self, has_reusable_buffer: bool) -> Option<FastReadbackReservation> {
        let reservation = if has_reusable_buffer {
            assert!(
                self.in_flight < self.allocated,
                "a reusable fast buffer must already be allocated and idle"
            );
            FastReadbackReservation::Reuse
        } else if self.allocated < MAX_FAST_READBACK_BUFFERS {
            self.allocated += 1;
            FastReadbackReservation::Allocate
        } else {
            return None;
        };

        self.in_flight += 1;
        self.max_in_flight = self.max_in_flight.max(self.in_flight);
        Some(reservation)
    }

    fn release_recycled(&mut self) {
        assert!(self.in_flight > 0, "fast buffer recycled more than once");
        self.in_flight -= 1;
    }

    fn release_retired(&mut self) {
        assert!(self.in_flight > 0, "fast buffer retired more than once");
        assert!(self.allocated > 0, "retired fast buffer was not allocated");
        self.in_flight -= 1;
        self.allocated -= 1;
    }
}

#[derive(Default)]
struct FastReadbackPoolInner {
    slots: FastReadbackSlotState,
    free_buffers: Vec<Buffer>,
}

#[derive(Default)]
struct FastReadbackPool {
    inner: Mutex<FastReadbackPoolInner>,
}

impl FastReadbackPool {
    fn try_acquire(
        self: &Arc<Self>,
        render_device: &RenderDevice,
        size: u64,
    ) -> Option<FastReadbackBuffer> {
        let (reusable, reservation) = {
            let mut inner = self.inner.lock().unwrap();
            let reusable = inner.free_buffers.pop();
            let reservation = inner.slots.try_reserve(reusable.is_some());
            let Some(reservation) = reservation else {
                debug_assert!(reusable.is_none());
                record_fast_no_buffer_drop();
                return None;
            };
            (reusable, reservation)
        };

        let allocated_new = reservation == FastReadbackReservation::Allocate;
        let buffer = reusable.unwrap_or_else(|| {
            render_device.create_buffer(&BufferDescriptor {
                label: Some("bounded fast capture readback"),
                size,
                usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        record_fast_buffer_acquire(allocated_new);
        Some(FastReadbackBuffer {
            buffer: Some(buffer),
            pool: self.clone(),
        })
    }

    fn recycle(&self, buffer: Buffer) {
        let mut inner = self.inner.lock().unwrap();
        inner.slots.release_recycled();
        inner.free_buffers.push(buffer);
        record_fast_buffer_release(false);
    }

    fn retire(&self, buffer: Buffer) {
        let mut inner = self.inner.lock().unwrap();
        // Drop the last application-owned handle before reopening an allocation
        // slot so another thread cannot transiently create a fourth live buffer.
        drop(buffer);
        inner.slots.release_retired();
        record_fast_buffer_release(true);
    }
}

impl Drop for FastReadbackPool {
    fn drop(&mut self) {
        let inner = self
            .inner
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        debug_assert_eq!(inner.slots.in_flight, 0);
        if inner.slots.allocated > 0 {
            let previous = CAPTURE_FAST_BUFFER_ALLOCATED
                .fetch_sub(inner.slots.allocated as u64, Ordering::Relaxed);
            debug_assert!(previous >= inner.slots.allocated as u64);
        }
    }
}

struct FastReadbackBuffer {
    buffer: Option<Buffer>,
    pool: Arc<FastReadbackPool>,
}

impl FastReadbackBuffer {
    fn buffer(&self) -> &Buffer {
        self.buffer
            .as_ref()
            .expect("fast readback buffer was already released")
    }

    fn recycle(mut self) {
        let buffer = self
            .buffer
            .take()
            .expect("fast readback buffer recycled more than once");
        self.pool.recycle(buffer);
    }

    fn retire(mut self) {
        let buffer = self
            .buffer
            .take()
            .expect("fast readback buffer retired more than once");
        self.pool.retire(buffer);
    }
}

impl Drop for FastReadbackBuffer {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            // A dropped lease may still have a pending map. Retire it rather than
            // making it reusable before the callback has completed.
            self.pool.retire(buffer);
        }
    }
}

enum QueuedReadbackBuffer {
    Legacy(Buffer),
    Fast(FastReadbackBuffer),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SubmittedEvictionAction {
    RecycleLegacy,
    RetireFast,
}

impl SubmittedEvictionAction {
    fn for_fast(fast: bool) -> Self {
        if fast {
            Self::RetireFast
        } else {
            Self::RecycleLegacy
        }
    }
}

impl QueuedReadbackBuffer {
    fn buffer(&self) -> &Buffer {
        match self {
            Self::Legacy(buffer) => buffer,
            Self::Fast(buffer) => buffer.buffer(),
        }
    }

    /// Release a buffer before its texture-to-buffer copy has been submitted.
    ///
    /// Both buffer classes are safe to reuse here because the command encoder
    /// has not recorded any work that refers to the buffer yet.
    fn recycle_unsubmitted(self, legacy_pool: &Arc<Mutex<Vec<Buffer>>>) {
        match self {
            Self::Legacy(buffer) => legacy_pool.lock().unwrap().push(buffer),
            Self::Fast(buffer) => buffer.recycle(),
        }
    }

    fn submitted_eviction_action(&self) -> SubmittedEvictionAction {
        SubmittedEvictionAction::for_fast(matches!(self, Self::Fast(_)))
    }

    /// Release a queued buffer whose GPU copy has already been recorded.
    ///
    /// Legacy buffers retain their historical latest-queue recycling behavior.
    /// A bounded fast lease must instead be retired: returning it to the fast
    /// pool here could let a later frame reuse it while the submitted GPU copy
    /// still references the same allocation.
    fn release_submitted_eviction(self, legacy_pool: &Arc<Mutex<Vec<Buffer>>>) {
        let action = self.submitted_eviction_action();
        match (action, self) {
            (SubmittedEvictionAction::RecycleLegacy, Self::Legacy(buffer)) => {
                legacy_pool.lock().unwrap().push(buffer);
            }
            (SubmittedEvictionAction::RetireFast, Self::Fast(buffer)) => buffer.retire(),
            _ => unreachable!("readback eviction action must match its buffer class"),
        }
    }
}

struct ImageCopier {
    config: CaptureConfig,
    src_image: Handle<Image>,
    queue: Mutex<
        VecDeque<(
            QueuedReadbackBuffer,
            Vec<DynSnapshotSync>,
            u32,
            u32,
            TextureFormat,
            Option<Instant>,
        )>,
    >,
    free_buffers: Arc<Mutex<Vec<Buffer>>>,
    fast_readback_pool: Arc<FastReadbackPool>,
    snapshots: Arc<Vec<ToSyncSnapshot>>,
}

impl ImageCopier {
    pub fn new(
        config: CaptureConfig,
        src_image: Handle<Image>,
        snapshots: Arc<Vec<ToSyncSnapshot>>,
    ) -> ImageCopier {
        ImageCopier {
            config,
            src_image,
            queue: Mutex::new(VecDeque::new()),
            free_buffers: Arc::new(Mutex::new(Vec::new())),
            fast_readback_pool: Arc::new(FastReadbackPool::default()),
            snapshots,
        }
    }

    fn acquire_buffer(&self, render_device: &RenderDevice, size: u64) -> Buffer {
        if let Some(buf) = self.free_buffers.lock().unwrap().pop() {
            return buf;
        }
        render_device.create_buffer(&BufferDescriptor {
            label: None,
            size,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }
}

fn unpad_rows(padded: &[u8], row_bytes: usize, aligned_row_bytes: usize, height: u32) -> Vec<u8> {
    if row_bytes == aligned_row_bytes {
        return padded.to_vec();
    }
    let mut out = Vec::with_capacity(row_bytes * height as usize);
    for row in padded.chunks(aligned_row_bytes).take(height as usize) {
        out.extend_from_slice(&row[..row_bytes.min(row.len())]);
    }
    out
}

fn unpad_rows_owned(
    padded: Vec<u8>,
    row_bytes: usize,
    aligned_row_bytes: usize,
    height: u32,
) -> Vec<u8> {
    if row_bytes == aligned_row_bytes {
        return padded;
    }
    unpad_rows(&padded, row_bytes, aligned_row_bytes, height)
}

fn native_rgba_frame_bytes(
    padded: Vec<u8>,
    width: u32,
    height: u32,
    format: TextureFormat,
) -> Option<Vec<u8>> {
    if !matches!(
        format,
        TextureFormat::Rgba8UnormSrgb | TextureFormat::Rgba8Unorm
    ) {
        return None;
    }
    let row_bytes = width as usize * 4;
    let aligned_row_bytes = RenderDevice::align_copy_bytes_per_row(row_bytes);
    Some(unpad_rows_owned(
        padded,
        row_bytes,
        aligned_row_bytes,
        height,
    ))
}

fn padded_rgba_to_rgb(
    padded: &[u8],
    width: u32,
    height: u32,
    format: TextureFormat,
) -> Option<Vec<u8>> {
    let pixel_size = format.pixel_size().ok()?;
    if pixel_size != 4 {
        return None;
    }
    let row_bytes = width as usize * pixel_size;
    let aligned_row_bytes = RenderDevice::align_copy_bytes_per_row(row_bytes);

    match format {
        TextureFormat::Bgra8UnormSrgb | TextureFormat::Bgra8Unorm => {
            let mut out = Vec::with_capacity(width as usize * height as usize * 3);
            for row in padded.chunks(aligned_row_bytes).take(height as usize) {
                let row = &row[..row_bytes.min(row.len())];
                for px in row.chunks_exact(4) {
                    out.extend_from_slice(&[px[2], px[1], px[0]]);
                }
            }
            Some(out)
        }
        TextureFormat::Rgba8UnormSrgb | TextureFormat::Rgba8Unorm => {
            let mut out = Vec::with_capacity(width as usize * height as usize * 3);
            for row in padded.chunks(aligned_row_bytes).take(height as usize) {
                let row = &row[..row_bytes.min(row.len())];
                for px in row.chunks_exact(4) {
                    out.extend_from_slice(&[px[0], px[1], px[2]]);
                }
            }
            Some(out)
        }
        _ => None,
    }
}

fn capture_texture_aspect(format: TextureFormat) -> TextureAspect {
    if matches!(
        format,
        TextureFormat::Depth16Unorm
            | TextureFormat::Depth24Plus
            | TextureFormat::Depth24PlusStencil8
            | TextureFormat::Depth32Float
            | TextureFormat::Depth32FloatStencil8
    ) {
        TextureAspect::DepthOnly
    } else {
        TextureAspect::All
    }
}

pub trait SnapshotSync: Send {
    /// Explicit pre-map capability for the bounded sole-consumer native-RGBA
    /// path. Existing file, RGB, depth, dataset, and multi-consumer handlers
    /// remain on the legacy callback-copy path by default.
    fn accepts_owned_rgba(&self) -> bool {
        false
    }

    fn captured(
        self: Box<Self>,
        world: &mut DeferredWorld,
        config: &CaptureConfig,
    ) -> Box<dyn SnapshotAsync>;
}

pub trait SnapshotAsync: Send {
    fn captured(&mut self, frame: CapturedFrame<'_>);

    /// Explicit capability gate for the sole-consumer native-RGBA ownership
    /// fast path. Existing RGB/depth/dataset/file handlers remain borrowed by
    /// default without source changes.
    fn accepts_owned_rgba(&self) -> bool {
        false
    }

    /// Takes ownership of an already mapped and unpadded native RGBA frame.
    /// Returning `Err(data)` declines ownership and requires the driver to call
    /// `captured` once with the returned allocation and the original metadata.
    fn captured_owned_rgba(
        &mut self,
        _width: u32,
        _height: u32,
        data: Vec<u8>,
    ) -> Result<(), Vec<u8>> {
        Err(data)
    }
}

pub trait GpuCaptureHandler: Send + Sync + 'static {
    fn captured(&self, world: &World) -> Option<Box<dyn SnapshotSync>>;
}

fn premap_owned_rgba_fast_path(
    frame_kind: CapturedFrameKind,
    width: u32,
    texture_format: TextureFormat,
    snapshots: &[DynSnapshotSync],
) -> bool {
    if frame_kind != CapturedFrameKind::Rgba8
        || !matches!(
            texture_format,
            TextureFormat::Rgba8UnormSrgb | TextureFormat::Rgba8Unorm
        )
        || snapshots.len() != 1
        || !snapshots[0].accepts_owned_rgba()
    {
        return false;
    }

    let row_bytes = width as usize * 4;
    RenderDevice::align_copy_bytes_per_row(row_bytes) == row_bytes
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FastMapStatus {
    Success,
    Error,
}

#[derive(Debug)]
enum MapReadbackCompletion {
    Legacy(Vec<u8>),
    Fast(FastMapStatus),
}

fn notify_fast_map_completion(
    map_succeeded: bool,
    sender: futures::channel::oneshot::Sender<MapReadbackCompletion>,
    copy_recorded_at: Option<Instant>,
) {
    if let Some(copy_recorded_at) = copy_recorded_at {
        let latency_ns = elapsed_ns(copy_recorded_at);
        P1_COPY_TO_MAP_CALLBACK_TOTAL.fetch_add(1, Ordering::Relaxed);
        record_duration(
            &P1_COPY_TO_MAP_CALLBACK_NS_TOTAL,
            &P1_COPY_TO_MAP_CALLBACK_NS_MAX,
            latency_ns,
        );
    }
    let started = Instant::now();
    CAPTURE_FAST_MAP_CALLBACK_TOTAL.fetch_add(1, Ordering::Relaxed);
    let status = if map_succeeded {
        CAPTURE_FAST_MAP_SUCCESS_TOTAL.fetch_add(1, Ordering::Relaxed);
        FastMapStatus::Success
    } else {
        CAPTURE_FAST_MAP_ERROR_TOTAL.fetch_add(1, Ordering::Relaxed);
        FastMapStatus::Error
    };

    // This function deliberately has no buffer or byte-slice argument: the
    // native callback publishes only map status and cannot copy/unmap/recycle.
    let _ = sender.send(MapReadbackCompletion::Fast(status));
    record_duration(
        &CAPTURE_FAST_MAP_CALLBACK_NS_TOTAL,
        &CAPTURE_FAST_MAP_CALLBACK_NS_MAX,
        elapsed_ns(started),
    );
}

fn copy_fast_mapped_bytes(mapped: &[u8]) -> Vec<u8> {
    let started = Instant::now();
    let bytes = mapped.to_vec();
    CAPTURE_FAST_MAPPED_COPY_TOTAL.fetch_add(1, Ordering::Relaxed);
    record_duration(
        &CAPTURE_FAST_MAPPED_COPY_NS_TOTAL,
        &CAPTURE_FAST_MAPPED_COPY_NS_MAX,
        elapsed_ns(started),
    );
    bytes
}

fn dispatch_captured_frame(
    frame_kind: CapturedFrameKind,
    width: u32,
    height: u32,
    frame_bytes: Vec<u8>,
    mut snapshots: Vec<Box<dyn SnapshotAsync>>,
) {
    if frame_kind == CapturedFrameKind::Rgba8
        && snapshots.len() == 1
        && snapshots[0].accepts_owned_rgba()
    {
        let mut snapshot = snapshots
            .pop()
            .expect("sole owned RGBA snapshot must exist");
        CAPTURE_OWNED_RGBA_ATTEMPT_TOTAL.fetch_add(1, Ordering::Relaxed);
        match snapshot.captured_owned_rgba(width, height, frame_bytes) {
            Ok(()) => {
                CAPTURE_OWNED_RGBA_CONSUMED_TOTAL.fetch_add(1, Ordering::Relaxed);
            }
            Err(frame_bytes) => {
                CAPTURE_OWNED_RGBA_FALLBACK_TOTAL.fetch_add(1, Ordering::Relaxed);
                CAPTURE_BORROWED_CALLBACK_TOTAL.fetch_add(1, Ordering::Relaxed);
                snapshot.captured(CapturedFrame {
                    kind: frame_kind,
                    width,
                    height,
                    data: frame_bytes.as_slice(),
                });
            }
        }
        return;
    }

    for mut snapshot in snapshots {
        CAPTURE_BORROWED_CALLBACK_TOTAL.fetch_add(1, Ordering::Relaxed);
        snapshot.captured(CapturedFrame {
            kind: frame_kind,
            width,
            height,
            data: frame_bytes.as_slice(),
        });
    }
}

fn image_copy_driver(world: &World, mut render_context: RenderContext) {
    let Some(copiers) = world.get_resource::<ImageCopiers>() else {
        return;
    };
    let Some(gpu_images) = world.get_resource::<RenderAssets<GpuImage>>() else {
        return;
    };

    for copier in copiers.iter() {
        let Some(src_image) = gpu_images.get(&copier.src_image) else {
            continue;
        };

        // Determine whether any consumer wants this frame before reserving GPU readback memory.
        let snapshot: Vec<DynSnapshotSync> = copier
            .snapshots
            .iter()
            .filter_map(|handler| handler.captured(world))
            .collect();
        if snapshot.is_empty() {
            continue;
        }

        let size = src_image.texture_descriptor.size;
        let format = src_image.texture_descriptor.format;
        let block_dimensions = format.block_dimensions();
        let block_size = format.block_copy_size(None).unwrap();
        let padded_bytes_per_row = RenderDevice::align_copy_bytes_per_row(
            (size.width as usize / block_dimensions.0 as usize) * block_size as usize,
        );
        let buffer_size = padded_bytes_per_row as u64 * size.height as u64;
        let fast_path =
            premap_owned_rgba_fast_path(copier.config.frame_kind, size.width, format, &snapshot);
        let readback_buffer = if fast_path {
            let Some(buffer) = copier
                .fast_readback_pool
                .try_acquire(render_context.render_device(), buffer_size)
            else {
                // The sequence/timestamp captured above is intentionally skipped.
                // Never block the renderer or allocate a fourth live staging buffer.
                continue;
            };
            QueuedReadbackBuffer::Fast(buffer)
        } else {
            QueuedReadbackBuffer::Legacy(
                copier.acquire_buffer(render_context.render_device(), buffer_size),
            )
        };

        // Unlike the legacy queue, a fast buffer must never be recycled after a
        // GPU copy has already been recorded. Reserve queue capacity first and
        // drop the current (not-yet-submitted) frame if both waiting slots are full.
        let mut fast_queue = if fast_path {
            let queue = copier.queue.lock().unwrap();
            if !fast_queue_admits_copy(queue.len()) {
                drop(queue);
                readback_buffer.recycle_unsubmitted(&copier.free_buffers);
                continue;
            }
            Some(queue)
        } else {
            None
        };

        render_context.command_encoder().copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &src_image.texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: capture_texture_aspect(format),
            },
            TexelCopyBufferInfo {
                buffer: readback_buffer.buffer(),
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(
                        std::num::NonZero::<u32>::new(padded_bytes_per_row as u32)
                            .unwrap()
                            .into(),
                    ),
                    rows_per_image: None,
                },
            },
            size,
        );
        CAPTURE_COPY_SUBMIT_TOTAL.fetch_add(1, Ordering::Relaxed);
        // This marks CPU command recording, not an invented GPU-pass boundary.
        // WGPU submits the encoder after this render system returns.
        let copy_recorded_at = (fast_path && p1_timing_enabled()).then(Instant::now);

        if let Some(queue) = fast_queue.as_mut() {
            queue.push_back((
                readback_buffer,
                snapshot,
                size.width,
                size.height,
                format,
                copy_recorded_at,
            ));
        } else {
            // Preserve the legacy latest-queued behavior exactly. Only the fast
            // ring uses pre-submit admission because its leases bound map/tasks.
            let mut queue = copier.queue.lock().unwrap();
            queue.push_back((
                readback_buffer,
                snapshot,
                size.width,
                size.height,
                format,
                copy_recorded_at,
            ));
            while queue.len() > MAX_QUEUED_FRAMES {
                if let Some((buffer, _, _, _, _, _)) = queue.pop_front() {
                    CAPTURE_QUEUE_DROP_TOTAL.fetch_add(1, Ordering::Relaxed);
                    buffer.release_submitted_eviction(&copier.free_buffers);
                }
            }
        }
    }
}

fn receive_image_from_buffer(mut world: DeferredWorld) {
    let copier_count = world.resource::<ImageCopiers>().len();
    if copier_count == 0 {
        return;
    }

    for idx in 0..copier_count {
        let next = {
            let copiers = world.resource::<ImageCopiers>();
            let Some(copier) = copiers.get(idx) else {
                continue;
            };
            let mut guard = copier.queue.lock().unwrap();
            guard.pop_front().map(
                |(buffer, snapshots, width, height, texture_format, copy_recorded_at)| {
                    (
                        buffer,
                        snapshots,
                        width,
                        height,
                        texture_format,
                        copy_recorded_at,
                        copier.free_buffers.clone(),
                        copier.config.clone(),
                    )
                },
            )
        };

        let Some((
            buffer,
            snapshots,
            width,
            height,
            texture_format,
            copy_recorded_at,
            free_buffers,
            config,
        )) = next
        else {
            continue;
        };
        let processing_guard = CaptureProcessingGuard::start();

        let (s, r) = futures::channel::oneshot::channel::<MapReadbackCompletion>();
        let fast_readback = match buffer {
            QueuedReadbackBuffer::Legacy(buffer) => {
                // Preserve the exact legacy callback-copy/unmap/pool behavior for
                // file, RGB, depth, dataset, zero/multiple, and declining handlers.
                let buffer_slice = buffer.slice(..);
                let buffer_for_map = buffer.clone();
                buffer_slice.map_async(MapMode::Read, move |res| {
                    res.expect("Failed to map buffer");
                    let buffer_slice = buffer_for_map.slice(..);
                    let data = buffer_slice.get_mapped_range();
                    let dat = data.to_vec();
                    drop(data);
                    buffer_for_map.unmap();
                    free_buffers.lock().unwrap().push(buffer_for_map);
                    s.send(MapReadbackCompletion::Legacy(dat))
                        .expect("Failed to send map update");
                });
                None
            }
            QueuedReadbackBuffer::Fast(buffer) => {
                let buffer_slice = buffer.buffer().slice(..);
                buffer_slice.map_async(MapMode::Read, move |res| {
                    notify_fast_map_completion(res.is_ok(), s, copy_recorded_at);
                });
                Some(buffer)
            }
        };

        let snapshots: Vec<Box<dyn SnapshotAsync>> = snapshots
            .into_iter()
            .map(|v| v.captured(&mut world, &config))
            .collect();
        let frame_kind = config.frame_kind;

        AsyncComputeTaskPool::get()
            .spawn(async move {
                let _processing_guard = processing_guard;
                let mut fast_readback = fast_readback;
                let padded = match r.await {
                    Ok(MapReadbackCompletion::Legacy(bytes)) => bytes,
                    Ok(MapReadbackCompletion::Fast(FastMapStatus::Success)) => {
                        let buffer = fast_readback
                            .take()
                            .expect("fast map completion must own one staging buffer");
                        let buffer_slice = buffer.buffer().slice(..);
                        let data = buffer_slice.get_mapped_range();
                        let bytes = copy_fast_mapped_bytes(&data);
                        drop(data);
                        buffer.buffer().unmap();
                        buffer.recycle();
                        bytes
                    }
                    Ok(MapReadbackCompletion::Fast(FastMapStatus::Error)) => {
                        fast_readback
                            .take()
                            .expect("failed fast map must own one staging buffer")
                            .retire();
                        warn!("Failed to map bounded fast capture buffer; dropping frame");
                        return;
                    }
                    Err(_) if fast_readback.is_none() => {
                        panic!("Failed to receive the map_async message");
                    }
                    Err(_) => {
                        CAPTURE_FAST_MAP_ERROR_TOTAL.fetch_add(1, Ordering::Relaxed);
                        if let Some(buffer) = fast_readback.take() {
                            buffer.retire();
                        }
                        warn!("Fast capture map callback was canceled; dropping frame");
                        return;
                    }
                };
                let frame_bytes = match frame_kind {
                    CapturedFrameKind::Rgb8 => {
                        padded_rgba_to_rgb(&padded, width, height, texture_format).unwrap_or_else(
                            || {
                                let pixel_size = texture_format
                                    .pixel_size()
                                    .expect("Unsupported capture texture format");
                                let row_bytes = width as usize * pixel_size;
                                let aligned_row_bytes =
                                    RenderDevice::align_copy_bytes_per_row(row_bytes);
                                let unpadded =
                                    unpad_rows(&padded, row_bytes, aligned_row_bytes, height);
                                let mut bevy_image = Image::new_target_texture(
                                    width,
                                    height,
                                    texture_format,
                                    Some(texture_format),
                                );
                                bevy_image.data = Some(unpadded);
                                bevy_image.try_into_dynamic().unwrap().to_rgb8().into_raw()
                            },
                        )
                    }
                    CapturedFrameKind::Rgba8 => {
                        native_rgba_frame_bytes(padded, width, height, texture_format)
                            .expect("RGBA capture requires an RGBA8 texture format")
                    }
                    CapturedFrameKind::Depth32F => {
                        let pixel_size = texture_format
                            .pixel_size()
                            .expect("Unsupported depth capture texture format");
                        let row_bytes = width as usize * pixel_size;
                        let aligned_row_bytes = RenderDevice::align_copy_bytes_per_row(row_bytes);
                        unpad_rows(&padded, row_bytes, aligned_row_bytes, height)
                    }
                };

                dispatch_captured_frame(frame_kind, width, height, frame_bytes, snapshots);
                CAPTURE_PROCESSING_COMPLETE_TOTAL.fetch_add(1, Ordering::Relaxed);
            })
            .detach();
    }
}

pub struct CameraCapturePlugin {
    config: CaptureConfig,
    snapshots: Arc<Vec<ToSyncSnapshot>>,
    handle: Handle<Image>,
    expose_config_resource: bool,
}

impl CameraCapturePlugin {
    pub fn new(
        app: &mut App,
        config: CaptureConfig,
        snapshots: Vec<ToSyncSnapshot>,
    ) -> (Self, Handle<Image>) {
        let handle = create_capture_image_handle(
            app,
            config.width,
            config.height,
            config.texture_format,
            RenderAssetUsages::default(),
            TextureUsages::COPY_SRC,
        );

        (
            Self {
                config,
                snapshots: Arc::new(snapshots),
                handle: handle.clone(),
                expose_config_resource: true,
            },
            handle,
        )
    }

    pub fn from_existing_handle(
        config: CaptureConfig,
        handle: Handle<Image>,
        snapshots: Vec<ToSyncSnapshot>,
    ) -> Self {
        Self {
            config,
            snapshots: Arc::new(snapshots),
            handle,
            expose_config_resource: false,
        }
    }
}

impl Plugin for CameraCapturePlugin {
    fn is_unique(&self) -> bool {
        false
    }

    fn build(&self, app: &mut App) {
        if self.expose_config_resource {
            app.insert_resource(self.config.clone());
        }

        let render_app = app.sub_app_mut(RenderApp);
        render_app.world_mut().init_resource::<ImageCopiers>();
        render_app
            .world_mut()
            .init_resource::<ImageCopyDriverInstalled>();

        {
            let mut copiers = render_app.world_mut().resource_mut::<ImageCopiers>();
            copiers.push(ImageCopier::new(
                self.config.clone(),
                self.handle.clone(),
                self.snapshots.clone(),
            ));
        }

        let installed = render_app.world().resource::<ImageCopyDriverInstalled>().0;
        if !installed {
            render_app.add_systems(
                RenderGraph,
                image_copy_driver
                    .after(camera_driver)
                    .in_set(RenderGraphSystems::Render),
            );

            render_app
                .world_mut()
                .resource_mut::<ImageCopyDriverInstalled>()
                .0 = true;
            render_app.add_systems(
                Render,
                receive_image_from_buffer.after(RenderSystems::Render),
            );
        }

        if self.expose_config_resource {
            render_app.insert_resource(self.config.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CaptureConfig, CapturedFrame, CapturedFrameKind, FastMapStatus, FastReadbackReservation,
        FastReadbackSlotState, MapReadbackCompletion, SnapshotAsync, SnapshotSync,
        SubmittedEvictionAction, capture_pipeline_counters, copy_fast_mapped_bytes,
        dispatch_captured_frame, fast_queue_admits_copy, native_rgba_frame_bytes,
        notify_fast_map_completion, premap_owned_rgba_fast_path, record_fast_no_buffer_drop,
    };
    use bevy::ecs::world::DeferredWorld;
    use bevy::render::render_resource::TextureFormat;
    use std::sync::{Arc, Mutex};

    static FAST_COUNTER_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[derive(Debug, Default)]
    struct CallbackRecord {
        owned_calls: usize,
        borrowed_calls: usize,
        owned_pointer: usize,
        owned_capacity: usize,
        borrowed_pointer: usize,
        borrowed_bytes: Vec<u8>,
    }

    struct RecordingSnapshot {
        accepts_owned_rgba: bool,
        decline_owned_rgba: bool,
        record: Arc<Mutex<CallbackRecord>>,
    }

    struct RecordingSync {
        accepts_owned_rgba: bool,
    }

    struct DefaultSync;

    impl SnapshotSync for DefaultSync {
        fn captured(
            self: Box<Self>,
            _world: &mut DeferredWorld,
            _config: &CaptureConfig,
        ) -> Box<dyn SnapshotAsync> {
            panic!("pre-map capability tests must not convert the sync snapshot")
        }
    }

    impl SnapshotSync for RecordingSync {
        fn accepts_owned_rgba(&self) -> bool {
            self.accepts_owned_rgba
        }

        fn captured(
            self: Box<Self>,
            _world: &mut DeferredWorld,
            _config: &CaptureConfig,
        ) -> Box<dyn SnapshotAsync> {
            panic!("pre-map capability tests must not convert the sync snapshot")
        }
    }

    impl RecordingSnapshot {
        fn new(
            accepts_owned_rgba: bool,
            decline_owned_rgba: bool,
        ) -> (Self, Arc<Mutex<CallbackRecord>>) {
            let record = Arc::new(Mutex::new(CallbackRecord::default()));
            (
                Self {
                    accepts_owned_rgba,
                    decline_owned_rgba,
                    record: record.clone(),
                },
                record,
            )
        }
    }

    impl SnapshotAsync for RecordingSnapshot {
        fn captured(&mut self, frame: CapturedFrame<'_>) {
            let mut record = self.record.lock().unwrap();
            record.borrowed_calls += 1;
            record.borrowed_pointer = frame.data.as_ptr() as usize;
            record.borrowed_bytes = frame.data.to_vec();
        }

        fn accepts_owned_rgba(&self) -> bool {
            self.accepts_owned_rgba
        }

        fn captured_owned_rgba(
            &mut self,
            _width: u32,
            _height: u32,
            data: Vec<u8>,
        ) -> Result<(), Vec<u8>> {
            let mut record = self.record.lock().unwrap();
            record.owned_calls += 1;
            record.owned_pointer = data.as_ptr() as usize;
            record.owned_capacity = data.capacity();
            if self.decline_owned_rgba {
                Err(data)
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn premap_fast_path_requires_one_opted_in_aligned_native_rgba_consumer() {
        let snapshots: Vec<Box<dyn SnapshotSync>> = vec![Box::new(RecordingSync {
            accepts_owned_rgba: true,
        })];
        assert!(premap_owned_rgba_fast_path(
            CapturedFrameKind::Rgba8,
            64,
            TextureFormat::Rgba8UnormSrgb,
            &snapshots,
        ));
        assert!(!premap_owned_rgba_fast_path(
            CapturedFrameKind::Rgb8,
            64,
            TextureFormat::Rgba8UnormSrgb,
            &snapshots,
        ));
        assert!(!premap_owned_rgba_fast_path(
            CapturedFrameKind::Rgba8,
            64,
            TextureFormat::Bgra8UnormSrgb,
            &snapshots,
        ));
        assert!(!premap_owned_rgba_fast_path(
            CapturedFrameKind::Rgba8,
            1,
            TextureFormat::Rgba8UnormSrgb,
            &snapshots,
        ));

        let default_off: Vec<Box<dyn SnapshotSync>> = vec![Box::new(DefaultSync)];
        assert!(!premap_owned_rgba_fast_path(
            CapturedFrameKind::Rgba8,
            64,
            TextureFormat::Rgba8UnormSrgb,
            &default_off,
        ));

        let declining: Vec<Box<dyn SnapshotSync>> = vec![Box::new(RecordingSync {
            accepts_owned_rgba: false,
        })];
        assert!(!premap_owned_rgba_fast_path(
            CapturedFrameKind::Rgba8,
            64,
            TextureFormat::Rgba8UnormSrgb,
            &declining,
        ));

        let multiple: Vec<Box<dyn SnapshotSync>> = vec![
            Box::new(RecordingSync {
                accepts_owned_rgba: true,
            }),
            Box::new(RecordingSync {
                accepts_owned_rgba: true,
            }),
        ];
        assert!(!premap_owned_rgba_fast_path(
            CapturedFrameKind::Rgba8,
            64,
            TextureFormat::Rgba8UnormSrgb,
            &multiple,
        ));
        assert!(!premap_owned_rgba_fast_path(
            CapturedFrameKind::Rgba8,
            64,
            TextureFormat::Rgba8UnormSrgb,
            &[],
        ));
    }

    #[test]
    fn fast_readback_slots_cap_three_and_the_fourth_request_is_counted_as_a_drop() {
        let _guard = FAST_COUNTER_TEST_LOCK.lock().unwrap();
        let counters_before = capture_pipeline_counters();
        let mut slots = FastReadbackSlotState::default();

        for _ in 0..3 {
            assert_eq!(
                slots.try_reserve(false),
                Some(FastReadbackReservation::Allocate)
            );
        }
        assert_eq!(slots.try_reserve(false), None);
        record_fast_no_buffer_drop();
        assert_eq!(slots.allocated, 3);
        assert_eq!(slots.in_flight, 3);
        assert_eq!(slots.max_in_flight, 3);

        slots.release_recycled();
        assert_eq!(
            slots.try_reserve(true),
            Some(FastReadbackReservation::Reuse)
        );
        assert_eq!(slots.allocated, 3);
        assert_eq!(slots.in_flight, 3);
        assert_eq!(slots.max_in_flight, 3);

        slots.release_retired();
        slots.release_retired();
        slots.release_retired();
        assert_eq!(slots.allocated, 0);
        assert_eq!(slots.in_flight, 0);
        assert_eq!(
            capture_pipeline_counters().fast_no_buffer_drop_total,
            counters_before.fast_no_buffer_drop_total + 1
        );
    }

    #[test]
    fn fast_queue_full_drop_is_counted_before_any_gpu_copy_submit() {
        let _guard = FAST_COUNTER_TEST_LOCK.lock().unwrap();
        let counters_before = capture_pipeline_counters();

        assert!(fast_queue_admits_copy(0));
        assert!(fast_queue_admits_copy(1));
        assert!(!fast_queue_admits_copy(2));

        let counters_after = capture_pipeline_counters();
        assert_eq!(
            counters_after.fast_queue_pre_submit_drop_total,
            counters_before.fast_queue_pre_submit_drop_total + 1
        );
        assert_eq!(
            counters_after.queue_drop_total,
            counters_before.queue_drop_total + 1
        );
        assert_eq!(
            counters_after.copy_submit_total,
            counters_before.copy_submit_total
        );
        assert_eq!(
            counters_after.fast_buffer_allocation_total,
            counters_before.fast_buffer_allocation_total
        );
    }

    #[test]
    fn submitted_fast_evicted_by_legacy_path_is_retired_and_cannot_be_reused() {
        assert_eq!(
            SubmittedEvictionAction::for_fast(false),
            SubmittedEvictionAction::RecycleLegacy
        );
        assert_eq!(
            SubmittedEvictionAction::for_fast(true),
            SubmittedEvictionAction::RetireFast
        );

        // Model the mixed fast -> legacy queue transition: the fast lease was
        // reserved and its GPU copy submitted before the legacy insertion
        // evicts it. Retirement closes both the in-flight lease and allocation
        // slot; it must not create an idle/reusable fast slot.
        let mut slots = FastReadbackSlotState::default();
        assert_eq!(
            slots.try_reserve(false),
            Some(FastReadbackReservation::Allocate)
        );
        assert_eq!((slots.allocated, slots.in_flight), (1, 1));
        slots.release_retired();
        assert_eq!((slots.allocated, slots.in_flight), (0, 0));

        // A following fast frame has to allocate a new slot. Claiming reuse is
        // rejected because retirement did not return a buffer to the pool.
        assert_eq!(
            slots.try_reserve(false),
            Some(FastReadbackReservation::Allocate)
        );
        assert_eq!((slots.allocated, slots.in_flight), (1, 1));
        slots.release_retired();
        assert_eq!((slots.allocated, slots.in_flight), (0, 0));
    }

    #[test]
    fn fast_map_callback_body_sends_only_status_and_never_runs_the_frame_copy() {
        let _guard = FAST_COUNTER_TEST_LOCK.lock().unwrap();
        let counters_before = capture_pipeline_counters();

        let (success_sender, success_receiver) = futures::channel::oneshot::channel();
        notify_fast_map_completion(true, success_sender, None);
        assert!(matches!(
            futures::executor::block_on(success_receiver).unwrap(),
            MapReadbackCompletion::Fast(FastMapStatus::Success)
        ));

        let (error_sender, error_receiver) = futures::channel::oneshot::channel();
        notify_fast_map_completion(false, error_sender, None);
        assert!(matches!(
            futures::executor::block_on(error_receiver).unwrap(),
            MapReadbackCompletion::Fast(FastMapStatus::Error)
        ));

        let counters_after = capture_pipeline_counters();
        assert_eq!(
            counters_after.fast_map_callback_total,
            counters_before.fast_map_callback_total + 2
        );
        assert_eq!(
            counters_after.fast_map_success_total,
            counters_before.fast_map_success_total + 1
        );
        assert_eq!(
            counters_after.fast_map_error_total,
            counters_before.fast_map_error_total + 1
        );
        assert_eq!(
            counters_after.fast_mapped_copy_total,
            counters_before.fast_mapped_copy_total
        );
    }

    #[test]
    fn fast_worker_copy_preserves_exact_bytes_and_counts_one_copy() {
        let _guard = FAST_COUNTER_TEST_LOCK.lock().unwrap();
        let counters_before = capture_pipeline_counters();
        let source = [1, 2, 3, 4, 5, 6, 7, 8];

        assert_eq!(copy_fast_mapped_bytes(&source), source);

        let counters_after = capture_pipeline_counters();
        assert_eq!(
            counters_after.fast_mapped_copy_total,
            counters_before.fast_mapped_copy_total + 1
        );
        assert_eq!(
            counters_after.fast_map_callback_total,
            counters_before.fast_map_callback_total
        );
    }

    #[test]
    fn native_rgba_capture_removes_only_row_padding_and_preserves_channels() {
        let mut padded = vec![0; 512];
        padded[..4].copy_from_slice(&[1, 2, 3, 4]);
        padded[256..260].copy_from_slice(&[5, 6, 7, 8]);
        assert_eq!(
            native_rgba_frame_bytes(padded, 1, 2, TextureFormat::Rgba8UnormSrgb).unwrap(),
            vec![1, 2, 3, 4, 5, 6, 7, 8]
        );
    }

    #[test]
    fn native_rgba_capture_rejects_bgra_to_avoid_silent_channel_drift() {
        assert!(
            native_rgba_frame_bytes(vec![0; 256], 1, 1, TextureFormat::Bgra8UnormSrgb).is_none()
        );
    }

    #[test]
    fn sole_opted_in_rgba_consumer_receives_the_same_vec_allocation_without_borrowing() {
        let mut bytes = Vec::with_capacity(32);
        bytes.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let pointer = bytes.as_ptr() as usize;
        let capacity = bytes.capacity();
        let (snapshot, record) = RecordingSnapshot::new(true, false);
        let counters_before = capture_pipeline_counters();

        dispatch_captured_frame(
            CapturedFrameKind::Rgba8,
            2,
            1,
            bytes,
            vec![Box::new(snapshot)],
        );

        let record = record.lock().unwrap();
        assert_eq!(record.owned_calls, 1);
        assert_eq!(record.borrowed_calls, 0);
        assert_eq!(record.owned_pointer, pointer);
        assert_eq!(record.owned_capacity, capacity);
        let counters_after = capture_pipeline_counters();
        assert!(
            counters_after.owned_rgba_attempt_total >= counters_before.owned_rgba_attempt_total + 1
        );
        assert!(
            counters_after.owned_rgba_consumed_total
                >= counters_before.owned_rgba_consumed_total + 1
        );
    }

    #[test]
    fn owned_rgba_decline_returns_the_same_allocation_to_one_borrowed_fallback() {
        let bytes = vec![9, 8, 7, 6, 5, 4, 3, 2];
        let pointer = bytes.as_ptr() as usize;
        let (snapshot, record) = RecordingSnapshot::new(true, true);
        let counters_before = capture_pipeline_counters();

        dispatch_captured_frame(
            CapturedFrameKind::Rgba8,
            2,
            1,
            bytes,
            vec![Box::new(snapshot)],
        );

        let record = record.lock().unwrap();
        assert_eq!(record.owned_calls, 1);
        assert_eq!(record.borrowed_calls, 1);
        assert_eq!(record.owned_pointer, pointer);
        assert_eq!(record.borrowed_pointer, pointer);
        assert_eq!(record.borrowed_bytes, [9, 8, 7, 6, 5, 4, 3, 2]);
        let counters_after = capture_pipeline_counters();
        assert!(
            counters_after.owned_rgba_fallback_total
                >= counters_before.owned_rgba_fallback_total + 1
        );
        assert!(
            counters_after.borrowed_callback_total >= counters_before.borrowed_callback_total + 1
        );
    }

    #[test]
    fn zero_multiple_unsupported_rgb_and_depth_consumers_stay_on_borrowed_callbacks() {
        dispatch_captured_frame(CapturedFrameKind::Rgba8, 1, 1, vec![0; 4], vec![]);

        let (first, first_record) = RecordingSnapshot::new(true, false);
        let (second, second_record) = RecordingSnapshot::new(false, false);
        dispatch_captured_frame(
            CapturedFrameKind::Rgba8,
            1,
            1,
            vec![1; 4],
            vec![Box::new(first), Box::new(second)],
        );
        assert_eq!(first_record.lock().unwrap().owned_calls, 0);
        assert_eq!(first_record.lock().unwrap().borrowed_calls, 1);
        assert_eq!(second_record.lock().unwrap().owned_calls, 0);
        assert_eq!(second_record.lock().unwrap().borrowed_calls, 1);

        for kind in [CapturedFrameKind::Rgb8, CapturedFrameKind::Depth32F] {
            let (snapshot, record) = RecordingSnapshot::new(true, false);
            dispatch_captured_frame(kind, 1, 1, vec![2; 4], vec![Box::new(snapshot)]);
            assert_eq!(record.lock().unwrap().owned_calls, 0);
            assert_eq!(record.lock().unwrap().borrowed_calls, 1);
        }

        let (unsupported, record) = RecordingSnapshot::new(false, false);
        dispatch_captured_frame(
            CapturedFrameKind::Rgba8,
            1,
            1,
            vec![3; 4],
            vec![Box::new(unsupported)],
        );
        assert_eq!(record.lock().unwrap().owned_calls, 0);
        assert_eq!(record.lock().unwrap().borrowed_calls, 1);
    }
}
