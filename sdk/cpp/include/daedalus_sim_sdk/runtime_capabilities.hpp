#pragma once

#include <daedalus_sim_sdk/client_result.hpp>
#include <daedalus_sim_sdk/endpoints_v1.hpp>

#include <cstdint>
#include <string>

namespace daedalus::sim::sdk::v1 {

struct RuntimeCapabilities {
  std::uint32_t schema_version = 0;
  std::string product_version;
  bool distribution_locked = false;
  std::string adapter_selection;
  std::string render_backend;
  std::string adapter_name;
  std::uint32_t vendor_id = 0;
  std::uint32_t device_id = 0;
  std::string device_type;
  std::string driver;
  std::string driver_info;
};

// Reads the actual adapter selected by wgpu after renderer initialization.
// Pass TALOS_IPC_DIR, not the full JSON path.
[[nodiscard]] ClientResult<RuntimeCapabilities> readRuntimeCapabilities(
    const std::string& ipc_directory);

}  // namespace daedalus::sim::sdk::v1
