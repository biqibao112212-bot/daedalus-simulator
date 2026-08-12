use std::fmt;
use std::io::{self, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::corner_labels::{CornerLabelFrame, CornerLabelJsonlWriter};
use bevy::log::error;

pub const MAGIC: u32 = 0x5449_4d47;
pub const VERSION: u16 = 1;
pub const HEADER_BYTES: usize = 64;
pub const MAX_WIDTH: u32 = 1440;
pub const MAX_HEIGHT: u32 = 1080;

const DEFAULT_ACCEPT_POLL: Duration = Duration::from_millis(10);
pub const DEFAULT_WRITE_TIMEOUT: Duration = Duration::from_millis(250);
const NONBLOCKING_WRITE_RETRY_POLL: Duration = Duration::from_millis(1);

static RUNTIME_STATUS: AtomicU8 = AtomicU8::new(TcpImageRuntimeStatus::File as u8);
static BIND_FAIL_TOTAL: AtomicU64 = AtomicU64::new(0);

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TcpImageRuntimeStatus {
    #[default]
    File = 0,
    TcpListening = 1,
    TcpBindFailed = 2,
}

/// Bounded telemetry category for the most recent sender-side write error.
/// The raw OS code is exported separately because Windows and Unix use
/// different numeric socket errors.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TcpImageWriteErrorKind {
    #[default]
    None = 0,
    TimedOut = 1,
    ConnectionReset = 2,
    BrokenPipe = 3,
    Other = 4,
}

impl TcpImageWriteErrorKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::TimedOut => "timed_out",
            Self::ConnectionReset => "connection_reset",
            Self::BrokenPipe => "broken_pipe",
            Self::Other => "other",
        }
    }

    fn from_error(error: &io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::TimedOut => Self::TimedOut,
            io::ErrorKind::ConnectionReset => Self::ConnectionReset,
            io::ErrorKind::BrokenPipe => Self::BrokenPipe,
            _ => Self::Other,
        }
    }

    fn load(value: u8) -> Self {
        match value {
            1 => Self::TimedOut,
            2 => Self::ConnectionReset,
            3 => Self::BrokenPipe,
            4 => Self::Other,
            _ => Self::None,
        }
    }
}

impl TcpImageRuntimeStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::TcpListening => "tcp",
            Self::TcpBindFailed => "tcp_bind_failed",
        }
    }

    fn load() -> Self {
        match RUNTIME_STATUS.load(Ordering::Relaxed) {
            1 => Self::TcpListening,
            2 => Self::TcpBindFailed,
            _ => Self::File,
        }
    }
}

pub(crate) fn mark_file_image_transport() {
    RUNTIME_STATUS.store(TcpImageRuntimeStatus::File as u8, Ordering::Relaxed);
}

fn record_tcp_start_failure() {
    BIND_FAIL_TOTAL.fetch_add(1, Ordering::Relaxed);
    RUNTIME_STATUS.store(
        TcpImageRuntimeStatus::TcpBindFailed as u8,
        Ordering::Relaxed,
    );
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Rgb24 = 1,
    Rgba32 = 2,
}

impl PixelFormat {
    fn channels(self) -> u32 {
        match self {
            Self::Rgb24 => 3,
            Self::Rgba32 => 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TcpImageError {
    InvalidDimensions { width: u32, height: u32 },
    InvalidIdentity,
    PayloadSizeOverflow,
    PayloadSizeMismatch { expected: usize, actual: usize },
}

impl fmt::Display for TcpImageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimensions { width, height } => write!(
                formatter,
                "image dimensions {width}x{height} are outside 1..={MAX_WIDTH} x 1..={MAX_HEIGHT}"
            ),
            Self::InvalidIdentity => {
                formatter.write_str("producer epoch and sequence must both be nonzero")
            }
            Self::PayloadSizeOverflow => formatter.write_str("image payload size overflow"),
            Self::PayloadSizeMismatch { expected, actual } => write!(
                formatter,
                "image payload size mismatch: expected {expected}, got {actual}"
            ),
        }
    }
}

impl std::error::Error for TcpImageError {}

fn checked_payload_bytes(
    format: PixelFormat,
    width: u32,
    height: u32,
) -> Result<u32, TcpImageError> {
    if width == 0 || height == 0 || width > MAX_WIDTH || height > MAX_HEIGHT {
        return Err(TcpImageError::InvalidDimensions { width, height });
    }

    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(format.channels()))
        .ok_or(TcpImageError::PayloadSizeOverflow)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TcpImageHeader {
    pub format: PixelFormat,
    pub width: u32,
    pub height: u32,
    pub payload_bytes: u32,
    pub producer_epoch: u64,
    pub sequence: u64,
    pub capture_timestamp_ns: u64,
}

impl TcpImageHeader {
    pub fn new(
        format: PixelFormat,
        width: u32,
        height: u32,
        producer_epoch: u64,
        sequence: u64,
        capture_timestamp_ns: u64,
    ) -> Result<Self, TcpImageError> {
        if producer_epoch == 0 || sequence == 0 {
            return Err(TcpImageError::InvalidIdentity);
        }
        let payload_bytes = checked_payload_bytes(format, width, height)?;
        Ok(Self {
            format,
            width,
            height,
            payload_bytes,
            producer_epoch,
            sequence,
            capture_timestamp_ns,
        })
    }

    pub fn encode(self) -> [u8; HEADER_BYTES] {
        let mut bytes = [0; HEADER_BYTES];
        bytes[0..4].copy_from_slice(&MAGIC.to_be_bytes());
        bytes[4..6].copy_from_slice(&VERSION.to_be_bytes());
        bytes[6..8].copy_from_slice(&(HEADER_BYTES as u16).to_be_bytes());
        bytes[8..10].copy_from_slice(&(self.format as u16).to_be_bytes());
        // flags at 10..12 and both reserved u64 fields remain zero.
        bytes[12..16].copy_from_slice(&self.width.to_be_bytes());
        bytes[16..20].copy_from_slice(&self.height.to_be_bytes());
        bytes[20..24].copy_from_slice(&self.payload_bytes.to_be_bytes());
        bytes[24..32].copy_from_slice(&self.producer_epoch.to_be_bytes());
        bytes[32..40].copy_from_slice(&self.sequence.to_be_bytes());
        bytes[40..48].copy_from_slice(&self.capture_timestamp_ns.to_be_bytes());
        bytes
    }
}

#[derive(Debug)]
pub struct TcpImageFrame {
    pub header: TcpImageHeader,
    pub payload: Vec<u8>,
    pub(crate) corner_labels: Option<CornerLabelFrame>,
}

/// Validation failure for an ownership-transfer frame. The original allocation
/// is returned intact so the capture driver can invoke the legacy borrowed
/// callback exactly once.
#[derive(Debug)]
pub struct OwnedTcpImageError {
    error: TcpImageError,
    payload: Vec<u8>,
}

impl OwnedTcpImageError {
    fn new(error: TcpImageError, payload: Vec<u8>) -> Self {
        Self { error, payload }
    }

    pub fn error(&self) -> &TcpImageError {
        &self.error
    }

    pub fn into_parts(self) -> (TcpImageError, Vec<u8>) {
        (self.error, self.payload)
    }
}

impl TcpImageFrame {
    pub fn from_slice(
        format: PixelFormat,
        width: u32,
        height: u32,
        producer_epoch: u64,
        sequence: u64,
        capture_timestamp_ns: u64,
        payload: &[u8],
    ) -> Result<Self, TcpImageError> {
        let header = TcpImageHeader::new(
            format,
            width,
            height,
            producer_epoch,
            sequence,
            capture_timestamp_ns,
        )?;
        let expected = header.payload_bytes as usize;
        if payload.len() != expected {
            return Err(TcpImageError::PayloadSizeMismatch {
                expected,
                actual: payload.len(),
            });
        }
        Ok(Self {
            header,
            payload: payload.to_vec(),
            corner_labels: None,
        })
    }

    pub fn from_owned(
        format: PixelFormat,
        width: u32,
        height: u32,
        producer_epoch: u64,
        sequence: u64,
        capture_timestamp_ns: u64,
        payload: Vec<u8>,
    ) -> Result<Self, OwnedTcpImageError> {
        let header = match TcpImageHeader::new(
            format,
            width,
            height,
            producer_epoch,
            sequence,
            capture_timestamp_ns,
        ) {
            Ok(header) => header,
            Err(error) => return Err(OwnedTcpImageError::new(error, payload)),
        };
        let expected = header.payload_bytes as usize;
        if payload.len() != expected {
            return Err(OwnedTcpImageError::new(
                TcpImageError::PayloadSizeMismatch {
                    expected,
                    actual: payload.len(),
                },
                payload,
            ));
        }
        Ok(Self {
            header,
            payload,
            corner_labels: None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitOutcome {
    Accepted,
    Replaced,
    Rejected,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TcpImageSenderCounters {
    pub submit_total: u64,
    pub owned_submit_total: u64,
    pub borrowed_submit_total: u64,
    pub sent_total: u64,
    pub replaced_total: u64,
    pub rejected_total: u64,
    pub connect_total: u64,
    pub disconnect_total: u64,
    pub accept_fail_total: u64,
    pub write_fail_total: u64,
    pub write_timeout_total: u64,
    pub write_connection_reset_total: u64,
    pub write_broken_pipe_total: u64,
    pub write_other_total: u64,
    pub write_payload_bytes_before_error_total: u64,
    pub last_write_error_kind: TcpImageWriteErrorKind,
    /// Zero means that the most recent error did not expose a platform raw code.
    pub last_write_error_raw_os_code: i32,
    pub last_write_payload_bytes_before_error: u64,
    pub write_frame_duration_count: u64,
    pub write_frame_duration_ns_total: u64,
    pub write_frame_duration_ns_max: u64,
    pub write_header_duration_count: u64,
    pub write_header_duration_ns_total: u64,
    pub write_header_duration_ns_max: u64,
    pub write_payload_duration_count: u64,
    pub write_payload_duration_ns_total: u64,
    pub write_payload_duration_ns_max: u64,
    pub write_would_block_total: u64,
    pub write_retry_total: u64,
    pub bind_fail_total: u64,
    pub wire_bytes_sent_total: u64,
    pub mailbox_current: u64,
    pub mailbox_max: u64,
    pub connected: bool,
    pub latest_submitted_seq: u64,
    pub latest_sent_seq: u64,
    pub runtime_status: TcpImageRuntimeStatus,
}

#[derive(Default)]
struct AtomicCounters {
    submit_total: AtomicU64,
    owned_submit_total: AtomicU64,
    borrowed_submit_total: AtomicU64,
    sent_total: AtomicU64,
    replaced_total: AtomicU64,
    rejected_total: AtomicU64,
    connect_total: AtomicU64,
    disconnect_total: AtomicU64,
    accept_fail_total: AtomicU64,
    write_fail_total: AtomicU64,
    write_timeout_total: AtomicU64,
    write_connection_reset_total: AtomicU64,
    write_broken_pipe_total: AtomicU64,
    write_other_total: AtomicU64,
    write_payload_bytes_before_error_total: AtomicU64,
    last_write_error_kind: AtomicU8,
    last_write_error_raw_os_code: AtomicI32,
    last_write_payload_bytes_before_error: AtomicU64,
    write_frame_duration_count: AtomicU64,
    write_frame_duration_ns_total: AtomicU64,
    write_frame_duration_ns_max: AtomicU64,
    write_header_duration_count: AtomicU64,
    write_header_duration_ns_total: AtomicU64,
    write_header_duration_ns_max: AtomicU64,
    write_payload_duration_count: AtomicU64,
    write_payload_duration_ns_total: AtomicU64,
    write_payload_duration_ns_max: AtomicU64,
    write_would_block_total: AtomicU64,
    write_retry_total: AtomicU64,
    wire_bytes_sent_total: AtomicU64,
    mailbox_current: AtomicU64,
    mailbox_max: AtomicU64,
    connected: AtomicBool,
    latest_submitted_seq: AtomicU64,
    latest_sent_seq: AtomicU64,
}

impl AtomicCounters {
    fn snapshot(&self) -> TcpImageSenderCounters {
        TcpImageSenderCounters {
            submit_total: self.submit_total.load(Ordering::Relaxed),
            owned_submit_total: self.owned_submit_total.load(Ordering::Relaxed),
            borrowed_submit_total: self.borrowed_submit_total.load(Ordering::Relaxed),
            sent_total: self.sent_total.load(Ordering::Relaxed),
            replaced_total: self.replaced_total.load(Ordering::Relaxed),
            rejected_total: self.rejected_total.load(Ordering::Relaxed),
            connect_total: self.connect_total.load(Ordering::Relaxed),
            disconnect_total: self.disconnect_total.load(Ordering::Relaxed),
            accept_fail_total: self.accept_fail_total.load(Ordering::Relaxed),
            write_fail_total: self.write_fail_total.load(Ordering::Relaxed),
            write_timeout_total: self.write_timeout_total.load(Ordering::Relaxed),
            write_connection_reset_total: self.write_connection_reset_total.load(Ordering::Relaxed),
            write_broken_pipe_total: self.write_broken_pipe_total.load(Ordering::Relaxed),
            write_other_total: self.write_other_total.load(Ordering::Relaxed),
            write_payload_bytes_before_error_total: self
                .write_payload_bytes_before_error_total
                .load(Ordering::Relaxed),
            last_write_error_kind: TcpImageWriteErrorKind::load(
                self.last_write_error_kind.load(Ordering::Relaxed),
            ),
            last_write_error_raw_os_code: self.last_write_error_raw_os_code.load(Ordering::Relaxed),
            last_write_payload_bytes_before_error: self
                .last_write_payload_bytes_before_error
                .load(Ordering::Relaxed),
            write_frame_duration_count: self.write_frame_duration_count.load(Ordering::Relaxed),
            write_frame_duration_ns_total: self
                .write_frame_duration_ns_total
                .load(Ordering::Relaxed),
            write_frame_duration_ns_max: self.write_frame_duration_ns_max.load(Ordering::Relaxed),
            write_header_duration_count: self.write_header_duration_count.load(Ordering::Relaxed),
            write_header_duration_ns_total: self
                .write_header_duration_ns_total
                .load(Ordering::Relaxed),
            write_header_duration_ns_max: self.write_header_duration_ns_max.load(Ordering::Relaxed),
            write_payload_duration_count: self.write_payload_duration_count.load(Ordering::Relaxed),
            write_payload_duration_ns_total: self
                .write_payload_duration_ns_total
                .load(Ordering::Relaxed),
            write_payload_duration_ns_max: self
                .write_payload_duration_ns_max
                .load(Ordering::Relaxed),
            write_would_block_total: self.write_would_block_total.load(Ordering::Relaxed),
            write_retry_total: self.write_retry_total.load(Ordering::Relaxed),
            bind_fail_total: BIND_FAIL_TOTAL.load(Ordering::Relaxed),
            wire_bytes_sent_total: self.wire_bytes_sent_total.load(Ordering::Relaxed),
            mailbox_current: self.mailbox_current.load(Ordering::Relaxed),
            mailbox_max: self.mailbox_max.load(Ordering::Relaxed),
            connected: self.connected.load(Ordering::Relaxed),
            latest_submitted_seq: self.latest_submitted_seq.load(Ordering::Relaxed),
            latest_sent_seq: self.latest_sent_seq.load(Ordering::Relaxed),
            runtime_status: TcpImageRuntimeStatus::load(),
        }
    }
}

#[derive(Default)]
struct MailboxState {
    latest_sequence: u64,
    frame: Option<TcpImageFrame>,
}

pub struct LatestFrameMailbox {
    producer_epoch: u64,
    corner_label_writer: Option<Arc<CornerLabelJsonlWriter>>,
    state: Mutex<MailboxState>,
    wake: Condvar,
    stopping: AtomicBool,
    counters: AtomicCounters,
}

impl LatestFrameMailbox {
    pub fn new(producer_epoch: u64) -> Self {
        Self::with_corner_label_writer(producer_epoch, None)
    }

    fn with_corner_label_writer(
        producer_epoch: u64,
        corner_label_writer: Option<Arc<CornerLabelJsonlWriter>>,
    ) -> Self {
        assert_ne!(
            producer_epoch, 0,
            "TCP image producer epoch must be nonzero"
        );
        Self {
            producer_epoch,
            corner_label_writer,
            state: Mutex::new(MailboxState::default()),
            wake: Condvar::new(),
            stopping: AtomicBool::new(false),
            counters: AtomicCounters::default(),
        }
    }

    pub fn submit(&self, frame: TcpImageFrame) -> SubmitOutcome {
        self.counters.submit_total.fetch_add(1, Ordering::Relaxed);
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.stopping.load(Ordering::Acquire)
            || frame.header.producer_epoch != self.producer_epoch
            || frame.header.sequence <= state.latest_sequence
        {
            self.counters.rejected_total.fetch_add(1, Ordering::Relaxed);
            return SubmitOutcome::Rejected;
        }

        let outcome = if state.frame.is_some() {
            self.counters.replaced_total.fetch_add(1, Ordering::Relaxed);
            SubmitOutcome::Replaced
        } else {
            SubmitOutcome::Accepted
        };
        state.latest_sequence = frame.header.sequence;
        self.counters
            .latest_submitted_seq
            .store(frame.header.sequence, Ordering::Relaxed);
        state.frame = Some(frame);
        self.counters.mailbox_current.store(1, Ordering::Relaxed);
        self.counters.mailbox_max.fetch_max(1, Ordering::Relaxed);
        drop(state);
        self.wake.notify_one();
        outcome
    }

    fn record_rejected_attempt(&self) {
        self.counters.submit_total.fetch_add(1, Ordering::Relaxed);
        self.counters.rejected_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn take_latest(&self) -> Option<TcpImageFrame> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let frame = state.frame.take();
        if frame.is_some() {
            self.counters.mailbox_current.store(0, Ordering::Relaxed);
        }
        frame
    }

    fn wait_for_signal(&self, timeout: Duration) -> MailboxWaitOutcome {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.stopping.load(Ordering::Acquire) {
            return MailboxWaitOutcome::Stopping;
        }
        if state.frame.is_some() {
            return MailboxWaitOutcome::FrameAvailable;
        }
        let (state, wait_result) = self
            .wake
            .wait_timeout_while(state, timeout, |state| {
                !self.stopping.load(Ordering::Acquire) && state.frame.is_none()
            })
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.stopping.load(Ordering::Acquire) {
            MailboxWaitOutcome::Stopping
        } else if state.frame.is_some() {
            MailboxWaitOutcome::FrameAvailable
        } else {
            debug_assert!(wait_result.timed_out());
            MailboxWaitOutcome::TimedOut
        }
    }

    fn stop(&self) {
        self.stopping.store(true, Ordering::Release);
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.frame = None;
        self.counters.mailbox_current.store(0, Ordering::Relaxed);
        drop(state);
        self.wake.notify_all();
    }

    fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::Acquire)
    }

    pub fn counters(&self) -> TcpImageSenderCounters {
        self.counters.snapshot()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MailboxWaitOutcome {
    FrameAvailable,
    Stopping,
    TimedOut,
}

#[derive(Clone)]
pub struct TcpImagePublisher {
    mailbox: Arc<LatestFrameMailbox>,
}

impl TcpImagePublisher {
    pub fn submit_rgba32(
        &self,
        width: u32,
        height: u32,
        sequence: u64,
        capture_timestamp_ns: u64,
        payload: &[u8],
    ) -> Result<SubmitOutcome, TcpImageError> {
        self.submit(
            PixelFormat::Rgba32,
            width,
            height,
            sequence,
            capture_timestamp_ns,
            payload,
        )
    }

    pub fn submit_rgba32_owned(
        &self,
        width: u32,
        height: u32,
        sequence: u64,
        capture_timestamp_ns: u64,
        payload: Vec<u8>,
    ) -> Result<SubmitOutcome, OwnedTcpImageError> {
        self.submit_owned_with_corner_labels(
            PixelFormat::Rgba32,
            width,
            height,
            sequence,
            capture_timestamp_ns,
            payload,
            None,
        )
    }

    pub(crate) fn submit_rgba32_with_corner_labels(
        &self,
        width: u32,
        height: u32,
        sequence: u64,
        capture_timestamp_ns: u64,
        payload: &[u8],
        corner_labels: Option<CornerLabelFrame>,
    ) -> Result<SubmitOutcome, TcpImageError> {
        self.submit_with_corner_labels(
            PixelFormat::Rgba32,
            width,
            height,
            sequence,
            capture_timestamp_ns,
            payload,
            corner_labels,
        )
    }

    pub(crate) fn submit_rgba32_owned_with_corner_labels(
        &self,
        width: u32,
        height: u32,
        sequence: u64,
        capture_timestamp_ns: u64,
        payload: Vec<u8>,
        corner_labels: Option<CornerLabelFrame>,
    ) -> Result<SubmitOutcome, OwnedTcpImageError> {
        self.submit_owned_with_corner_labels(
            PixelFormat::Rgba32,
            width,
            height,
            sequence,
            capture_timestamp_ns,
            payload,
            corner_labels,
        )
    }

    pub fn submit_owned(
        &self,
        format: PixelFormat,
        width: u32,
        height: u32,
        sequence: u64,
        capture_timestamp_ns: u64,
        payload: Vec<u8>,
    ) -> Result<SubmitOutcome, OwnedTcpImageError> {
        self.submit_owned_with_corner_labels(
            format,
            width,
            height,
            sequence,
            capture_timestamp_ns,
            payload,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn submit_owned_with_corner_labels(
        &self,
        format: PixelFormat,
        width: u32,
        height: u32,
        sequence: u64,
        capture_timestamp_ns: u64,
        payload: Vec<u8>,
        corner_labels: Option<CornerLabelFrame>,
    ) -> Result<SubmitOutcome, OwnedTcpImageError> {
        let mut frame = TcpImageFrame::from_owned(
            format,
            width,
            height,
            self.mailbox.producer_epoch,
            sequence,
            capture_timestamp_ns,
            payload,
        )?;
        frame.corner_labels = corner_labels;
        self.mailbox
            .counters
            .owned_submit_total
            .fetch_add(1, Ordering::Relaxed);
        Ok(self.mailbox.submit(frame))
    }

    pub fn submit(
        &self,
        format: PixelFormat,
        width: u32,
        height: u32,
        sequence: u64,
        capture_timestamp_ns: u64,
        payload: &[u8],
    ) -> Result<SubmitOutcome, TcpImageError> {
        self.submit_with_corner_labels(
            format,
            width,
            height,
            sequence,
            capture_timestamp_ns,
            payload,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn submit_with_corner_labels(
        &self,
        format: PixelFormat,
        width: u32,
        height: u32,
        sequence: u64,
        capture_timestamp_ns: u64,
        payload: &[u8],
        corner_labels: Option<CornerLabelFrame>,
    ) -> Result<SubmitOutcome, TcpImageError> {
        self.mailbox
            .counters
            .borrowed_submit_total
            .fetch_add(1, Ordering::Relaxed);
        let mut frame = match TcpImageFrame::from_slice(
            format,
            width,
            height,
            self.mailbox.producer_epoch,
            sequence,
            capture_timestamp_ns,
            payload,
        ) {
            Ok(frame) => frame,
            Err(error) => {
                self.mailbox.record_rejected_attempt();
                return Err(error);
            }
        };
        frame.corner_labels = corner_labels;
        Ok(self.mailbox.submit(frame))
    }

    pub fn producer_epoch(&self) -> u64 {
        self.mailbox.producer_epoch
    }

    pub fn counters(&self) -> TcpImageSenderCounters {
        self.mailbox.counters()
    }
}

#[derive(Clone, Debug)]
pub struct TcpImageSenderConfig {
    pub bind_addr: SocketAddr,
    pub producer_epoch: u64,
    pub accept_poll: Duration,
    pub write_timeout: Duration,
    pub(crate) corner_label_writer: Option<Arc<CornerLabelJsonlWriter>>,
}

impl TcpImageSenderConfig {
    pub fn new(bind_addr: SocketAddr, producer_epoch: u64) -> Self {
        Self {
            bind_addr,
            producer_epoch,
            accept_poll: DEFAULT_ACCEPT_POLL,
            write_timeout: DEFAULT_WRITE_TIMEOUT,
            corner_label_writer: None,
        }
    }

    #[cfg(test)]
    fn loopback_ephemeral(producer_epoch: u64) -> Self {
        Self::new(SocketAddr::from(([127, 0, 0, 1], 0)), producer_epoch)
    }
}

pub struct TcpImageSender {
    mailbox: Arc<LatestFrameMailbox>,
    local_addr: SocketAddr,
    thread: Option<JoinHandle<()>>,
}

impl TcpImageSender {
    pub fn bind(config: TcpImageSenderConfig) -> io::Result<Self> {
        if config.producer_epoch == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "TCP image producer epoch must be nonzero",
            ));
        }
        let listener = match TcpListener::bind(config.bind_addr) {
            Ok(listener) => listener,
            Err(error) => {
                record_tcp_start_failure();
                return Err(error);
            }
        };
        if let Err(error) = listener.set_nonblocking(true) {
            record_tcp_start_failure();
            return Err(error);
        }
        let local_addr = match listener.local_addr() {
            Ok(address) => address,
            Err(error) => {
                record_tcp_start_failure();
                return Err(error);
            }
        };
        let mailbox = Arc::new(LatestFrameMailbox::with_corner_label_writer(
            config.producer_epoch,
            config.corner_label_writer.clone(),
        ));
        let sender_mailbox = mailbox.clone();
        let thread = match thread::Builder::new()
            .name("talos-tcp-image-sender".to_string())
            .spawn(move || sender_loop(listener, sender_mailbox, config))
        {
            Ok(thread) => thread,
            Err(error) => {
                record_tcp_start_failure();
                return Err(error);
            }
        };
        set_active_mailbox(&mailbox);
        RUNTIME_STATUS.store(TcpImageRuntimeStatus::TcpListening as u8, Ordering::Relaxed);

        Ok(Self {
            mailbox,
            local_addr,
            thread: Some(thread),
        })
    }

    pub fn publisher(&self) -> TcpImagePublisher {
        TcpImagePublisher {
            mailbox: self.mailbox.clone(),
        }
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub fn counters(&self) -> TcpImageSenderCounters {
        self.mailbox.counters()
    }
}

impl Drop for TcpImageSender {
    fn drop(&mut self) {
        self.mailbox.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn active_mailbox() -> &'static Mutex<Weak<LatestFrameMailbox>> {
    static ACTIVE: OnceLock<Mutex<Weak<LatestFrameMailbox>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(Weak::new()))
}

fn set_active_mailbox(mailbox: &Arc<LatestFrameMailbox>) {
    *active_mailbox()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Arc::downgrade(mailbox);
}

pub fn tcp_image_sender_counters() -> TcpImageSenderCounters {
    let mut counters = active_mailbox()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .upgrade()
        .map(|mailbox| mailbox.counters())
        .unwrap_or_default();
    counters.bind_fail_total = BIND_FAIL_TOTAL.load(Ordering::Relaxed);
    counters.runtime_status = TcpImageRuntimeStatus::load();
    counters
}

fn configure_stream(stream: &TcpStream, config: &TcpImageSenderConfig) -> io::Result<()> {
    // The listener and data stream are both explicitly nonblocking. The sender
    // retains one in-progress frame and treats config.write_timeout as a
    // no-forward-progress deadline, so an OS blocking timeout is never relied on.
    stream.set_nonblocking(true)?;
    stream.set_nodelay(true)?;
    stream.set_write_timeout(None)?;
    stream.set_read_timeout(None)?;
    let _ = config;
    Ok(())
}

fn peer_disconnected(stream: &TcpStream) -> bool {
    let mut byte = [0; 1];
    match stream.peek(&mut byte) {
        Ok(0) => true,
        Ok(_) => false,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
            ) =>
        {
            false
        }
        Err(_) => true,
    }
}

fn mark_disconnected(mailbox: &LatestFrameMailbox) {
    mailbox.counters.connected.store(false, Ordering::Relaxed);
    mailbox
        .counters
        .disconnect_total
        .fetch_add(1, Ordering::Relaxed);
}

fn record_write_failure(
    mailbox: &LatestFrameMailbox,
    error: &io::Error,
    payload_bytes_written: usize,
) {
    mailbox
        .counters
        .write_fail_total
        .fetch_add(1, Ordering::Relaxed);
    let kind = TcpImageWriteErrorKind::from_error(error);
    match kind {
        TcpImageWriteErrorKind::TimedOut => {
            mailbox
                .counters
                .write_timeout_total
                .fetch_add(1, Ordering::Relaxed);
        }
        TcpImageWriteErrorKind::ConnectionReset => {
            mailbox
                .counters
                .write_connection_reset_total
                .fetch_add(1, Ordering::Relaxed);
        }
        TcpImageWriteErrorKind::BrokenPipe => {
            mailbox
                .counters
                .write_broken_pipe_total
                .fetch_add(1, Ordering::Relaxed);
        }
        TcpImageWriteErrorKind::None | TcpImageWriteErrorKind::Other => {
            mailbox
                .counters
                .write_other_total
                .fetch_add(1, Ordering::Relaxed);
        }
    }
    let payload_bytes_written = payload_bytes_written as u64;
    mailbox
        .counters
        .write_payload_bytes_before_error_total
        .fetch_add(payload_bytes_written, Ordering::Relaxed);
    mailbox
        .counters
        .last_write_error_kind
        .store(kind as u8, Ordering::Relaxed);
    mailbox
        .counters
        .last_write_error_raw_os_code
        .store(error.raw_os_error().unwrap_or(0), Ordering::Relaxed);
    mailbox
        .counters
        .last_write_payload_bytes_before_error
        .store(payload_bytes_written, Ordering::Relaxed);
}

fn record_duration(total: &AtomicU64, maximum: &AtomicU64, elapsed_ns: u64) {
    total.fetch_add(elapsed_ns, Ordering::Relaxed);
    maximum.fetch_max(elapsed_ns, Ordering::Relaxed);
}

fn record_write_timing(mailbox: &LatestFrameMailbox, timing: &WriteFrameTiming) {
    mailbox
        .counters
        .write_frame_duration_count
        .fetch_add(1, Ordering::Relaxed);
    record_duration(
        &mailbox.counters.write_frame_duration_ns_total,
        &mailbox.counters.write_frame_duration_ns_max,
        timing.frame_duration_ns,
    );
    mailbox
        .counters
        .write_header_duration_count
        .fetch_add(1, Ordering::Relaxed);
    record_duration(
        &mailbox.counters.write_header_duration_ns_total,
        &mailbox.counters.write_header_duration_ns_max,
        timing.header.duration_ns,
    );
    if let Some(payload) = timing.payload {
        mailbox
            .counters
            .write_payload_duration_count
            .fetch_add(1, Ordering::Relaxed);
        record_duration(
            &mailbox.counters.write_payload_duration_ns_total,
            &mailbox.counters.write_payload_duration_ns_max,
            payload.duration_ns,
        );
    }
    mailbox
        .counters
        .write_would_block_total
        .fetch_add(timing.header.would_block_total, Ordering::Relaxed);
    if let Some(payload) = timing.payload {
        mailbox
            .counters
            .write_would_block_total
            .fetch_add(payload.would_block_total, Ordering::Relaxed);
    }
    mailbox
        .counters
        .write_retry_total
        .fetch_add(timing.header.retry_total, Ordering::Relaxed);
    if let Some(payload) = timing.payload {
        mailbox
            .counters
            .write_retry_total
            .fetch_add(payload.retry_total, Ordering::Relaxed);
    }
}

fn sender_loop(
    listener: TcpListener,
    mailbox: Arc<LatestFrameMailbox>,
    config: TcpImageSenderConfig,
) {
    let mut stream: Option<TcpStream> = None;
    let mut pending: Option<PendingWrite> = None;
    while !mailbox.is_stopping() {
        if stream.is_none() {
            match listener.accept() {
                Ok((accepted, _)) => match configure_stream(&accepted, &config) {
                    Ok(()) => {
                        mailbox
                            .counters
                            .connect_total
                            .fetch_add(1, Ordering::Relaxed);
                        mailbox.counters.connected.store(true, Ordering::Relaxed);
                        stream = Some(accepted);
                    }
                    Err(_) => {
                        mailbox
                            .counters
                            .accept_fail_total
                            .fetch_add(1, Ordering::Relaxed);
                    }
                },
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    mailbox.wait_for_signal(config.accept_poll);
                }
                Err(_) => {
                    mailbox
                        .counters
                        .accept_fail_total
                        .fetch_add(1, Ordering::Relaxed);
                    mailbox.wait_for_signal(config.accept_poll);
                }
            }
            continue;
        }

        if pending.is_none() {
            let Some(frame) = mailbox.take_latest() else {
                if stream.as_ref().is_some_and(peer_disconnected) {
                    mark_disconnected(&mailbox);
                    stream = None;
                } else {
                    let _ = mailbox.wait_for_signal(config.accept_poll);
                }
                continue;
            };
            pending = Some(PendingWrite::new(frame, Instant::now()));
        }

        let active_stream = stream.as_mut().expect("TCP image stream must exist");
        let now = Instant::now();
        let step = advance_pending_write(
            active_stream,
            pending.as_mut().expect("pending write must exist"),
            now,
        );
        match step {
            PendingWriteStep::Progress => continue,
            PendingWriteStep::WouldBlock => {
                let pending_write = pending.as_ref().expect("pending write must exist");
                if pending_write_stalled(pending_write, now, config.write_timeout) {
                    let failed = pending.take().expect("pending write must exist");
                    record_write_timing(&mailbox, &failed.timing_at(now));
                    record_write_failure(
                        &mailbox,
                        &io::Error::new(
                            io::ErrorKind::TimedOut,
                            "TCP image sender made no write progress before deadline",
                        ),
                        failed.payload_offset,
                    );
                    mark_disconnected(&mailbox);
                    stream = None;
                } else {
                    thread::sleep(NONBLOCKING_WRITE_RETRY_POLL);
                }
            }
            PendingWriteStep::Complete => {
                let completed = pending.take().expect("pending write must exist");
                record_write_timing(&mailbox, &completed.timing_at(now));
                mailbox.counters.sent_total.fetch_add(1, Ordering::Relaxed);
                mailbox.counters.wire_bytes_sent_total.fetch_add(
                    (HEADER_BYTES + completed.frame.payload.len()) as u64,
                    Ordering::Relaxed,
                );
                mailbox
                    .counters
                    .latest_sent_seq
                    .store(completed.frame.header.sequence, Ordering::Relaxed);
                if let (Some(writer), Some(labels)) = (
                    mailbox.corner_label_writer.as_ref(),
                    completed.frame.corner_labels.as_ref(),
                ) && let Err(write_error) =
                    writer.write_sent_frame(&completed.frame.header, labels)
                {
                    error!(
                        "corner-label row rejected for {} after sent TCP frame {} (I/O failures disable the writer): {}",
                        writer.path().display(),
                        completed.frame.header.sequence,
                        write_error
                    );
                }
            }
            PendingWriteStep::Error(error) => {
                let failed = pending.take().expect("pending write must exist");
                record_write_timing(&mailbox, &failed.timing_at(now));
                record_write_failure(&mailbox, &error, failed.payload_offset);
                mark_disconnected(&mailbox);
                stream = None;
            }
        }
    }

    mailbox.counters.connected.store(false, Ordering::Relaxed);
}

#[derive(Debug)]
struct WriteFrameFailure {
    error: io::Error,
    payload_bytes_written: usize,
    timing: WriteFrameTiming,
}

#[derive(Clone, Copy, Debug, Default)]
struct WriteStageTiming {
    duration_ns: u64,
    retry_total: u64,
    would_block_total: u64,
}

#[derive(Clone, Copy, Debug, Default)]
struct WriteFrameTiming {
    frame_duration_ns: u64,
    header: WriteStageTiming,
    payload: Option<WriteStageTiming>,
}

struct PendingWrite {
    frame: TcpImageFrame,
    header_offset: usize,
    payload_offset: usize,
    frame_started: Instant,
    header_started: Instant,
    payload_started: Option<Instant>,
    last_progress: Instant,
    header_retry_total: u64,
    header_would_block_total: u64,
    payload_retry_total: u64,
    payload_would_block_total: u64,
}

impl PendingWrite {
    fn new(frame: TcpImageFrame, now: Instant) -> Self {
        Self {
            frame,
            header_offset: 0,
            payload_offset: 0,
            frame_started: now,
            header_started: now,
            payload_started: None,
            last_progress: now,
            header_retry_total: 0,
            header_would_block_total: 0,
            payload_retry_total: 0,
            payload_would_block_total: 0,
        }
    }

    fn timing_at(&self, now: Instant) -> WriteFrameTiming {
        WriteFrameTiming {
            frame_duration_ns: now.duration_since(self.frame_started).as_nanos() as u64,
            header: WriteStageTiming {
                duration_ns: now.duration_since(self.header_started).as_nanos() as u64,
                retry_total: self.header_retry_total,
                would_block_total: self.header_would_block_total,
            },
            payload: self.payload_started.map(|started| WriteStageTiming {
                duration_ns: now.duration_since(started).as_nanos() as u64,
                retry_total: self.payload_retry_total,
                would_block_total: self.payload_would_block_total,
            }),
        }
    }
}

enum PendingWriteStep {
    Progress,
    WouldBlock,
    Complete,
    Error(io::Error),
}

fn advance_pending_write(
    writer: &mut impl Write,
    pending: &mut PendingWrite,
    now: Instant,
) -> PendingWriteStep {
    let writing_header = pending.header_offset < HEADER_BYTES;
    let result = if writing_header {
        let header = pending.frame.header.encode();
        writer.write(&header[pending.header_offset..])
    } else {
        writer.write(&pending.frame.payload[pending.payload_offset..])
    };
    match result {
        Ok(0) => PendingWriteStep::Error(io::Error::new(
            io::ErrorKind::WriteZero,
            "failed to make TCP image write progress",
        )),
        Ok(written) => {
            pending.last_progress = now;
            if writing_header {
                pending.header_offset += written;
                if pending.header_offset == HEADER_BYTES {
                    pending.payload_started = Some(now);
                }
                PendingWriteStep::Progress
            } else {
                pending.payload_offset += written;
                if pending.payload_offset == pending.frame.payload.len() {
                    PendingWriteStep::Complete
                } else {
                    PendingWriteStep::Progress
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::Interrupted => {
            if writing_header {
                pending.header_retry_total = pending.header_retry_total.saturating_add(1);
            } else {
                pending.payload_retry_total = pending.payload_retry_total.saturating_add(1);
            }
            PendingWriteStep::Progress
        }
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
            if writing_header {
                pending.header_would_block_total =
                    pending.header_would_block_total.saturating_add(1);
            } else {
                pending.payload_would_block_total =
                    pending.payload_would_block_total.saturating_add(1);
            }
            PendingWriteStep::WouldBlock
        }
        Err(error) => PendingWriteStep::Error(error),
    }
}

fn pending_write_stalled(pending: &PendingWrite, now: Instant, deadline: Duration) -> bool {
    now.duration_since(pending.last_progress) >= deadline
}

fn write_all_counting(
    writer: &mut impl Write,
    bytes: &[u8],
) -> Result<(usize, WriteStageTiming), (io::Error, usize, WriteStageTiming)> {
    let started = Instant::now();
    let mut bytes_written = 0;
    let mut retry_total = 0;
    while bytes_written < bytes.len() {
        match writer.write(&bytes[bytes_written..]) {
            Ok(0) => {
                return Err((
                    io::Error::new(
                        io::ErrorKind::WriteZero,
                        "failed to write the complete TCP image frame",
                    ),
                    bytes_written,
                    WriteStageTiming {
                        duration_ns: started.elapsed().as_nanos() as u64,
                        retry_total,
                        would_block_total: 0,
                    },
                ));
            }
            Ok(written) => bytes_written += written,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                retry_total = retry_total.saturating_add(1);
            }
            Err(error) => {
                let would_block_total = u64::from(error.kind() == io::ErrorKind::WouldBlock);
                return Err((
                    error,
                    bytes_written,
                    WriteStageTiming {
                        duration_ns: started.elapsed().as_nanos() as u64,
                        retry_total,
                        would_block_total,
                    },
                ));
            }
        }
    }
    Ok((
        bytes_written,
        WriteStageTiming {
            duration_ns: started.elapsed().as_nanos() as u64,
            retry_total,
            would_block_total: 0,
        },
    ))
}

fn write_frame_with_progress(
    writer: &mut impl Write,
    frame: &TcpImageFrame,
) -> Result<WriteFrameTiming, WriteFrameFailure> {
    let frame_started = Instant::now();
    let (header_bytes_written, header_timing) =
        match write_all_counting(writer, &frame.header.encode()) {
            Ok(result) => result,
            Err((error, _, header_timing)) => {
                return Err(WriteFrameFailure {
                    error,
                    payload_bytes_written: 0,
                    timing: WriteFrameTiming {
                        frame_duration_ns: frame_started.elapsed().as_nanos() as u64,
                        header: header_timing,
                        payload: None,
                    },
                });
            }
        };
    debug_assert_eq!(header_bytes_written, HEADER_BYTES);
    match write_all_counting(writer, &frame.payload) {
        Ok((_, payload_timing)) => Ok(WriteFrameTiming {
            frame_duration_ns: frame_started.elapsed().as_nanos() as u64,
            header: header_timing,
            payload: Some(payload_timing),
        }),
        Err((error, payload_bytes_written, payload_timing)) => Err(WriteFrameFailure {
            error,
            payload_bytes_written,
            timing: WriteFrameTiming {
                frame_duration_ns: frame_started.elapsed().as_nanos() as u64,
                header: header_timing,
                payload: Some(payload_timing),
            },
        }),
    }
}

fn write_frame(writer: &mut impl Write, frame: &TcpImageFrame) -> io::Result<()> {
    write_frame_with_progress(writer, frame)
        .map(|_| ())
        .map_err(|failure| failure.error)
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_WRITE_TIMEOUT, HEADER_BYTES, LatestFrameMailbox, MailboxWaitOutcome, PendingWrite,
        PendingWriteStep, PixelFormat, SubmitOutcome, TcpImageFrame, TcpImageHeader,
        TcpImagePublisher, TcpImageSender, TcpImageSenderConfig, TcpImageWriteErrorKind,
        advance_pending_write, configure_stream, pending_write_stalled, record_write_failure,
        record_write_timing, tcp_image_sender_counters, write_frame, write_frame_with_progress,
    };
    use std::collections::VecDeque;
    use std::io::{self, Read, Write};
    use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    const EPOCH: u64 = 0x0102_0304_0506_0708;

    fn frame(sequence: u64, fill: u8) -> TcpImageFrame {
        TcpImageFrame::from_slice(
            PixelFormat::Rgba32,
            2,
            1,
            EPOCH,
            sequence,
            0x2122_2324_2526_2728,
            &[fill; 8],
        )
        .unwrap()
    }

    #[test]
    fn header_codec_matches_cpp_v1_big_endian_golden_bytes() {
        let header = TcpImageHeader::new(
            PixelFormat::Rgba32,
            2,
            1,
            EPOCH,
            0x1112_1314_1516_1718,
            0x2122_2324_2526_2728,
        )
        .unwrap();

        let expected = [
            0x54, 0x49, 0x4d, 0x47, // magic TIMG
            0x00, 0x01, // version
            0x00, 0x40, // header bytes
            0x00, 0x02, // RGBA32
            0x00, 0x00, // flags
            0x00, 0x00, 0x00, 0x02, // width
            0x00, 0x00, 0x00, 0x01, // height
            0x00, 0x00, 0x00, 0x08, // payload bytes
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, // epoch
            0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, // sequence
            0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, // capture ns
            0, 0, 0, 0, 0, 0, 0, 0, // reserved 0
            0, 0, 0, 0, 0, 0, 0, 0, // reserved 1
        ];
        assert_eq!(header.encode(), expected);

        let rgb = TcpImageHeader::new(PixelFormat::Rgb24, 1440, 1080, EPOCH, 1, 2)
            .unwrap()
            .encode();
        assert_eq!(&rgb[8..10], &1u16.to_be_bytes());
        assert_eq!(&rgb[20..24], &4_665_600u32.to_be_bytes());

        let rgba = TcpImageHeader::new(PixelFormat::Rgba32, 1440, 1080, EPOCH, 1, 2)
            .unwrap()
            .encode();
        assert_eq!(&rgba[20..24], &6_220_800u32.to_be_bytes());
    }

    #[test]
    fn exact_validated_owned_frame_preserves_vec_pointer_and_capacity() {
        let mut payload = Vec::with_capacity(32);
        payload.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let pointer = payload.as_ptr() as usize;
        let capacity = payload.capacity();

        let frame =
            TcpImageFrame::from_owned(PixelFormat::Rgba32, 2, 1, EPOCH, 7, 8, payload).unwrap();

        assert_eq!(frame.payload.as_ptr() as usize, pointer);
        assert_eq!(frame.payload.capacity(), capacity);
        assert_eq!(frame.payload, [1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn invalid_owned_frame_returns_the_original_allocation_for_borrowed_fallback() {
        let mut payload = Vec::with_capacity(32);
        payload.extend_from_slice(&[1, 2, 3, 4]);
        let pointer = payload.as_ptr() as usize;
        let capacity = payload.capacity();

        let error =
            TcpImageFrame::from_owned(PixelFormat::Rgba32, 2, 1, EPOCH, 7, 8, payload).unwrap_err();
        let (error, payload) = error.into_parts();

        assert!(matches!(
            error,
            super::TcpImageError::PayloadSizeMismatch {
                expected: 8,
                actual: 4
            }
        ));
        assert_eq!(payload.as_ptr() as usize, pointer);
        assert_eq!(payload.capacity(), capacity);
        assert_eq!(payload, [1, 2, 3, 4]);
    }

    #[test]
    fn owned_publisher_moves_the_same_vec_into_mailbox_and_counts_owned_vs_borrowed() {
        let mailbox = Arc::new(LatestFrameMailbox::new(EPOCH));
        let publisher = TcpImagePublisher {
            mailbox: mailbox.clone(),
        };
        let payload = vec![0x44; 8];
        let pointer = payload.as_ptr() as usize;

        assert_eq!(
            publisher.submit_rgba32_owned(2, 1, 1, 10, payload).unwrap(),
            SubmitOutcome::Accepted
        );
        let frame = mailbox.take_latest().unwrap();
        assert_eq!(frame.payload.as_ptr() as usize, pointer);
        assert_eq!(publisher.counters().owned_submit_total, 1);
        assert_eq!(publisher.counters().borrowed_submit_total, 0);

        assert_eq!(
            publisher.submit_rgba32(2, 1, 2, 11, &[0x55; 8]).unwrap(),
            SubmitOutcome::Accepted
        );
        assert_eq!(publisher.counters().owned_submit_total, 1);
        assert_eq!(publisher.counters().borrowed_submit_total, 1);
    }

    #[test]
    fn mailbox_is_capacity_one_latest_only_and_rejects_nonmonotonic_identity() {
        let mailbox = LatestFrameMailbox::new(EPOCH);

        assert_eq!(mailbox.submit(frame(1, 1)), SubmitOutcome::Accepted);
        assert_eq!(mailbox.submit(frame(2, 2)), SubmitOutcome::Replaced);
        assert_eq!(mailbox.submit(frame(2, 3)), SubmitOutcome::Rejected);
        assert_eq!(mailbox.submit(frame(1, 4)), SubmitOutcome::Rejected);
        let wrong_epoch =
            TcpImageFrame::from_slice(PixelFormat::Rgba32, 2, 1, EPOCH + 1, 4, 5, &[0; 8]).unwrap();
        assert_eq!(mailbox.submit(wrong_epoch), SubmitOutcome::Rejected);

        let latest = mailbox.take_latest().unwrap();
        assert_eq!(latest.header.sequence, 2);
        assert_eq!(latest.payload.as_slice(), &[2; 8]);
        assert!(mailbox.take_latest().is_none());

        assert_eq!(mailbox.submit(frame(3, 3)), SubmitOutcome::Accepted);
        assert_eq!(mailbox.take_latest().unwrap().header.sequence, 3);

        let counters = mailbox.counters();
        assert_eq!(counters.submit_total, 6);
        assert_eq!(counters.replaced_total, 1);
        assert_eq!(counters.rejected_total, 3);
        assert_eq!(counters.mailbox_current, 0);
        assert_eq!(counters.mailbox_max, 1);
        assert_eq!(counters.latest_submitted_seq, 3);
    }

    #[test]
    fn mailbox_wait_observes_preexisting_frame_without_consuming_a_timeout() {
        let mailbox = LatestFrameMailbox::new(EPOCH);
        assert_eq!(mailbox.submit(frame(1, 0x11)), SubmitOutcome::Accepted);

        // This models the sender's former take_latest()->wait race: submission
        // has already happened before wait acquires the mailbox mutex. The
        // predicate must observe it directly rather than sleep for accept_poll.
        assert_eq!(
            mailbox.wait_for_signal(Duration::from_secs(60)),
            MailboxWaitOutcome::FrameAvailable
        );
        assert_eq!(mailbox.take_latest().unwrap().header.sequence, 1);
    }

    #[derive(Default)]
    struct FragmentedWriter {
        bytes: Vec<u8>,
        maximum_chunk: usize,
    }

    impl Write for FragmentedWriter {
        fn write(&mut self, input: &[u8]) -> io::Result<usize> {
            let count = input.len().min(self.maximum_chunk.max(1));
            self.bytes.extend_from_slice(&input[..count]);
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct DisconnectWriter {
        remaining: usize,
    }

    struct InterruptedThenFragmentedWriter {
        interrupted: bool,
        bytes: Vec<u8>,
    }

    impl Write for InterruptedThenFragmentedWriter {
        fn write(&mut self, input: &[u8]) -> io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "simulated retry",
                ));
            }
            let count = input.len().min(3);
            self.bytes.extend_from_slice(&input[..count]);
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct WouldBlockWriter {
        remaining: usize,
    }

    #[derive(Clone, Copy)]
    enum ScriptedWrite {
        Write(usize),
        WouldBlock,
        Error(io::ErrorKind),
    }

    struct ScriptedWriter {
        script: VecDeque<ScriptedWrite>,
        bytes: Vec<u8>,
    }

    impl Write for ScriptedWriter {
        fn write(&mut self, input: &[u8]) -> io::Result<usize> {
            match self.script.pop_front().expect("scripted writer exhausted") {
                ScriptedWrite::Write(limit) => {
                    let count = input.len().min(limit);
                    self.bytes.extend_from_slice(&input[..count]);
                    Ok(count)
                }
                ScriptedWrite::WouldBlock => Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "scripted would block",
                )),
                ScriptedWrite::Error(kind) => Err(io::Error::new(kind, "scripted error")),
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Write for WouldBlockWriter {
        fn write(&mut self, input: &[u8]) -> io::Result<usize> {
            if self.remaining == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "simulated would block",
                ));
            }
            let count = input.len().min(self.remaining);
            self.remaining -= count;
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Write for DisconnectWriter {
        fn write(&mut self, input: &[u8]) -> io::Result<usize> {
            if self.remaining == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "simulated disconnect",
                ));
            }
            let count = input.len().min(self.remaining);
            self.remaining -= count;
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn full_write_handles_fragmentation_and_disconnect_does_not_poison_next_writer() {
        let frame = frame(7, 0x5a);
        let expected_header = frame.header.encode();

        let mut fragmented = FragmentedWriter {
            bytes: Vec::new(),
            maximum_chunk: 3,
        };
        write_frame(&mut fragmented, &frame).unwrap();
        assert_eq!(&fragmented.bytes[..HEADER_BYTES], &expected_header);
        assert_eq!(&fragmented.bytes[HEADER_BYTES..], frame.payload.as_slice());

        let mut disconnected = DisconnectWriter { remaining: 17 };
        assert_eq!(
            write_frame(&mut disconnected, &frame).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );

        let mut replacement = FragmentedWriter {
            bytes: Vec::new(),
            maximum_chunk: 2,
        };
        write_frame(&mut replacement, &frame).unwrap();
        assert_eq!(replacement.bytes.len(), HEADER_BYTES + frame.payload.len());
    }

    #[test]
    fn write_failure_telemetry_classifies_error_and_preserves_payload_progress() {
        let mailbox = LatestFrameMailbox::new(EPOCH);
        record_write_failure(
            &mailbox,
            &io::Error::new(io::ErrorKind::TimedOut, "simulated timeout"),
            3,
        );
        record_write_failure(
            &mailbox,
            &io::Error::new(io::ErrorKind::ConnectionReset, "simulated reset"),
            2,
        );
        record_write_failure(
            &mailbox,
            &io::Error::new(io::ErrorKind::BrokenPipe, "simulated pipe"),
            1,
        );
        record_write_failure(
            &mailbox,
            &io::Error::new(io::ErrorKind::WriteZero, "simulated other"),
            4,
        );

        let counters = mailbox.counters();
        assert_eq!(counters.write_fail_total, 4);
        assert_eq!(counters.write_timeout_total, 1);
        assert_eq!(counters.write_connection_reset_total, 1);
        assert_eq!(counters.write_broken_pipe_total, 1);
        assert_eq!(counters.write_other_total, 1);
        assert_eq!(counters.write_payload_bytes_before_error_total, 10);
        assert_eq!(
            counters.last_write_error_kind,
            TcpImageWriteErrorKind::Other
        );
        assert_eq!(counters.last_write_error_raw_os_code, 0);
        assert_eq!(counters.last_write_payload_bytes_before_error, 4);
    }

    #[test]
    fn payload_write_failure_reports_partial_progress_without_changing_write_semantics() {
        let frame = frame(9, 0x33);
        let mut writer = DisconnectWriter {
            remaining: HEADER_BYTES + 3,
        };
        let failure = write_frame_with_progress(&mut writer, &frame).unwrap_err();
        assert_eq!(failure.error.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(failure.payload_bytes_written, 3);
    }

    #[test]
    fn write_timing_counts_deterministic_retry_and_would_block_stages() {
        let frame = frame(10, 0x66);
        let mailbox = LatestFrameMailbox::new(EPOCH);
        let mut retrying = InterruptedThenFragmentedWriter {
            interrupted: false,
            bytes: Vec::new(),
        };
        let success = write_frame_with_progress(&mut retrying, &frame).unwrap();
        assert_eq!(success.header.retry_total, 1);
        assert_eq!(success.payload.unwrap().retry_total, 0);
        record_write_timing(&mailbox, &success);

        let mut would_block = WouldBlockWriter {
            remaining: HEADER_BYTES + 2,
        };
        let failure = write_frame_with_progress(&mut would_block, &frame).unwrap_err();
        assert_eq!(failure.timing.payload.unwrap().would_block_total, 1);
        record_write_timing(&mailbox, &failure.timing);

        let counters = mailbox.counters();
        assert_eq!(counters.write_frame_duration_count, 2);
        assert_eq!(counters.write_header_duration_count, 2);
        assert_eq!(counters.write_payload_duration_count, 2);
        assert_eq!(counters.write_retry_total, 1);
        assert_eq!(counters.write_would_block_total, 1);
        assert!(counters.write_frame_duration_ns_max <= counters.write_frame_duration_ns_total);
        assert!(counters.write_header_duration_ns_max <= counters.write_header_duration_ns_total);
        assert!(counters.write_payload_duration_ns_max <= counters.write_payload_duration_ns_total);
    }

    #[test]
    fn partial_nonblocking_writer_resumes_once_without_frame_interleave() {
        let first = frame(20, 0x20);
        let second = frame(21, 0x21);
        let first_wire = [first.header.encode().as_slice(), first.payload.as_slice()].concat();
        let second_wire = [second.header.encode().as_slice(), second.payload.as_slice()].concat();
        let started = Instant::now();
        let mut pending = PendingWrite::new(first, started);
        let mut writer = ScriptedWriter {
            script: VecDeque::from([
                ScriptedWrite::Write(7),
                ScriptedWrite::WouldBlock,
                ScriptedWrite::Write(HEADER_BYTES - 7),
                ScriptedWrite::Write(3),
                ScriptedWrite::WouldBlock,
                ScriptedWrite::Write(5),
                ScriptedWrite::Write(HEADER_BYTES),
                ScriptedWrite::Write(8),
            ]),
            bytes: Vec::new(),
        };

        assert!(matches!(
            advance_pending_write(&mut writer, &mut pending, started),
            PendingWriteStep::Progress
        ));
        assert!(matches!(
            advance_pending_write(&mut writer, &mut pending, started),
            PendingWriteStep::WouldBlock
        ));
        assert_eq!(pending.header_offset, 7);
        assert_eq!(pending.payload_offset, 0);
        assert!(matches!(
            advance_pending_write(&mut writer, &mut pending, started),
            PendingWriteStep::Progress
        ));
        assert!(matches!(
            advance_pending_write(&mut writer, &mut pending, started),
            PendingWriteStep::Progress
        ));
        assert!(matches!(
            advance_pending_write(&mut writer, &mut pending, started),
            PendingWriteStep::WouldBlock
        ));
        assert_eq!(pending.payload_offset, 3);
        assert!(matches!(
            advance_pending_write(&mut writer, &mut pending, started),
            PendingWriteStep::Complete
        ));
        assert_eq!(writer.bytes, first_wire);

        // A future frame can only begin after the first complete wire frame.
        let mut next = PendingWrite::new(second, started);
        assert!(matches!(
            advance_pending_write(&mut writer, &mut next, started),
            PendingWriteStep::Progress
        ));
        assert!(matches!(
            advance_pending_write(&mut writer, &mut next, started),
            PendingWriteStep::Complete
        ));
        assert_eq!(writer.bytes, [first_wire, second_wire].concat());
        assert_eq!(pending.header_would_block_total, 1);
        assert_eq!(pending.payload_would_block_total, 1);
    }

    #[test]
    fn partial_writer_stall_deadline_and_peer_error_are_distinct() {
        let started = Instant::now();
        let mut pending = PendingWrite::new(frame(30, 0x30), started);
        let mut writer = ScriptedWriter {
            script: VecDeque::from([
                ScriptedWrite::WouldBlock,
                ScriptedWrite::Error(io::ErrorKind::BrokenPipe),
            ]),
            bytes: Vec::new(),
        };
        assert!(matches!(
            advance_pending_write(&mut writer, &mut pending, started),
            PendingWriteStep::WouldBlock
        ));
        assert!(!pending_write_stalled(
            &pending,
            started + Duration::from_millis(249),
            DEFAULT_WRITE_TIMEOUT
        ));
        assert!(pending_write_stalled(
            &pending,
            started + DEFAULT_WRITE_TIMEOUT,
            DEFAULT_WRITE_TIMEOUT
        ));
        assert!(matches!(
            advance_pending_write(&mut writer, &mut pending, started + Duration::from_millis(1)),
            PendingWriteStep::Error(error) if error.kind() == io::ErrorKind::BrokenPipe
        ));
        assert_eq!(pending.header_offset, 0);
        assert_eq!(pending.payload_offset, 0);
    }

    #[test]
    fn sender_config_default_write_timeout_remains_exact() {
        let config = TcpImageSenderConfig::loopback_ephemeral(EPOCH);
        assert_eq!(config.write_timeout, DEFAULT_WRITE_TIMEOUT);
        assert_eq!(config.write_timeout, Duration::from_millis(250));
    }

    #[test]
    fn accepted_data_stream_is_explicitly_nonblocking() {
        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).unwrap();
        listener.set_nonblocking(true).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let accepted = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "timed out accepting loopback client"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("unexpected accept error: {error}"),
            }
        };
        configure_stream(&accepted, &TcpImageSenderConfig::loopback_ephemeral(EPOCH)).unwrap();

        let mut byte = [0; 1];
        let error = accepted.peek(&mut byte).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        drop(client);
    }

    fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if predicate() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        predicate()
    }

    #[test]
    fn sender_reaccepts_after_peer_disconnect_and_delivers_only_complete_frame() {
        let sender = TcpImageSender::bind(TcpImageSenderConfig::loopback_ephemeral(EPOCH)).unwrap();
        let publisher = sender.publisher();
        let address = sender.local_addr();

        let first = TcpStream::connect(address).unwrap();
        assert!(wait_until(Duration::from_secs(2), || {
            sender.counters().connect_total >= 1
        }));
        first.shutdown(Shutdown::Both).unwrap();
        drop(first);

        let large = vec![0x11; 1440 * 1080 * 4];
        publisher
            .submit_rgba32(1440, 1080, 10, 100, &large)
            .unwrap();
        assert!(wait_until(Duration::from_secs(2), || {
            sender.counters().disconnect_total >= 1
        }));

        // Publish the replacement while disconnected so capacity-one semantics
        // deterministically replace any frame that raced with disconnect
        // detection before the next peer can drain the mailbox.
        publisher.submit_rgba32(2, 1, 11, 101, &[0x22; 8]).unwrap();
        let mut second = TcpStream::connect(address).unwrap();
        second
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        assert!(wait_until(Duration::from_secs(2), || {
            sender.counters().connect_total >= 2
        }));
        let mut wire = vec![0; HEADER_BYTES + 8];
        second.read_exact(&mut wire).unwrap();
        assert_eq!(&wire[..4], b"TIMG");
        assert_eq!(u64::from_be_bytes(wire[32..40].try_into().unwrap()), 11);
        assert_eq!(&wire[HEADER_BYTES..], &[0x22; 8]);
        assert!(wait_until(Duration::from_secs(2), || {
            sender.counters().sent_total >= 1
        }));
    }

    #[test]
    fn explicit_tcp_bind_failure_is_counted_and_never_constructs_a_sender() {
        let blocker = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).unwrap();
        let occupied = blocker.local_addr().unwrap();
        let before = tcp_image_sender_counters().bind_fail_total;
        let result = TcpImageSender::bind(TcpImageSenderConfig::new(occupied, EPOCH));
        assert!(result.is_err());
        assert!(tcp_image_sender_counters().bind_fail_total >= before + 1);
    }
}
