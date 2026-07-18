#pragma once

#include <daedalus_sim_sdk/client_result.hpp>

#include <cstddef>
#include <cstdint>
#include <string>

#ifdef _WIN32
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <winsock2.h>
#include <ws2tcpip.h>
#else
#include <arpa/inet.h>
#include <netdb.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <unistd.h>
#endif

namespace daedalus::sim::sdk::v1::detail {

#ifdef _WIN32
using SocketHandle = SOCKET;
inline constexpr SocketHandle kInvalidSocket = INVALID_SOCKET;
#else
using SocketHandle = int;
inline constexpr SocketHandle kInvalidSocket = -1;
#endif

ClientStatus ensureSocketRuntime();
void closeSocket(SocketHandle socket) noexcept;
void shutdownSocket(SocketHandle socket) noexcept;
std::string socketErrorMessage(const char* operation);

struct ResolvedAddress {
  sockaddr_storage storage{};
  socklen_t length = 0;
};

ClientResult<ResolvedAddress> resolveIpv4(const UdpEndpoint& endpoint,
                                          int socket_type,
                                          int protocol);
ClientStatus setReceiveTimeout(SocketHandle socket, int timeout_ms);
ClientStatus sendAll(SocketHandle socket, const std::uint8_t* data,
                     std::size_t size);
ClientStatus receiveAll(SocketHandle socket, std::uint8_t* data,
                        std::size_t size);

}  // namespace daedalus::sim::sdk::v1::detail
