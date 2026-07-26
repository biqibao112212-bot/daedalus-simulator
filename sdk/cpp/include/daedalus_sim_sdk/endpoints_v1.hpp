#pragma once

#include <cstdint>
#include <string_view>

namespace daedalus::sim::sdk::v1 {

inline constexpr std::string_view kMetaFileName = "talos_ipc_meta";
inline constexpr std::string_view kImagePoolFileName = "talos_ipc_image_pool";
inline constexpr std::string_view kRuntimeCapabilitiesFileName =
    "daedalus-runtime-capabilities-v1.json";
inline constexpr std::string_view kIpcDirectoryEnvironment = "TALOS_IPC_DIR";
inline constexpr std::string_view kImageTransportEnvironment =
    "DAEDALUS_TALOS_IMAGE_TRANSPORT";
inline constexpr std::string_view kTcpImageBindEnvironment =
    "DAEDALUS_TALOS_TCP_BIND";
inline constexpr std::uint16_t kTcpImagePort = 5602;
inline constexpr std::uint16_t kUdpCommandPort = 5601;
inline constexpr std::uint16_t kUdpSceneControlPort = 5603;
inline constexpr std::string_view kDefaultHost = "127.0.0.1";

}  // namespace daedalus::sim::sdk::v1
