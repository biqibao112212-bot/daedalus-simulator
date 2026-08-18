#include <daedalus_sim_sdk/runtime_capabilities.hpp>

#include <cctype>
#include <filesystem>
#include <fstream>
#include <iterator>
#include <limits>
#include <string_view>

namespace daedalus::sim::sdk::v1 {
namespace {

std::size_t valueStart(std::string_view json, std::string_view key) {
  const std::string token = "\"" + std::string(key) + "\"";
  const auto key_pos = json.find(token);
  if (key_pos == std::string_view::npos) return key_pos;
  const auto colon = json.find(':', key_pos + token.size());
  if (colon == std::string_view::npos) return colon;
  auto position = colon + 1;
  while (position < json.size() &&
         std::isspace(static_cast<unsigned char>(json[position]))) {
    ++position;
  }
  return position;
}

bool parseString(std::string_view json, std::string_view key,
                 std::string* output) {
  auto position = valueStart(json, key);
  if (position == std::string_view::npos || position >= json.size() ||
      json[position] != '"') {
    return false;
  }
  ++position;
  std::string value;
  while (position < json.size()) {
    const char ch = json[position++];
    if (ch == '"') {
      *output = std::move(value);
      return true;
    }
    if (ch != '\\') {
      value.push_back(ch);
      continue;
    }
    if (position >= json.size()) return false;
    const char escaped = json[position++];
    switch (escaped) {
      case '"': value.push_back('"'); break;
      case '\\': value.push_back('\\'); break;
      case '/': value.push_back('/'); break;
      case 'b': value.push_back('\b'); break;
      case 'f': value.push_back('\f'); break;
      case 'n': value.push_back('\n'); break;
      case 'r': value.push_back('\r'); break;
      case 't': value.push_back('\t'); break;
      default: return false;
    }
  }
  return false;
}

bool parseU32(std::string_view json, std::string_view key,
              std::uint32_t* output) {
  auto position = valueStart(json, key);
  if (position == std::string_view::npos || position >= json.size()) {
    return false;
  }
  std::uint64_t value = 0;
  bool found = false;
  while (position < json.size() && std::isdigit(
             static_cast<unsigned char>(json[position]))) {
    found = true;
    value = value * 10U + static_cast<unsigned>(json[position++] - '0');
    if (value > std::numeric_limits<std::uint32_t>::max()) return false;
  }
  if (!found) return false;
  *output = static_cast<std::uint32_t>(value);
  return true;
}

bool parseBool(std::string_view json, std::string_view key, bool* output) {
  const auto position = valueStart(json, key);
  if (position == std::string_view::npos) return false;
  if (json.substr(position, 4) == "true") {
    *output = true;
    return true;
  }
  if (json.substr(position, 5) == "false") {
    *output = false;
    return true;
  }
  return false;
}

}  // namespace

ClientResult<RuntimeCapabilities> readRuntimeCapabilities(
    const std::string& ipc_directory) {
  if (ipc_directory.empty()) {
    return ClientResult<RuntimeCapabilities>::failure(
        ClientError::InvalidArgument, "IPC directory must not be empty");
  }
  const auto path = std::filesystem::path(ipc_directory) /
                    std::string(kRuntimeCapabilitiesFileName);
  std::ifstream input(path, std::ios::binary);
  if (!input) {
    return ClientResult<RuntimeCapabilities>::failure(
        ClientError::ReceiveFailed,
        "runtime capabilities are not available; the renderer may still be starting");
  }
  const std::string json((std::istreambuf_iterator<char>(input)),
                         std::istreambuf_iterator<char>());
  RuntimeCapabilities value;
  const bool valid =
      parseU32(json, "schema_version", &value.schema_version) &&
      parseString(json, "product_version", &value.product_version) &&
      parseBool(json, "distribution_locked", &value.distribution_locked) &&
      parseString(json, "distribution_profile", &value.distribution_profile) &&
      parseBool(json, "competition_eligible", &value.competition_eligible) &&
      parseBool(json, "online_ground_truth_enabled", &value.online_ground_truth_enabled) &&
      parseBool(json, "future_truth_included", &value.future_truth_included) &&
      parseString(json, "adapter_selection", &value.adapter_selection) &&
      parseString(json, "render_backend", &value.render_backend) &&
      parseString(json, "adapter_name", &value.adapter_name) &&
      parseU32(json, "vendor_id", &value.vendor_id) &&
      parseU32(json, "device_id", &value.device_id) &&
      parseString(json, "device_type", &value.device_type) &&
      parseString(json, "driver", &value.driver) &&
      parseString(json, "driver_info", &value.driver_info);
  if (!valid || value.schema_version != 1) {
    return ClientResult<RuntimeCapabilities>::failure(
        ClientError::ProtocolError,
        "runtime capabilities JSON is malformed or unsupported");
  }
  return ClientResult<RuntimeCapabilities>::success(std::move(value));
}

}  // namespace daedalus::sim::sdk::v1
