use std::ffi::OsString;
use std::sync::OnceLock;

use crate::config::SimulationConfig;

const RELEASE_MODE_ENV: &str = "DAEDALUS_RELEASE_MODE";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseMode {
    Performance,
    Visible,
}

static RELEASE_MODE: OnceLock<ReleaseMode> = OnceLock::new();

pub const fn is_locked() -> bool {
    cfg!(feature = "distribution-release")
}

pub fn prepare_environment() {
    if !is_locked() {
        return;
    }

    let mode = std::env::var(RELEASE_MODE_ENV)
        .ok()
        .filter(|value| value.eq_ignore_ascii_case("visible"))
        .map(|_| ReleaseMode::Visible)
        .unwrap_or(ReleaseMode::Performance);
    let _ = RELEASE_MODE.set(mode);

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
    set_env("DAEDALUS_AUTO_AIM_ON_START", "1");
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
    config.auto_aim.enabled_on_start = true;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_build_is_not_distribution_locked() {
        if !cfg!(feature = "distribution-release") {
            assert!(!is_locked());
        }
    }
}
