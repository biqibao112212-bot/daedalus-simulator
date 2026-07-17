use bevy::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::capture::driver::capture_pipeline_counters;
use crate::network_bridge::network_command_received_count;

static PHYSICS_STEP_TOTAL: AtomicU64 = AtomicU64::new(0);
static PHYSICS_STEP_NS_TOTAL: AtomicU64 = AtomicU64::new(0);
static PHYSICS_STEP_NS_MAX: AtomicU64 = AtomicU64::new(0);
static MAIN_SCHEDULE_TIMING_TOTAL: AtomicU64 = AtomicU64::new(0);
static MAIN_SCHEDULE_TIMING_NS_TOTAL: AtomicU64 = AtomicU64::new(0);
static MAIN_SCHEDULE_TIMING_NS_MAX: AtomicU64 = AtomicU64::new(0);

#[derive(Resource, Default)]
pub struct MainScheduleTiming {
    started_at: Option<Instant>,
}

#[derive(Resource, Default)]
pub struct PhysicsScheduleTiming {
    started_at: Option<Instant>,
}

fn p1_timing_enabled() -> bool {
    std::env::var("DAEDALUS_P1_TIMING")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

pub fn mark_main_schedule_begin(mut timing: ResMut<MainScheduleTiming>) {
    if p1_timing_enabled() {
        timing.started_at = Some(Instant::now());
    }
}

pub fn mark_main_schedule_end(mut timing: ResMut<MainScheduleTiming>) {
    let Some(started_at) = timing.started_at.take() else {
        return;
    };
    let elapsed_ns = started_at.elapsed().as_nanos().min(u64::MAX as u128) as u64;
    MAIN_SCHEDULE_TIMING_TOTAL.fetch_add(1, Ordering::Relaxed);
    MAIN_SCHEDULE_TIMING_NS_TOTAL.fetch_add(elapsed_ns, Ordering::Relaxed);
    MAIN_SCHEDULE_TIMING_NS_MAX.fetch_max(elapsed_ns, Ordering::Relaxed);
}

pub fn mark_physics_step_begin(mut timing: ResMut<PhysicsScheduleTiming>) {
    if p1_timing_enabled() {
        timing.started_at = Some(Instant::now());
    }
}

#[derive(Debug, Clone, Copy)]
pub enum FrequencyMetricKind {
    MainUpdate,
    SimCommandApply,
    GimbalUpdate,
}

#[derive(Debug, Clone, Copy, Default)]
struct RateCounter {
    initialized: bool,
    window_start_s: f64,
    window_count: u64,
    last_total: u64,
    hz: f64,
}

impl RateCounter {
    fn mark(&mut self, now_s: f64) {
        if !self.initialized {
            self.initialized = true;
            self.window_start_s = now_s;
            self.window_count = 0;
        }
        self.window_count = self.window_count.saturating_add(1);
        self.refresh_marked(now_s);
    }

    fn observe_total(&mut self, total: u64, now_s: f64) {
        if !self.initialized {
            self.initialized = true;
            self.window_start_s = now_s;
            self.last_total = total;
            return;
        }

        let elapsed = now_s - self.window_start_s;
        if elapsed >= 1.0 {
            let delta = total.saturating_sub(self.last_total);
            self.hz = delta as f64 / elapsed.max(f64::EPSILON);
            self.window_start_s = now_s;
            self.last_total = total;
        }
    }

    fn refresh_marked(&mut self, now_s: f64) {
        let elapsed = now_s - self.window_start_s;
        if elapsed >= 1.0 {
            self.hz = self.window_count as f64 / elapsed.max(f64::EPSILON);
            self.window_start_s = now_s;
            self.window_count = 0;
        }
    }
}

#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct FrequencyMetrics {
    main_update: RateCounter,
    main_update_total: u64,
    physics_step: RateCounter,
    preview_present: RateCounter,
    capture_copy_submit: RateCounter,
    talos_frame: RateCounter,
    talos_tcp_image_sent: RateCounter,
    network_command_receive: RateCounter,
    sim_command_apply: RateCounter,
    gimbal_update: RateCounter,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FrequencyMetricsSnapshot {
    pub talos_image_transport: &'static str,
    pub main_update_hz: f64,
    /// Compatibility alias for the historical metric, which is sampled from MainApp Update.
    pub render_fps: f64,
    /// Direct rate of completed Avian PhysicsSchedule executions.
    pub physics_step_hz: f64,
    pub preview_present_hz: f64,
    pub capture_copy_submit_hz: f64,
    pub talos_publish_hz: f64,
    pub talos_tcp_image_sent_hz: f64,
    /// Compatibility alias for successful Talos image publications.
    pub talos_frame_fps: f64,
    pub network_command_receive_hz: f64,
    pub sim_command_apply_hz: f64,
    pub gimbal_update_hz: f64,
    pub main_update_total: u64,
    pub main_schedule_timing_total: u64,
    pub main_schedule_timing_ns_total: u64,
    pub main_schedule_timing_ns_max: u64,
    pub physics_step_total: u64,
    pub physics_step_ns_total: u64,
    pub physics_step_ns_max: u64,
    pub preview_present_total: u64,
    pub capture_copy_submit_total: u64,
    pub capture_queue_drop_total: u64,
    pub capture_processing_complete_total: u64,
    pub capture_processing_in_flight: u64,
    pub capture_processing_max_in_flight: u64,
    pub capture_owned_rgba_attempt_total: u64,
    pub capture_owned_rgba_consumed_total: u64,
    pub capture_owned_rgba_fallback_total: u64,
    pub capture_borrowed_callback_total: u64,
    pub capture_fast_buffer_allocated: u64,
    pub capture_fast_buffer_allocation_total: u64,
    pub capture_fast_buffer_in_flight: u64,
    pub capture_fast_buffer_max_in_flight: u64,
    pub capture_fast_no_buffer_drop_total: u64,
    pub capture_fast_queue_pre_submit_drop_total: u64,
    pub capture_fast_map_callback_total: u64,
    pub capture_fast_map_success_total: u64,
    pub capture_fast_map_error_total: u64,
    pub capture_fast_mapped_copy_total: u64,
    pub capture_fast_map_callback_ns_total: u64,
    pub capture_fast_map_callback_ns_max: u64,
    pub capture_fast_submit_to_map_total: u64,
    pub capture_fast_submit_to_map_ns_total: u64,
    pub capture_fast_submit_to_map_ns_max: u64,
    pub capture_fast_mapped_copy_ns_total: u64,
    pub capture_fast_mapped_copy_ns_max: u64,
    pub talos_publish_total: u64,
    pub talos_publish_lock_drop_total: u64,
    pub talos_tcp_image_submit_total: u64,
    pub talos_tcp_image_sent_total: u64,
    pub talos_tcp_image_replaced_total: u64,
    pub talos_tcp_image_rejected_total: u64,
    pub talos_tcp_image_connect_total: u64,
    pub talos_tcp_image_disconnect_total: u64,
    pub talos_tcp_image_accept_fail_total: u64,
    pub talos_tcp_image_write_fail_total: u64,
    pub talos_tcp_image_write_timeout_total: u64,
    pub talos_tcp_image_write_connection_reset_total: u64,
    pub talos_tcp_image_write_broken_pipe_total: u64,
    pub talos_tcp_image_write_other_total: u64,
    pub talos_tcp_image_write_payload_bytes_before_error_total: u64,
    pub talos_tcp_image_last_write_error_kind: &'static str,
    pub talos_tcp_image_last_write_error_raw_os_code: i32,
    pub talos_tcp_image_last_write_payload_bytes_before_error: u64,
    pub talos_tcp_image_write_frame_duration_count: u64,
    pub talos_tcp_image_write_frame_duration_ns_total: u64,
    pub talos_tcp_image_write_frame_duration_ns_max: u64,
    pub talos_tcp_image_write_header_duration_count: u64,
    pub talos_tcp_image_write_header_duration_ns_total: u64,
    pub talos_tcp_image_write_header_duration_ns_max: u64,
    pub talos_tcp_image_write_payload_duration_count: u64,
    pub talos_tcp_image_write_payload_duration_ns_total: u64,
    pub talos_tcp_image_write_payload_duration_ns_max: u64,
    pub talos_tcp_image_write_would_block_total: u64,
    pub talos_tcp_image_write_retry_total: u64,
    pub talos_tcp_image_bind_fail_total: u64,
    pub talos_tcp_image_wire_bytes_sent_total: u64,
    pub talos_tcp_image_mailbox_current: u64,
    pub talos_tcp_image_mailbox_max: u64,
    pub talos_tcp_image_connected: bool,
    pub talos_tcp_image_latest_submitted_seq: u64,
    pub talos_tcp_image_latest_sent_seq: u64,
    pub talos_tcp_image_owned_submit_total: u64,
    pub talos_tcp_image_borrowed_submit_total: u64,
}

impl FrequencyMetrics {
    pub fn mark(&mut self, kind: FrequencyMetricKind, now_s: f64) {
        match kind {
            FrequencyMetricKind::MainUpdate => {
                self.main_update_total = self.main_update_total.saturating_add(1);
                self.main_update.mark(now_s);
            }
            FrequencyMetricKind::SimCommandApply => self.sim_command_apply.mark(now_s),
            FrequencyMetricKind::GimbalUpdate => self.gimbal_update.mark(now_s),
        }
    }

    pub fn snapshot(&self) -> FrequencyMetricsSnapshot {
        let capture = capture_pipeline_counters();
        let (talos_publish_total, talos_publish_lock_drop_total) = talos_direct_counters();
        let tcp = talos_tcp_image_direct_counters();
        FrequencyMetricsSnapshot {
            talos_image_transport: tcp.runtime_status.label(),
            main_update_hz: self.main_update.hz,
            render_fps: self.main_update.hz,
            physics_step_hz: self.physics_step.hz,
            preview_present_hz: self.preview_present.hz,
            capture_copy_submit_hz: self.capture_copy_submit.hz,
            talos_publish_hz: self.talos_frame.hz,
            talos_tcp_image_sent_hz: self.talos_tcp_image_sent.hz,
            talos_frame_fps: self.talos_frame.hz,
            network_command_receive_hz: self.network_command_receive.hz,
            sim_command_apply_hz: self.sim_command_apply.hz,
            gimbal_update_hz: self.gimbal_update.hz,
            main_update_total: self.main_update_total,
            main_schedule_timing_total: MAIN_SCHEDULE_TIMING_TOTAL.load(Ordering::Relaxed),
            main_schedule_timing_ns_total: MAIN_SCHEDULE_TIMING_NS_TOTAL.load(Ordering::Relaxed),
            main_schedule_timing_ns_max: MAIN_SCHEDULE_TIMING_NS_MAX.load(Ordering::Relaxed),
            physics_step_total: physics_step_total(),
            physics_step_ns_total: PHYSICS_STEP_NS_TOTAL.load(Ordering::Relaxed),
            physics_step_ns_max: PHYSICS_STEP_NS_MAX.load(Ordering::Relaxed),
            preview_present_total: crate::capture::preview_present_total(),
            capture_copy_submit_total: capture.copy_submit_total,
            capture_queue_drop_total: capture.queue_drop_total,
            capture_processing_complete_total: capture.processing_complete_total,
            capture_processing_in_flight: capture.processing_in_flight,
            capture_processing_max_in_flight: capture.processing_max_in_flight,
            capture_owned_rgba_attempt_total: capture.owned_rgba_attempt_total,
            capture_owned_rgba_consumed_total: capture.owned_rgba_consumed_total,
            capture_owned_rgba_fallback_total: capture.owned_rgba_fallback_total,
            capture_borrowed_callback_total: capture.borrowed_callback_total,
            capture_fast_buffer_allocated: capture.fast_buffer_allocated,
            capture_fast_buffer_allocation_total: capture.fast_buffer_allocation_total,
            capture_fast_buffer_in_flight: capture.fast_buffer_in_flight,
            capture_fast_buffer_max_in_flight: capture.fast_buffer_max_in_flight,
            capture_fast_no_buffer_drop_total: capture.fast_no_buffer_drop_total,
            capture_fast_queue_pre_submit_drop_total: capture.fast_queue_pre_submit_drop_total,
            capture_fast_map_callback_total: capture.fast_map_callback_total,
            capture_fast_map_success_total: capture.fast_map_success_total,
            capture_fast_map_error_total: capture.fast_map_error_total,
            capture_fast_mapped_copy_total: capture.fast_mapped_copy_total,
            capture_fast_map_callback_ns_total: capture.fast_map_callback_ns_total,
            capture_fast_map_callback_ns_max: capture.fast_map_callback_ns_max,
            capture_fast_submit_to_map_total: capture.fast_submit_to_map_total,
            capture_fast_submit_to_map_ns_total: capture.fast_submit_to_map_ns_total,
            capture_fast_submit_to_map_ns_max: capture.fast_submit_to_map_ns_max,
            capture_fast_mapped_copy_ns_total: capture.fast_mapped_copy_ns_total,
            capture_fast_mapped_copy_ns_max: capture.fast_mapped_copy_ns_max,
            talos_publish_total,
            talos_publish_lock_drop_total,
            talos_tcp_image_submit_total: tcp.submit_total,
            talos_tcp_image_sent_total: tcp.sent_total,
            talos_tcp_image_replaced_total: tcp.replaced_total,
            talos_tcp_image_rejected_total: tcp.rejected_total,
            talos_tcp_image_connect_total: tcp.connect_total,
            talos_tcp_image_disconnect_total: tcp.disconnect_total,
            talos_tcp_image_accept_fail_total: tcp.accept_fail_total,
            talos_tcp_image_write_fail_total: tcp.write_fail_total,
            talos_tcp_image_write_timeout_total: tcp.write_timeout_total,
            talos_tcp_image_write_connection_reset_total: tcp.write_connection_reset_total,
            talos_tcp_image_write_broken_pipe_total: tcp.write_broken_pipe_total,
            talos_tcp_image_write_other_total: tcp.write_other_total,
            talos_tcp_image_write_payload_bytes_before_error_total: tcp
                .write_payload_bytes_before_error_total,
            talos_tcp_image_last_write_error_kind: tcp.last_write_error_kind.label(),
            talos_tcp_image_last_write_error_raw_os_code: tcp.last_write_error_raw_os_code,
            talos_tcp_image_last_write_payload_bytes_before_error: tcp
                .last_write_payload_bytes_before_error,
            talos_tcp_image_write_frame_duration_count: tcp.write_frame_duration_count,
            talos_tcp_image_write_frame_duration_ns_total: tcp.write_frame_duration_ns_total,
            talos_tcp_image_write_frame_duration_ns_max: tcp.write_frame_duration_ns_max,
            talos_tcp_image_write_header_duration_count: tcp.write_header_duration_count,
            talos_tcp_image_write_header_duration_ns_total: tcp.write_header_duration_ns_total,
            talos_tcp_image_write_header_duration_ns_max: tcp.write_header_duration_ns_max,
            talos_tcp_image_write_payload_duration_count: tcp.write_payload_duration_count,
            talos_tcp_image_write_payload_duration_ns_total: tcp.write_payload_duration_ns_total,
            talos_tcp_image_write_payload_duration_ns_max: tcp.write_payload_duration_ns_max,
            talos_tcp_image_write_would_block_total: tcp.write_would_block_total,
            talos_tcp_image_write_retry_total: tcp.write_retry_total,
            talos_tcp_image_bind_fail_total: tcp.bind_fail_total,
            talos_tcp_image_wire_bytes_sent_total: tcp.wire_bytes_sent_total,
            talos_tcp_image_mailbox_current: tcp.mailbox_current,
            talos_tcp_image_mailbox_max: tcp.mailbox_max,
            talos_tcp_image_connected: tcp.connected,
            talos_tcp_image_latest_submitted_seq: tcp.latest_submitted_seq,
            talos_tcp_image_latest_sent_seq: tcp.latest_sent_seq,
            talos_tcp_image_owned_submit_total: tcp.owned_submit_total,
            talos_tcp_image_borrowed_submit_total: tcp.borrowed_submit_total,
        }
    }

    fn observe_monotonic_sources(&mut self, now_s: f64) {
        self.physics_step.observe_total(physics_step_total(), now_s);
        self.preview_present
            .observe_total(crate::capture::preview_present_total(), now_s);
        self.capture_copy_submit
            .observe_total(capture_pipeline_counters().copy_submit_total, now_s);
        self.network_command_receive
            .observe_total(network_command_received_count(), now_s);

        #[cfg(feature = "talos")]
        {
            self.talos_frame
                .observe_total(crate::talos::capture::talos_published_frames(), now_s);
            self.talos_tcp_image_sent.observe_total(
                crate::talos::tcp_image::tcp_image_sender_counters().sent_total,
                now_s,
            );
        }
    }
}

/// Marks one completed execution of Avian's inner PhysicsSchedule.
///
/// This system is installed after `PhysicsStepSystems::Last`, so paused or zero-delta
/// outer schedule passes are not counted as actual physics steps.
pub fn mark_physics_step(mut timing: ResMut<PhysicsScheduleTiming>) {
    PHYSICS_STEP_TOTAL.fetch_add(1, Ordering::Relaxed);
    if let Some(started_at) = timing.started_at.take() {
        let elapsed_ns = started_at.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        PHYSICS_STEP_NS_TOTAL.fetch_add(elapsed_ns, Ordering::Relaxed);
        PHYSICS_STEP_NS_MAX.fetch_max(elapsed_ns, Ordering::Relaxed);
    }
}

pub fn physics_step_total() -> u64 {
    PHYSICS_STEP_TOTAL.load(Ordering::Relaxed)
}

#[cfg(feature = "talos")]
fn talos_direct_counters() -> (u64, u64) {
    (
        crate::talos::capture::talos_published_frames(),
        crate::talos::capture::talos_publish_lock_drops(),
    )
}

#[cfg(feature = "talos")]
fn talos_tcp_image_direct_counters() -> crate::talos::tcp_image::TcpImageSenderCounters {
    crate::talos::tcp_image::tcp_image_sender_counters()
}

#[cfg(not(feature = "talos"))]
#[derive(Clone, Copy, Debug, Default)]
struct EmptyTcpImageCounters {
    submit_total: u64,
    sent_total: u64,
    replaced_total: u64,
    rejected_total: u64,
    connect_total: u64,
    disconnect_total: u64,
    accept_fail_total: u64,
    write_fail_total: u64,
    write_timeout_total: u64,
    write_connection_reset_total: u64,
    write_broken_pipe_total: u64,
    write_other_total: u64,
    write_payload_bytes_before_error_total: u64,
    last_write_error_kind: EmptyTcpImageWriteErrorKind,
    last_write_error_raw_os_code: i32,
    last_write_payload_bytes_before_error: u64,
    write_frame_duration_count: u64,
    write_frame_duration_ns_total: u64,
    write_frame_duration_ns_max: u64,
    write_header_duration_count: u64,
    write_header_duration_ns_total: u64,
    write_header_duration_ns_max: u64,
    write_payload_duration_count: u64,
    write_payload_duration_ns_total: u64,
    write_payload_duration_ns_max: u64,
    write_would_block_total: u64,
    write_retry_total: u64,
    bind_fail_total: u64,
    wire_bytes_sent_total: u64,
    mailbox_current: u64,
    mailbox_max: u64,
    connected: bool,
    latest_submitted_seq: u64,
    latest_sent_seq: u64,
    owned_submit_total: u64,
    borrowed_submit_total: u64,
    runtime_status: EmptyTcpImageRuntimeStatus,
}

#[cfg(not(feature = "talos"))]
#[derive(Clone, Copy, Debug, Default)]
struct EmptyTcpImageRuntimeStatus;

#[cfg(not(feature = "talos"))]
impl EmptyTcpImageRuntimeStatus {
    fn label(self) -> &'static str {
        "unavailable"
    }
}

#[cfg(not(feature = "talos"))]
#[derive(Clone, Copy, Debug, Default)]
struct EmptyTcpImageWriteErrorKind;

#[cfg(not(feature = "talos"))]
impl EmptyTcpImageWriteErrorKind {
    fn label(self) -> &'static str {
        "unavailable"
    }
}

#[cfg(not(feature = "talos"))]
fn talos_tcp_image_direct_counters() -> EmptyTcpImageCounters {
    EmptyTcpImageCounters::default()
}

#[cfg(not(feature = "talos"))]
fn talos_direct_counters() -> (u64, u64) {
    (0, 0)
}

pub fn update_frequency_metrics(time: Res<Time>, mut metrics: ResMut<FrequencyMetrics>) {
    let now_s = time.elapsed_secs_f64();
    metrics.mark(FrequencyMetricKind::MainUpdate, now_s);
    metrics.observe_monotonic_sources(now_s);
}

#[cfg(test)]
mod tests {
    use super::{
        FrequencyMetricKind, FrequencyMetrics, PhysicsScheduleTiming, RateCounter,
        mark_physics_step, physics_step_total,
    };
    use bevy::prelude::{App, Update};

    #[test]
    fn main_update_total_is_direct_and_monotonic() {
        let mut metrics = FrequencyMetrics::default();
        metrics.mark(FrequencyMetricKind::MainUpdate, 0.0);
        metrics.mark(FrequencyMetricKind::MainUpdate, 0.5);
        metrics.mark(FrequencyMetricKind::MainUpdate, 1.0);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.main_update_total, 3);
        assert_eq!(snapshot.main_update_hz, 3.0);
        assert_eq!(snapshot.render_fps, snapshot.main_update_hz);
    }

    #[test]
    fn total_observation_uses_counter_delta_over_the_window() {
        let mut counter = RateCounter::default();
        counter.observe_total(10, 0.0);
        counter.observe_total(15, 1.0);
        assert_eq!(counter.hz, 5.0);
    }

    #[test]
    fn physics_step_total_is_direct_and_monotonic() {
        let before = physics_step_total();
        let mut app = App::new();
        app.init_resource::<PhysicsScheduleTiming>()
            .add_systems(Update, mark_physics_step);
        app.update();
        assert_eq!(physics_step_total(), before + 1);
    }
}
