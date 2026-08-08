#include <daedalus_sim_sdk/runtime_capabilities.hpp>

#include <cstdio>
#include <filesystem>
#include <fstream>

using namespace daedalus::sim::sdk::v1;

int main() {
  const auto directory = std::filesystem::path("daedalus_sdk_caps_test");
  std::filesystem::create_directories(directory);
  const auto path = directory / std::string(kRuntimeCapabilitiesFileName);
  {
    std::ofstream output(path, std::ios::binary | std::ios::trunc);
    output << R"({
      "schema_version": 1,
      "product_version": "1.2.0",
      "distribution_locked": true,
      "adapter_selection": "wgpu-high-performance",
      "render_backend": "Vulkan",
      "adapter_name": "Test GPU",
      "vendor_id": 4318,
      "device_id": 1234,
      "device_type": "DiscreteGpu",
      "driver": "test-driver",
      "driver_info": "1.2.3"
    })";
  }
  const auto result = readRuntimeCapabilities(directory.string());
  std::filesystem::remove_all(directory);
  if (!result) return 1;
  if (!result.value->distribution_locked) return 2;
  if (result.value->render_backend != "Vulkan") return 3;
  if (result.value->adapter_name != "Test GPU") return 4;
  if (result.value->vendor_id != 4318) return 5;
  return 0;
}
