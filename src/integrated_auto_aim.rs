use bevy::prelude::*;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::config::{AutoAimConfig, SimulationConfig};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
use std::process::Command;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntegratedAutoAimBridgeStatus {
    Disabled,
    Starting,
    Running,
    Error,
}

impl Default for IntegratedAutoAimBridgeStatus {
    fn default() -> Self {
        Self::Disabled
    }
}

#[derive(Resource, Default)]
pub struct IntegratedAutoAimBridge {
    process: Option<ManagedBridgeProcess>,
    last_spawn_attempt: Option<Instant>,
    status: IntegratedAutoAimBridgeStatus,
}

struct ManagedBridgeProcess {
    child: Child,
    cleanup: BridgeCleanup,
}

struct BridgeCleanup {
    distro: String,
    token: String,
}

impl IntegratedAutoAimBridge {
    pub fn status_label(&self) -> &'static str {
        match self.status {
            IntegratedAutoAimBridgeStatus::Disabled => "OFF",
            IntegratedAutoAimBridgeStatus::Starting => "START",
            IntegratedAutoAimBridgeStatus::Running => "RUN",
            IntegratedAutoAimBridgeStatus::Error => "ERR",
        }
    }
}

impl Drop for IntegratedAutoAimBridge {
    fn drop(&mut self) {
        stop_bridge(&mut self.process);
    }
}

pub struct IntegratedAutoAimPlugin;

impl Plugin for IntegratedAutoAimPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<IntegratedAutoAimBridge>()
            .add_systems(Update, manage_integrated_auto_aim_bridge);
    }
}

pub fn configure_integrated_auto_aim_environment(config: &SimulationConfig) {
    if !config.auto_aim.managed_bridge_enabled {
        return;
    }

    let Some(ipc_dir) = resolve_workspace_path(config, &config.auto_aim.talos_ipc_dir) else {
        warn!("integrated auto-aim could not resolve Talos IPC directory");
        return;
    };

    if std::env::var_os("TALOS_IPC_DIR").is_none() {
        // This runs before Bevy spawns worker threads and before Talos IPC is created.
        unsafe {
            std::env::set_var("TALOS_IPC_DIR", &ipc_dir);
        }
        info!("integrated auto-aim TALOS_IPC_DIR={}", ipc_dir.display());
    }
}

fn manage_integrated_auto_aim_bridge(
    config: Res<SimulationConfig>,
    mut bridge: ResMut<IntegratedAutoAimBridge>,
) {
    if !config.auto_aim.managed_bridge_enabled {
        if bridge.process.is_some() {
            stop_bridge(&mut bridge.process);
        }
        bridge.status = IntegratedAutoAimBridgeStatus::Disabled;
        return;
    }

    if let Some(process) = bridge.process.as_mut() {
        match process.child.try_wait() {
            Ok(Some(status)) => {
                warn!("integrated auto-aim bridge exited: {}", status);
                bridge.process = None;
                bridge.status = IntegratedAutoAimBridgeStatus::Error;
            }
            Ok(None) => {
                bridge.status = IntegratedAutoAimBridgeStatus::Running;
                return;
            }
            Err(error) => {
                warn!("could not query integrated auto-aim bridge: {}", error);
                bridge.process = None;
                bridge.status = IntegratedAutoAimBridgeStatus::Error;
            }
        }
    }

    let now = Instant::now();
    if bridge
        .last_spawn_attempt
        .is_some_and(|last| now.duration_since(last) < Duration::from_secs(2))
    {
        return;
    }
    bridge.last_spawn_attempt = Some(now);
    bridge.status = IntegratedAutoAimBridgeStatus::Starting;

    match spawn_bridge(&config) {
        Ok(process) => {
            info!("integrated auto-aim bridge started; F5 gates aiming inside the simulator");
            bridge.process = Some(process);
            bridge.status = IntegratedAutoAimBridgeStatus::Running;
        }
        Err(error) => {
            warn!("failed to start integrated auto-aim bridge: {}", error);
            bridge.status = IntegratedAutoAimBridgeStatus::Error;
        }
    }
}

fn stop_bridge(process: &mut Option<ManagedBridgeProcess>) {
    if let Some(mut process) = process.take() {
        cleanup_bridge_processes(&process.cleanup);
        let _ = process.child.kill();
        let _ = process.child.wait();
    }
}

fn spawn_bridge(config: &SimulationConfig) -> Result<ManagedBridgeProcess, String> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = config;
        return Err("integrated WSL bridge launch is only supported on Windows".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        let auto_aim = &config.auto_aim;
        let workspace_dir = resolve_workspace_root(auto_aim)
            .ok_or_else(|| "could not resolve workspace_dir".to_string())?;
        let bridge_workdir = canonicalize_if_possible(resolve_relative_or_absolute(
            &workspace_dir,
            &auto_aim.bridge_workdir,
        ));
        // A diagnostic instance can select a private IPC directory so it never
        // attaches to another simulator's bridge.
        let ipc_dir = std::env::var_os("TALOS_IPC_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                resolve_relative_or_absolute(&workspace_dir, &auto_aim.talos_ipc_dir)
            });
        let bridge_script = bridge_workdir
            .join("scripts")
            .join("run_talos_bridge_wsl.sh");
        let log_path = bridge_workdir.join("build").join(format!(
            "integrated_talos_bridge_{}.log",
            bridge_mode(auto_aim)
        ));

        if !bridge_script.is_file() {
            return Err(format!(
                "bridge runner not found: {}",
                bridge_script.display()
            ));
        }

        std::fs::create_dir_all(ipc_dir.as_path())
            .map_err(|error| format!("could not create Talos IPC dir: {error}"))?;
        let ipc_dir = canonicalize_if_possible(ipc_dir);
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("could not create bridge log dir: {error}"))?;
        }

        let bridge_workdir_wsl = windows_path_to_wsl(&bridge_workdir).ok_or_else(|| {
            format!(
                "could not convert bridge workdir: {}",
                bridge_workdir.display()
            )
        })?;
        let ipc_dir_wsl = windows_path_to_wsl(&ipc_dir)
            .ok_or_else(|| format!("could not convert Talos IPC dir: {}", ipc_dir.display()))?;

        let mode = bridge_mode(auto_aim);
        let distro = bridge_distro(auto_aim);
        let token = bridge_token();
        cleanup_stale_bridge_processes(&distro);
        let enable_udp = if bridge_udp_enabled(
            std::env::var("AIM_SIM_ENABLE_UDP").ok().as_deref(),
            &auto_aim.command_transport,
        ) {
            "ON"
        } else {
            "OFF"
        };
        let bullet_speed = config.projectile.speed.to_string();
        let dual_focal = std::env::var("AIM_SIM_DUAL_FOCAL").unwrap_or_default();
        let wide_focal_mm = std::env::var("AIM_SIM_WIDE_FOCAL_MM").unwrap_or_default();
        let precision_focal_mm = std::env::var("AIM_SIM_PRECISION_FOCAL_MM").unwrap_or_default();
        let active_target_number =
            std::env::var("AIM_SIM_ACTIVE_TARGET_NUMBER").unwrap_or_default();
        let command_policy = std::env::var("AIM_SIM_COMMAND_POLICY").unwrap_or_default();
        let image_transport = bridge_image_transport();
        let tcp_image_port = talos_tcp_image_port().to_string();
        let tcp_image_host = std::env::var("AIM_SIM_TCP_IMAGE_HOST").unwrap_or_default();
        let enable_talos_command =
            std::env::var("AIM_SIM_ENABLE_TALOS_COMMAND").unwrap_or_default();
        let script = format!(
            "cd '{}' && export TALOS_IPC_DIR='{}' && export AIM_SIM_WITH_VIVSIONN_TRT=ON && export AIM_SIM_ENABLE_UDP='{}' && export AIM_SIM_IMAGE_TRANSPORT='{}' && export AIM_SIM_TCP_IMAGE_PORT='{}' && export AIM_SIM_TCP_IMAGE_HOST='{}' && export AIM_SIM_ENABLE_TALOS_COMMAND='{}' && export AIM_SIM_BULLET_SPEED_MPS='{}' && export AIM_SIM_DUAL_FOCAL='{}' && export AIM_SIM_WIDE_FOCAL_MM='{}' && export AIM_SIM_PRECISION_FOCAL_MM='{}' && export AIM_SIM_ACTIVE_TARGET_NUMBER='{}' && export AIM_SIM_COMMAND_POLICY='{}' && export DAEDALUS_BRIDGE_TOKEN='{}' && bash scripts/run_talos_bridge_wsl.sh '{}' > 'build/integrated_talos_bridge_{}.log' 2>&1",
            bash_single_quote(&bridge_workdir_wsl),
            bash_single_quote(&ipc_dir_wsl),
            enable_udp,
            bash_single_quote(&image_transport),
            bash_single_quote(&tcp_image_port),
            bash_single_quote(&tcp_image_host),
            bash_single_quote(&enable_talos_command),
            bash_single_quote(&bullet_speed),
            bash_single_quote(&dual_focal),
            bash_single_quote(&wide_focal_mm),
            bash_single_quote(&precision_focal_mm),
            bash_single_quote(&active_target_number),
            bash_single_quote(&command_policy),
            bash_single_quote(&token),
            bash_single_quote(&mode),
            bash_single_quote(&mode),
        );

        let mut command = Command::new("wsl.exe");
        command
            .arg("-d")
            .arg(&distro)
            .arg("--")
            .arg("bash")
            .arg("-lc")
            .arg(script)
            .creation_flags(CREATE_NO_WINDOW);

        let child = command
            .spawn()
            .map_err(|error| format!("could not spawn wsl.exe: {error}"))?;

        Ok(ManagedBridgeProcess {
            child,
            cleanup: BridgeCleanup { distro, token },
        })
    }
}

fn bridge_token() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("daedalus_auto_aim_{}_{}", std::process::id(), millis)
}

fn cleanup_bridge_processes(cleanup: &BridgeCleanup) {
    #[cfg(target_os = "windows")]
    {
        let script = format!("pkill -f '{}' || true", bash_single_quote(&cleanup.token));
        let _ = Command::new("wsl.exe")
            .arg("-d")
            .arg(&cleanup.distro)
            .arg("--")
            .arg("bash")
            .arg("-lc")
            .arg(script)
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = cleanup;
    }
}

fn cleanup_stale_bridge_processes(distro: &str) {
    #[cfg(target_os = "windows")]
    {
        let script = "pkill -f '[d]aedalus_auto_aim_' || true";
        let _ = Command::new("wsl.exe")
            .arg("-d")
            .arg(distro)
            .arg("--")
            .arg("bash")
            .arg("-lc")
            .arg(script)
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = distro;
    }
}

fn bridge_mode(config: &AutoAimConfig) -> String {
    std::env::var("DAEDALUS_AUTO_AIM_BRIDGE_MODE")
        .ok()
        .filter(|mode| !mode.trim().is_empty())
        .or_else(|| {
            std::env::var("DAEDALUS_AUTO_AIM_MODE")
                .ok()
                .filter(|mode| !mode.trim().is_empty())
        })
        .map(normalize_bridge_mode)
        .unwrap_or_else(|| normalize_bridge_mode(config.bridge_mode.clone()))
}

fn normalize_bridge_mode(mode: String) -> String {
    match mode.trim().to_ascii_lowercase().as_str() {
        "normal" | "default" | "map" | "arena" | "range" | "shooting_range" | "shooting-range"
        | "fps_range" | "fps-range" => "armor".to_string(),
        _ => mode,
    }
}

fn bridge_image_transport() -> String {
    std::env::var("DAEDALUS_TALOS_IMAGE_TRANSPORT")
        .ok()
        .and_then(|value| normalize_image_transport(&value))
        .unwrap_or_else(|| "file".to_string())
}

fn bridge_udp_enabled(override_value: Option<&str>, configured_transport: &str) -> bool {
    match override_value.map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if matches!(value.as_str(), "1" | "on" | "true" | "yes") => true,
        Some(value) if matches!(value.as_str(), "0" | "off" | "false" | "no") => false,
        _ => configured_transport.eq_ignore_ascii_case("udp"),
    }
}

fn talos_tcp_image_port() -> u16 {
    std::env::var("DAEDALUS_TALOS_TCP_BIND")
        .ok()
        .and_then(|value| tcp_port_from_bind(&value))
        .unwrap_or(5602)
}

fn normalize_image_transport(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    matches!(normalized.as_str(), "file" | "tcp").then_some(normalized)
}

fn tcp_port_from_bind(value: &str) -> Option<u16> {
    value
        .trim()
        .parse::<SocketAddr>()
        .ok()
        .map(|address| address.port())
        .filter(|port| *port != 0)
}

fn bridge_distro(config: &AutoAimConfig) -> String {
    std::env::var("DAEDALUS_AUTO_AIM_WSL_DISTRO")
        .ok()
        .filter(|distro| !distro.trim().is_empty())
        .unwrap_or_else(|| config.wsl_distro.clone())
}

fn resolve_workspace_path(config: &SimulationConfig, value: &str) -> Option<PathBuf> {
    resolve_workspace_path_from_config(&config.auto_aim, value)
}

fn resolve_workspace_path_from_config(config: &AutoAimConfig, value: &str) -> Option<PathBuf> {
    let workspace = resolve_workspace_root(config)?;
    Some(resolve_relative_or_absolute(&workspace, value))
}

fn resolve_workspace_root(config: &AutoAimConfig) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("DAEDALUS_WORKSPACE_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return Some(canonicalize_if_possible(path));
    }

    std::env::current_dir()
        .ok()
        .map(|cwd| {
            canonicalize_if_possible(resolve_config_path_from_cwd(&cwd, &config.workspace_dir))
        })
        .filter(|path| workspace_root_looks_valid(path))
        .or_else(default_workspace_root_from_manifest)
}

fn resolve_config_path_from_cwd(cwd: &Path, value: &str) -> PathBuf {
    resolve_relative_or_absolute(cwd, value)
}

fn default_workspace_root_from_manifest() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest_dir.parent()?.parent()?.to_path_buf();
    Some(canonicalize_if_possible(workspace)).filter(|path| workspace_root_looks_valid(path))
}

fn workspace_root_looks_valid(path: &Path) -> bool {
    path.join("aim_sim_bridge").is_dir() && path.join("upstream").join("daedalus").is_dir()
}

fn resolve_relative_or_absolute(base: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn canonicalize_if_possible(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

fn windows_path_to_wsl(path: &Path) -> Option<String> {
    let mut text = path.to_string_lossy().replace('\\', "/");
    if let Some(stripped) = text.strip_prefix("//?/") {
        text = stripped.to_string();
    }
    let bytes = text.as_bytes();
    if bytes.len() < 3 || bytes[1] != b':' {
        return None;
    }

    let drive = (bytes[0] as char).to_ascii_lowercase();
    Some(format!("/mnt/{drive}{}", &text[2..]))
}

fn bash_single_quote(value: &str) -> String {
    value.replace('\'', "'\\''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_drive_letter_path_to_wsl_path() {
        let path = PathBuf::from(r"D:\sim\aim_sim_bridge");
        assert_eq!(
            windows_path_to_wsl(&path).as_deref(),
            Some("/mnt/d/sim/aim_sim_bridge")
        );
    }

    #[test]
    fn converts_canonical_windows_path_to_wsl_path() {
        let path = PathBuf::from(r"\\?\D:\sim\aim_sim_bridge");
        assert_eq!(
            windows_path_to_wsl(&path).as_deref(),
            Some("/mnt/d/sim/aim_sim_bridge")
        );
    }

    #[test]
    fn escapes_bash_single_quotes() {
        assert_eq!(bash_single_quote("a'b"), "a'\\''b");
    }

    #[test]
    fn shooting_range_bridge_mode_uses_armor_detector() {
        assert_eq!(normalize_bridge_mode("shooting_range".to_string()), "armor");
        assert_eq!(normalize_bridge_mode("range".to_string()), "armor");
        assert_eq!(normalize_bridge_mode("normal".to_string()), "armor");
        assert_eq!(normalize_bridge_mode("outpost".to_string()), "outpost");
    }

    #[test]
    fn maps_explicit_tcp_transport_and_listener_port_to_wsl_bridge() {
        assert_eq!(normalize_image_transport(" TCP ").as_deref(), Some("tcp"));
        assert_eq!(normalize_image_transport("invalid"), None);
        assert_eq!(tcp_port_from_bind("0.0.0.0:5602"), Some(5602));
        assert_eq!(tcp_port_from_bind("127.0.0.1:0"), None);
        assert_eq!(tcp_port_from_bind("invalid"), None);
    }

    #[test]
    fn bridge_udp_override_is_explicit_and_defaults_to_configured_transport() {
        assert!(bridge_udp_enabled(None, "udp"));
        assert!(!bridge_udp_enabled(None, "talos"));
        assert!(!bridge_udp_enabled(Some("OFF"), "udp"));
        assert!(bridge_udp_enabled(Some("true"), "talos"));
    }

    #[test]
    fn resolves_bridge_workdir_under_workspace_root_once() {
        let config = AutoAimConfig {
            workspace_dir: "../..".to_string(),
            bridge_workdir: "aim_sim_bridge".to_string(),
            ..Default::default()
        };
        let cwd = PathBuf::from(r"D:\sim\upstream\daedalus");
        let workspace = resolve_config_path_from_cwd(&cwd, &config.workspace_dir);
        let bridge = resolve_relative_or_absolute(&workspace, &config.bridge_workdir);

        assert_eq!(
            windows_path_to_wsl(&bridge).as_deref(),
            Some("/mnt/d/sim/upstream/daedalus/../../aim_sim_bridge")
        );
    }
}
