#include <daedalus_sim_sdk/scene_control_client.hpp>

#include "socket_platform.hpp"

#include <algorithm>
#include <cctype>
#include <cmath>
#include <limits>
#include <sstream>

namespace daedalus::sim::sdk::v1 {
namespace {

std::string escapeJson(const std::string& value) {
  std::ostringstream output;
  static constexpr char kHex[] = "0123456789abcdef";
  for (const unsigned char ch : value) {
    switch (ch) {
      case '"': output << "\\\""; break;
      case '\\': output << "\\\\"; break;
      case '\b': output << "\\b"; break;
      case '\f': output << "\\f"; break;
      case '\n': output << "\\n"; break;
      case '\r': output << "\\r"; break;
      case '\t': output << "\\t"; break;
      default:
        if (ch < 0x20) {
          output << "\\u00" << kHex[ch >> 4U] << kHex[ch & 0x0fU];
        } else {
          output << static_cast<char>(ch);
        }
    }
  }
  return output.str();
}

bool isJsonObject(const std::string& json) {
  std::size_t begin = 0;
  while (begin < json.size() &&
         std::isspace(static_cast<unsigned char>(json[begin]))) {
    ++begin;
  }
  std::size_t end = json.size();
  while (end > begin &&
         std::isspace(static_cast<unsigned char>(json[end - 1]))) {
    --end;
  }
  if (end - begin < 2 || json[begin] != '{' || json[end - 1] != '}') {
    return false;
  }
  bool in_string = false;
  bool escaped = false;
  int object_depth = 0;
  int array_depth = 0;
  for (std::size_t i = begin; i < end; ++i) {
    const char ch = json[i];
    if (in_string) {
      if (escaped) {
        escaped = false;
      } else if (ch == '\\') {
        escaped = true;
      } else if (ch == '"') {
        in_string = false;
      } else if (static_cast<unsigned char>(ch) < 0x20) {
        return false;
      }
      continue;
    }
    if (ch == '"') {
      in_string = true;
    } else if (ch == '{') {
      ++object_depth;
    } else if (ch == '}') {
      if (--object_depth < 0) return false;
    } else if (ch == '[') {
      ++array_depth;
    } else if (ch == ']') {
      if (--array_depth < 0) return false;
    }
  }
  return !in_string && !escaped && object_depth == 0 && array_depth == 0;
}

void skipWhitespace(const std::string& json, std::size_t* cursor) {
  while (*cursor < json.size() &&
         std::isspace(static_cast<unsigned char>(json[*cursor]))) {
    ++*cursor;
  }
}

bool parseJsonString(const std::string& json, std::size_t* cursor,
                     std::string* output) {
  skipWhitespace(json, cursor);
  if (*cursor >= json.size() || json[*cursor] != '"') return false;
  ++*cursor;
  output->clear();
  while (*cursor < json.size()) {
    const char ch = json[(*cursor)++];
    if (ch == '"') return true;
    if (ch != '\\') {
      if (static_cast<unsigned char>(ch) < 0x20) return false;
      output->push_back(ch);
      continue;
    }
    if (*cursor >= json.size()) return false;
    const char escaped = json[(*cursor)++];
    switch (escaped) {
      case '"': output->push_back('"'); break;
      case '\\': output->push_back('\\'); break;
      case '/': output->push_back('/'); break;
      case 'b': output->push_back('\b'); break;
      case 'f': output->push_back('\f'); break;
      case 'n': output->push_back('\n'); break;
      case 'r': output->push_back('\r'); break;
      case 't': output->push_back('\t'); break;
      default: return false;
    }
  }
  return false;
}

bool locateFieldValue(const std::string& json, const std::string& field,
                      std::size_t* cursor) {
  std::size_t position = 0;
  while (position < json.size()) {
    if (json[position] != '"') {
      ++position;
      continue;
    }
    std::string candidate;
    if (!parseJsonString(json, &position, &candidate)) return false;
    skipWhitespace(json, &position);
    if (position >= json.size() || json[position] != ':') continue;
    ++position;
    if (candidate == field) {
      skipWhitespace(json, &position);
      *cursor = position;
      return true;
    }
  }
  return false;
}

bool stringField(const std::string& json, const std::string& field,
                 std::string* value) {
  std::size_t cursor = 0;
  return locateFieldValue(json, field, &cursor) &&
         parseJsonString(json, &cursor, value);
}

bool uint64Field(const std::string& json, const std::string& field,
                 std::uint64_t* value) {
  std::size_t cursor = 0;
  if (!locateFieldValue(json, field, &cursor) || cursor >= json.size() ||
      !std::isdigit(static_cast<unsigned char>(json[cursor]))) {
    return false;
  }
  std::uint64_t parsed = 0;
  while (cursor < json.size() &&
         std::isdigit(static_cast<unsigned char>(json[cursor]))) {
    const std::uint64_t digit = static_cast<unsigned>(json[cursor] - '0');
    if (parsed > (std::numeric_limits<std::uint64_t>::max() - digit) / 10U) {
      return false;
    }
    parsed = parsed * 10U + digit;
    ++cursor;
  }
  if (cursor < json.size() &&
      (json[cursor] == '.' || json[cursor] == 'e' || json[cursor] == 'E')) {
    return false;
  }
  *value = parsed;
  return true;
}

ClientResult<SceneControlStatus> parseStatus(const std::string& value) {
  if (value == "ok")
    return ClientResult<SceneControlStatus>::success(SceneControlStatus::Ok);
  if (value == "invalid_request")
    return ClientResult<SceneControlStatus>::success(
        SceneControlStatus::InvalidRequest);
  if (value == "unsupported")
    return ClientResult<SceneControlStatus>::success(
        SceneControlStatus::Unsupported);
  if (value == "not_ready")
    return ClientResult<SceneControlStatus>::success(
        SceneControlStatus::NotReady);
  if (value == "internal_error")
    return ClientResult<SceneControlStatus>::success(
        SceneControlStatus::InternalError);
  return ClientResult<SceneControlStatus>::failure(
      ClientError::ProtocolError, "unknown scene-control status: " + value);
}

}  // namespace

ClientResult<std::string> buildSceneControlRequest(
    std::uint64_t command_id, const std::string& session_id,
    const std::string& op, const std::string& args_json) {
  if (command_id == 0) {
    return ClientResult<std::string>::failure(
        ClientError::InvalidArgument, "command_id must be non-zero");
  }
  if (op.empty()) {
    return ClientResult<std::string>::failure(ClientError::InvalidArgument,
                                               "op must not be empty");
  }
  if (!isJsonObject(args_json)) {
    return ClientResult<std::string>::failure(
        ClientError::InvalidArgument, "args must be one JSON object");
  }
  std::ostringstream json;
  json << "{\"protocol\":\"" << kSceneControlProtocol
       << "\",\"command_id\":" << command_id << ",\"session_id\":\""
       << escapeJson(session_id) << "\",\"op\":\"" << escapeJson(op)
       << "\",\"args\":" << args_json << '}';
  return ClientResult<std::string>::success(json.str());
}

ClientResult<std::string> encodeSetSceneArgs(SceneMode mode) {
  const char* value = "armor";
  switch (mode) {
    case SceneMode::Armor: value = "armor"; break;
    case SceneMode::Energy: value = "energy"; break;
    case SceneMode::Outpost: value = "outpost"; break;
    case SceneMode::ShootingRange: value = "shooting_range"; break;
  }
  return ClientResult<std::string>::success(
      std::string("{\"scene\":\"") + value + "\"}");
}

ClientResult<std::string> encodeRangeTargetMotionArgs(
    const RangeTargetMotion& motion) {
  if ((motion.target != 1 && motion.target != 3) ||
      !std::isfinite(motion.direction_deg) ||
      !std::isfinite(motion.linear_speed_mps) ||
      !std::isfinite(motion.linear_span_m) ||
      !std::isfinite(motion.spin_deg_s) || motion.linear_speed_mps < 0.0F ||
      motion.linear_span_m < 0.0F) {
    return ClientResult<std::string>::failure(
        ClientError::InvalidArgument, "invalid range target motion");
  }
  const char* mode = "stationary";
  switch (motion.mode) {
    case RangeMotionMode::Stationary: mode = "stationary"; break;
    case RangeMotionMode::Linear: mode = "linear"; break;
    case RangeMotionMode::Spin: mode = "spin"; break;
    case RangeMotionMode::LinearAndSpin: mode = "linear_and_spin"; break;
  }
  std::ostringstream json;
  json << "{\"target\":" << static_cast<unsigned>(motion.target)
       << ",\"mode\":\"" << mode << "\",\"direction_deg\":"
       << motion.direction_deg << ",\"linear_speed_mps\":"
       << motion.linear_speed_mps << ",\"linear_span_m\":"
       << motion.linear_span_m << ",\"spin_deg_s\":" << motion.spin_deg_s
       << '}';
  return ClientResult<std::string>::success(json.str());
}

ClientResult<std::string> encodeRangeTargetGeometryArgs(
    const RangeTargetGeometry& geometry) {
  if ((geometry.target != 1 && geometry.target != 3) ||
      !std::isfinite(geometry.radial_scale) ||
      geometry.radial_scale < 0.75F || geometry.radial_scale > 1.25F) {
    return ClientResult<std::string>::failure(
        ClientError::InvalidArgument,
        "invalid range target geometry radial_scale");
  }
  std::ostringstream json;
  json << "{\"target\":" << static_cast<unsigned>(geometry.target)
       << ",\"radial_scale\":" << geometry.radial_scale << '}';
  return ClientResult<std::string>::success(json.str());
}

ClientResult<std::string> encodeRuneStateArgs(const RuneState& state) {
  const char* mode = state.mode == RuneMode::Small
                         ? "small"
                         : state.mode == RuneMode::Large ? "large" : "off";
  if (state.mode == RuneMode::Off &&
      (!state.pending_targets.empty() || !state.activated_targets.empty())) {
    return ClientResult<std::string>::failure(
        ClientError::InvalidArgument, "off rune mode requires empty targets");
  }
  auto valid = [](const std::vector<std::uint8_t>& values) {
    for (std::size_t i = 0; i < values.size(); ++i) {
      if (values[i] >= 5) return false;
      for (std::size_t j = i + 1; j < values.size(); ++j) {
        if (values[i] == values[j]) return false;
      }
    }
    return true;
  };
  if (!valid(state.pending_targets) || !valid(state.activated_targets)) {
    return ClientResult<std::string>::failure(
        ClientError::InvalidArgument, "rune targets must be unique indices 0..4");
  }
  for (const auto pending : state.pending_targets) {
    for (const auto activated : state.activated_targets) {
      if (pending == activated) {
        return ClientResult<std::string>::failure(
            ClientError::InvalidArgument, "pending and activated targets overlap");
      }
    }
  }
  auto append = [](std::ostringstream& json,
                   const std::vector<std::uint8_t>& values) {
    json << '[';
    for (std::size_t i = 0; i < values.size(); ++i) {
      if (i != 0) json << ',';
      json << static_cast<unsigned>(values[i]);
    }
    json << ']';
  };
  std::ostringstream json;
  json << "{\"mode\":\"" << mode << "\",\"pending_targets\":";
  append(json, state.pending_targets);
  json << ",\"activated_targets\":";
  append(json, state.activated_targets);
  json << '}';
  return ClientResult<std::string>::success(json.str());
}

ClientResult<SceneControlResponse> parseSceneControlResponse(
    const std::string& json, std::uint64_t expected_command_id,
    const std::string& expected_session_id) {
  if (!isJsonObject(json)) {
    return ClientResult<SceneControlResponse>::failure(
        ClientError::ProtocolError, "response is not a JSON object");
  }
  SceneControlResponse response{};
  std::string status;
  if (!stringField(json, "protocol", &response.protocol) ||
      !uint64Field(json, "command_id", &response.command_id) ||
      !stringField(json, "session_id", &response.session_id) ||
      !stringField(json, "status", &status) ||
      !uint64Field(json, "applied_frame_seq", &response.applied_frame_seq) ||
      !uint64Field(json, "timestamp_ns", &response.timestamp_ns) ||
      !stringField(json, "message", &response.message)) {
    return ClientResult<SceneControlResponse>::failure(
        ClientError::ProtocolError, "response is missing a required field");
  }
  if (response.protocol != kSceneControlProtocol) {
    return ClientResult<SceneControlResponse>::failure(
        ClientError::ProtocolError, "scene-control protocol mismatch");
  }
  if (response.command_id != expected_command_id) {
    return ClientResult<SceneControlResponse>::failure(
        ClientError::ProtocolError, "scene-control command_id mismatch");
  }
  if (response.session_id != expected_session_id) {
    return ClientResult<SceneControlResponse>::failure(
        ClientError::ProtocolError, "scene-control session_id mismatch");
  }
  auto parsed_status = parseStatus(status);
  if (!parsed_status) {
    return ClientResult<SceneControlResponse>::failure(
        parsed_status.status.error, parsed_status.status.message);
  }
  response.status = *parsed_status.value;
  return ClientResult<SceneControlResponse>::success(std::move(response));
}

SceneControlClient::SceneControlClient(SceneControlOptions options)
    : options_(std::move(options)),
      next_command_id_(options_.first_command_id) {}

ClientResult<SceneControlResponse> SceneControlClient::request(
    const std::string& op, const std::string& args_json) {
  const std::uint64_t command_id =
      next_command_id_.fetch_add(1, std::memory_order_relaxed);
  auto request_json =
      buildSceneControlRequest(command_id, options_.session_id, op, args_json);
  if (!request_json) {
    return ClientResult<SceneControlResponse>::failure(
        request_json.status.error, request_json.status.message);
  }
  auto address = detail::resolveIpv4(options_.endpoint, SOCK_DGRAM, IPPROTO_UDP);
  if (!address) {
    return ClientResult<SceneControlResponse>::failure(
        address.status.error, address.status.message);
  }
  detail::SocketHandle socket = ::socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
  if (socket == detail::kInvalidSocket) {
    return ClientResult<SceneControlResponse>::failure(
        ClientError::SocketCreateFailed, detail::socketErrorMessage("socket"));
  }
  auto close = [&] { detail::closeSocket(socket); };
  auto timeout = detail::setReceiveTimeout(
      socket, static_cast<int>(options_.timeout.count()));
  if (!timeout) {
    close();
    return ClientResult<SceneControlResponse>::failure(timeout.error,
                                                       timeout.message);
  }
  const std::string& payload = *request_json.value;
  const int sent = ::sendto(
      socket, payload.data(), static_cast<int>(payload.size()), 0,
      reinterpret_cast<const sockaddr*>(&address.value->storage),
      address.value->length);
  if (sent != static_cast<int>(payload.size())) {
    const auto message = detail::socketErrorMessage("sendto");
    close();
    return ClientResult<SceneControlResponse>::failure(ClientError::SendFailed,
                                                       message);
  }
  char response_buffer[65536];
  const int received = ::recvfrom(socket, response_buffer,
                                  static_cast<int>(sizeof(response_buffer)), 0,
                                  nullptr, nullptr);
  if (received < 0) {
    const auto message = detail::socketErrorMessage("recvfrom");
    close();
    return ClientResult<SceneControlResponse>::failure(ClientError::Timeout,
                                                       message);
  }
  close();
  return parseSceneControlResponse(
      std::string(response_buffer, response_buffer + received), command_id,
      options_.session_id);
}

ClientResult<SceneControlResponse> SceneControlClient::ping(
    const std::string& args_json) {
  return request("ping", args_json);
}
ClientResult<SceneControlResponse> SceneControlClient::createSession(
    const std::string& args_json) {
  return request("create_session", args_json);
}
ClientResult<SceneControlResponse> SceneControlClient::status(
    const std::string& args_json) {
  return request("status", args_json);
}
ClientResult<SceneControlResponse> SceneControlClient::resetScene(
    const std::string& args_json) {
  return request("reset_scene", args_json);
}
ClientResult<SceneControlResponse> SceneControlClient::setScene(
    const std::string& args_json) {
  return request("set_scene", args_json);
}
ClientResult<SceneControlResponse> SceneControlClient::setScene(SceneMode mode) {
  const auto args = encodeSetSceneArgs(mode);
  if (!args) return ClientResult<SceneControlResponse>::failure(
      args.status.error, args.status.message);
  return setScene(*args.value);
}
ClientResult<SceneControlResponse> SceneControlClient::setRangeTargetMotion(
    const std::string& args_json) {
  return request("set_range_target_motion", args_json);
}
ClientResult<SceneControlResponse> SceneControlClient::setRangeTargetMotion(
    const RangeTargetMotion& motion) {
  const auto args = encodeRangeTargetMotionArgs(motion);
  if (!args) return ClientResult<SceneControlResponse>::failure(
      args.status.error, args.status.message);
  return setRangeTargetMotion(*args.value);
}
ClientResult<SceneControlResponse> SceneControlClient::setRangeTargetGeometry(
    const std::string& args_json) {
  return request("set_range_target_geometry", args_json);
}
ClientResult<SceneControlResponse> SceneControlClient::setRangeTargetGeometry(
    const RangeTargetGeometry& geometry) {
  const auto args = encodeRangeTargetGeometryArgs(geometry);
  if (!args) return ClientResult<SceneControlResponse>::failure(
      args.status.error, args.status.message);
  return setRangeTargetGeometry(*args.value);
}
ClientResult<SceneControlResponse> SceneControlClient::setRuneState(
    const std::string& args_json) {
  return request("set_rune_state", args_json);
}
ClientResult<SceneControlResponse> SceneControlClient::setRuneState(
    const RuneState& state) {
  const auto args = encodeRuneStateArgs(state);
  if (!args) return ClientResult<SceneControlResponse>::failure(
      args.status.error, args.status.message);
  return setRuneState(*args.value);
}

}  // namespace daedalus::sim::sdk::v1
