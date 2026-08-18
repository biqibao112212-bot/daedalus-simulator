use std::ffi::OsString;
use std::sync::OnceLock;

use crate::config::SimulationConfig;

const RELEASE_MODE_ENV: &str = "DAEDALUS_RELEASE_MODE";
const CORNER_LABELS_ENV: &str = "DAEDALUS_CORNER_LABELS_JSONL";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseMode {
    Performance,
    Visible,
}

static RELEASE_MODE: OnceLock<ReleaseMode> = OnceLock::new();

pub const fn is_locked() -> bool {
    cfg!(feature = "distribution-release")
}

// The contest binary is deliberately a narrower distribution profile.  Keep
// this separate from `is_locked()`: ordinary internal releases retain their
// historic scene set and only the dedicated contest build has this restriction.
pub const fn is_contest_release() -> bool {
    cfg!(feature = "contest-release")
}

/// Learning builds are deliberately separate from contest builds. They retain
/// the locked distribution launcher and fixed network endpoints, but publish
/// current and retained exposure-matched simulator truth for training.
pub const fn is_learning_release() -> bool {
    cfg!(feature = "learning-release")
}

/// Online target/rune truth is a learning-only capability. Contest and every
/// other distribution profile fail closed while retaining the empty exposure
/// history needed for image/gimbal synchronization.
pub fn allows_online_ground_truth() -> bool {
    !is_locked()
        || (is_learning_release()
            && std::env::var("DAEDALUS_LEARNING_TRUTH")
                .map(|value| value != "0")
                .unwrap_or(true))
}

/// Whether the release profile permits a participant to control the local
/// vehicle with the keyboard and mouse.
///
/// Ordinary distribution builds remain SDK-command driven. The internal
/// contest client is intentionally different: it is a visible teaching and
/// evaluation environment, so participants must be able to drive the local
/// vehicle without first writing an SDK program.
pub const fn allows_local_manual_controls() -> bool {
    !is_locked() || is_contest_release() || is_learning_release()
}

/// Whether the distribution profile claims the controlled gimbal at startup.
///
/// Contest builds start with manual ownership so arrow-key and mouse input is
/// immediately useful. SDK commands can still be used through the existing
/// command path.
pub const fn auto_aim_enabled_on_start() -> bool {
    !is_contest_release() && !is_learning_release()
}

pub fn prepare_environment() {
    if !is_locked() {
        return;
    }

    let mode = std::env::var(RELEASE_MODE_ENV)
        .ok()
        .filter(|value| value.eq_ignore_ascii_case("visible"))
        .map(|_| ReleaseMode::Visible)
        .unwrap_or(if is_learning_release() {
            ReleaseMode::Visible
        } else {
            ReleaseMode::Performance
        });
    let _ = RELEASE_MODE.set(mode);

    // This is the only caller-provided DAEDALUS_* value retained by a
    // distribution build. It controls a write-only, offline sidecar whose
    // rows are committed only after the matching TCP image is fully sent; it
    // does not relax the online ground-truth lock.
    let corner_labels_path = std::env::var_os(CORNER_LABELS_ENV).filter(|value| !value.is_empty());
    let learning_truth = is_learning_release()
        && std::env::var("DAEDALUS_LEARNING_TRUTH")
            .map(|value| value != "0")
            .unwrap_or(true);

    // DAEDALUS_* variables are development controls. A distribution build
    // removes all caller-provided values before setting the small, fixed set
    // needed by the public runtime contract.
    let keys = std::env::vars_os()
        .filter_map(|(key, _)| os_starts_with(&key, "DAEDALUS_").then_some(key))
        .collect::<Vec<_>>();
    for key in keys {
        // SAFETY: this runs before the simulator creates any worker threads.
        unsafe { std::env::remove_var(key) };
    }

    set_env("DAEDALUS_TALOS_RGB_ONLY", "1");
    set_env("DAEDALUS_TALOS_CAPTURE_MAX_HZ", "200");
    set_env("DAEDALUS_TALOS_IMAGE_TRANSPORT", "tcp");
    set_env("DAEDALUS_TALOS_TCP_BIND", "127.0.0.1:5602");
    set_env(
        "DAEDALUS_AUTO_AIM_ON_START",
        if auto_aim_enabled_on_start() {
            "1"
        } else {
            "0"
        },
    );
    if let Some(path) = corner_labels_path {
        set_env_os(CORNER_LABELS_ENV, path);
    }
    if is_learning_release() {
        set_env(
            "DAEDALUS_LEARNING_TRUTH",
            if learning_truth { "1" } else { "0" },
        );
    }
    set_env("WGPU_POWER_PREF", "high");
    // Distribution builds always let wgpu select the adapter. The launcher may
    // select a supported backend, but a caller cannot pin an unvalidated GPU.
    unsafe { std::env::remove_var("WGPU_ADAPTER_NAME") };
    if mode == ReleaseMode::Performance {
        set_env("DAEDALUS_PERF_DISABLE_UI", "1");
    }
}

pub fn mode() -> ReleaseMode {
    RELEASE_MODE
        .get()
        .copied()
        .unwrap_or(ReleaseMode::Performance)
}

pub fn apply_locked_config(config: &mut SimulationConfig) {
    if !is_locked() {
        return;
    }

    config.debug.egui = false;
    config.debug.inspector = false;
    config.debug.diagnostics = false;
    config.network_bridge.enabled = true;
    config.network_bridge.bind = "127.0.0.1:5601".to_string();
    config.auto_aim.enabled_on_start = auto_aim_enabled_on_start();
    config.auto_aim.managed_bridge_enabled = false;
    config.auto_aim.command_transport = "udp".to_string();
    match mode() {
        ReleaseMode::Performance => {
            config.preview.enabled = false;
            config.preview.max_hz = 0.0;
        }
        ReleaseMode::Visible => {
            config.preview.enabled = true;
            config.preview.max_hz = 60.0;
        }
    }
}

fn os_starts_with(value: &OsString, prefix: &str) -> bool {
    value
        .to_str()
        .map(|value| value.starts_with(prefix))
        .unwrap_or(false)
}

fn set_env(key: &str, value: &str) {
    // SAFETY: this module is invoked before Bevy or any simulator worker
    // thread starts, so the process environment is still single-threaded.
    unsafe { std::env::set_var(key, value) };
}

fn set_env_os(key: &str, value: OsString) {
    // SAFETY: this module is invoked before Bevy or any simulator worker
    // thread starts, so the process environment is still single-threaded.
    unsafe { std::env::set_var(key, value) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_build_is_not_distribution_locked() {
        if !cfg!(feature = "distribution-release") {
            assert!(!is_locked());
        }
    }

    #[cfg(feature = "distribution-release")]
    #[test]
    fn distribution_build_is_locked_even_with_offline_export_compiled() {
        assert!(is_locked());
        assert_eq!(CORNER_LABELS_ENV, "DAEDALUS_CORNER_LABELS_JSONL");
    }

    #[cfg(feature = "contest-release")]
    #[test]
    fn contest_build_starts_with_local_vehicle_controls() {
        assert!(is_contest_release());
        assert!(allows_local_manual_controls());
        assert!(!auto_aim_enabled_on_start());
    }

    #[cfg(feature = "learning-release")]
    #[test]
    fn learning_build_is_locked_but_exposes_teaching_controls_and_truth() {
        assert!(is_locked());
        assert!(is_learning_release());
        assert!(allows_local_manual_controls());
        assert!(!auto_aim_enabled_on_start());
        assert!(allows_online_ground_truth());
    }

    #[cfg(all(
        feature = "distribution-release",
        not(feature = "contest-release"),
        not(feature = "learning-release")
    ))]
    #[test]
    fn ordinary_distribution_build_remains_sdk_command_driven() {
        assert!(is_locked());
        assert!(!allows_local_manual_controls());
        assert!(auto_aim_enabled_on_start());
    }
}
