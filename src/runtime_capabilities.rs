use bevy::prelude::*;
use bevy::render::renderer::RenderAdapterInfo;
use bevy::render::{RenderApp, RenderStartup};
use serde::Serialize;
use std::path::PathBuf;

pub const RUNTIME_CAPABILITIES_FILE: &str = "daedalus-runtime-capabilities-v1.json";

#[derive(Serialize)]
struct RuntimeCapabilities<'a> {
    schema_version: u32,
    product_version: &'a str,
    distribution_locked: bool,
    distribution_profile: &'a str,
    competition_eligible: bool,
    online_ground_truth_enabled: bool,
    future_truth_included: bool,
    adapter_selection: &'a str,
    render_backend: String,
    adapter_name: &'a str,
    vendor_id: u32,
    device_id: u32,
    device_type: String,
    driver: &'a str,
    driver_info: &'a str,
}

pub struct RuntimeCapabilitiesPlugin;

impl Plugin for RuntimeCapabilitiesPlugin {
    fn build(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            warn!("render app unavailable; GPU runtime capabilities will not be published");
            return;
        };
        render_app.add_systems(RenderStartup, publish_runtime_capabilities);
    }
}

fn publish_runtime_capabilities(adapter: Res<RenderAdapterInfo>) {
    let capabilities = RuntimeCapabilities {
        schema_version: 1,
        product_version: env!("CARGO_PKG_VERSION"),
        distribution_locked: crate::distribution::is_locked(),
        distribution_profile: if crate::distribution::is_learning_release() {
            "learning"
        } else if crate::distribution::is_contest_release() {
            "contest"
        } else {
            "internal-lab"
        },
        competition_eligible: crate::distribution::is_contest_release(),
        online_ground_truth_enabled: crate::distribution::allows_online_ground_truth(),
        future_truth_included: false,
        adapter_selection: "wgpu-high-performance",
        render_backend: format!("{:?}", adapter.backend),
        adapter_name: &adapter.name,
        vendor_id: adapter.vendor,
        device_id: adapter.device,
        device_type: format!("{:?}", adapter.device_type),
        driver: &adapter.driver,
        driver_info: &adapter.driver_info,
    };
    let Ok(json) = serde_json::to_vec_pretty(&capabilities) else {
        error!("failed to serialize GPU runtime capabilities");
        return;
    };
    let directory = runtime_directory();
    if let Err(error) = std::fs::create_dir_all(&directory) {
        error!(
            "failed to create runtime directory {}: {error}",
            directory.display()
        );
        return;
    }
    let path = directory.join(RUNTIME_CAPABILITIES_FILE);
    let temporary = directory.join(format!("{RUNTIME_CAPABILITIES_FILE}.tmp"));
    if let Err(error) =
        std::fs::write(&temporary, json).and_then(|_| replace_file(&temporary, &path))
    {
        error!(
            "failed to publish GPU runtime capabilities {}: {error}",
            path.display()
        );
        let _ = std::fs::remove_file(&temporary);
        return;
    }
    info!(
        "selected GPU adapter name={:?} backend={:?} device_type={:?} vendor={:#06x} device={:#06x}; capabilities={}",
        adapter.name,
        adapter.backend,
        adapter.device_type,
        adapter.vendor,
        adapter.device,
        path.display()
    );
}

fn runtime_directory() -> PathBuf {
    std::env::var_os("TALOS_IPC_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("talos-ipc"))
}

fn replace_file(temporary: &std::path::Path, destination: &std::path::Path) -> std::io::Result<()> {
    #[cfg(windows)]
    if destination.exists() {
        std::fs::remove_file(destination)?;
    }
    std::fs::rename(temporary, destination)
}
