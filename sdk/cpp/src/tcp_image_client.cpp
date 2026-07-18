#include <daedalus_sim_sdk/tcp_image_client.hpp>

#include "socket_platform.hpp"

#include <atomic>
#include <condition_variable>
#include <mutex>
#include <thread>
#include <utility>

namespace daedalus::sim::sdk::v1 {

class TcpImageClient::Impl {
 public:
  explicit Impl(UdpEndpoint endpoint) : endpoint_(std::move(endpoint)) {}

  ~Impl() { close(); }

  ClientStatus connectClient() {
    std::lock_guard<std::mutex> lifecycle_lock(lifecycle_mutex_);
    if (connected_.load(std::memory_order_acquire)) {
      return ClientStatus::success();
    }
    if (receiver_.joinable()) receiver_.join();

    auto address = detail::resolveIpv4(endpoint_, SOCK_STREAM, IPPROTO_TCP);
    if (!address) return address.status;
    detail::SocketHandle socket = ::socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (socket == detail::kInvalidSocket) {
      return ClientStatus::failure(ClientError::SocketCreateFailed,
                                   detail::socketErrorMessage("socket"));
    }
    if (::connect(socket,
                  reinterpret_cast<const sockaddr*>(&address.value->storage),
                  address.value->length) != 0) {
      const auto message = detail::socketErrorMessage("connect");
      detail::closeSocket(socket);
      return ClientStatus::failure(ClientError::ConnectFailed, message);
    }

    stopping_.store(false, std::memory_order_release);
    socket_.store(socket, std::memory_order_release);
    connected_.store(true, std::memory_order_release);
    {
      std::lock_guard<std::mutex> frame_lock(frame_mutex_);
      terminal_status_ = ClientStatus::success();
      latest_.reset();
    }
    receiver_ = std::thread([this, socket] { receiverLoop(socket); });
    return ClientStatus::success();
  }

  void close() noexcept {
    std::lock_guard<std::mutex> lifecycle_lock(lifecycle_mutex_);
    stopping_.store(true, std::memory_order_release);
    const auto socket = socket_.exchange(detail::kInvalidSocket,
                                         std::memory_order_acq_rel);
    if (socket != detail::kInvalidSocket) {
      detail::shutdownSocket(socket);
      detail::closeSocket(socket);
    }
    if (receiver_.joinable()) receiver_.join();
    connected_.store(false, std::memory_order_release);
    frame_cv_.notify_all();
  }

  bool connected() const noexcept {
    return connected_.load(std::memory_order_acquire);
  }

  ClientResult<TcpImageFrame> latest() const {
    std::lock_guard<std::mutex> lock(frame_mutex_);
    if (latest_) return ClientResult<TcpImageFrame>::success(*latest_);
    if (!terminal_status_) {
      return ClientResult<TcpImageFrame>::failure(terminal_status_.error,
                                                  terminal_status_.message);
    }
    return ClientResult<TcpImageFrame>::failure(
        ClientError::NotReady, "no complete TCP image frame is available");
  }

  ClientResult<TcpImageFrame> waitForLatest(
      std::uint64_t after_source_sequence,
      std::chrono::milliseconds timeout) const {
    std::unique_lock<std::mutex> lock(frame_mutex_);
    const bool ready = frame_cv_.wait_for(lock, timeout, [&] {
      return (latest_ &&
              latest_->header.source_sequence > after_source_sequence) ||
             !terminal_status_.ok();
    });
    if (latest_ && latest_->header.source_sequence > after_source_sequence) {
      return ClientResult<TcpImageFrame>::success(*latest_);
    }
    if (!terminal_status_) {
      return ClientResult<TcpImageFrame>::failure(terminal_status_.error,
                                                  terminal_status_.message);
    }
    return ClientResult<TcpImageFrame>::failure(
        ready ? ClientError::ReceiveFailed : ClientError::Timeout,
        ready ? "TCP image receiver stopped" : "timed out waiting for image");
  }

 private:
  void receiverLoop(detail::SocketHandle socket) {
    ClientStatus terminal = ClientStatus::success();
    while (!stopping_.load(std::memory_order_acquire)) {
      tcp_image::WireHeader wire{};
      terminal = detail::receiveAll(socket, wire.data(), wire.size());
      if (!terminal) break;
      const auto decoded = tcp_image::decodeHeader(wire.data(), wire.size());
      if (!decoded.ok()) {
        terminal = ClientStatus::failure(
            ClientError::ProtocolError,
            "invalid TCP image frame header status " +
                std::to_string(static_cast<int>(decoded.status)));
        break;
      }
      if (decoded.header.payload_bytes > tcp_image::kMaxPayloadBytes) {
        terminal = ClientStatus::failure(ClientError::PayloadTooLarge,
                                         "TCP image payload exceeds SDK limit");
        break;
      }
      TcpImageFrame frame{};
      frame.header = decoded.header;
      frame.payload.resize(decoded.header.payload_bytes);
      terminal = detail::receiveAll(socket, frame.payload.data(),
                                    frame.payload.size());
      if (!terminal) break;
      {
        std::lock_guard<std::mutex> lock(frame_mutex_);
        latest_ = std::move(frame);
      }
      frame_cv_.notify_all();
    }

    connected_.store(false, std::memory_order_release);
    const auto owned = socket_.exchange(detail::kInvalidSocket,
                                        std::memory_order_acq_rel);
    if (owned != detail::kInvalidSocket) detail::closeSocket(owned);
    {
      std::lock_guard<std::mutex> lock(frame_mutex_);
      if (!stopping_.load(std::memory_order_acquire)) {
        terminal_status_ = terminal;
      }
    }
    frame_cv_.notify_all();
  }

  UdpEndpoint endpoint_;
  mutable std::mutex lifecycle_mutex_;
  mutable std::mutex frame_mutex_;
  mutable std::condition_variable frame_cv_;
  std::thread receiver_;
  std::atomic<detail::SocketHandle> socket_{detail::kInvalidSocket};
  std::atomic<bool> connected_{false};
  std::atomic<bool> stopping_{false};
  std::optional<TcpImageFrame> latest_;
  ClientStatus terminal_status_;
};

TcpImageClient::TcpImageClient(UdpEndpoint endpoint)
    : impl_(std::make_unique<Impl>(std::move(endpoint))) {}
TcpImageClient::~TcpImageClient() = default;
TcpImageClient::TcpImageClient(TcpImageClient&&) noexcept = default;
TcpImageClient& TcpImageClient::operator=(TcpImageClient&&) noexcept = default;
ClientStatus TcpImageClient::connect() { return impl_->connectClient(); }
void TcpImageClient::close() noexcept { impl_->close(); }
bool TcpImageClient::connected() const noexcept { return impl_->connected(); }
ClientResult<TcpImageFrame> TcpImageClient::latest() const {
  return impl_->latest();
}
ClientResult<TcpImageFrame> TcpImageClient::waitForLatest(
    std::uint64_t after_source_sequence,
    std::chrono::milliseconds timeout) const {
  return impl_->waitForLatest(after_source_sequence, timeout);
}

}  // namespace daedalus::sim::sdk::v1
