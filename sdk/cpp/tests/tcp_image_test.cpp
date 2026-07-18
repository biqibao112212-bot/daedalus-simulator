#include <daedalus_sim_sdk/tcp_image_v1.hpp>

using namespace daedalus::sim::sdk::v1;

int main() {
  tcp_image::FrameHeader header{};
  header.format = tcp_image::PixelFormat::Rgba32;
  header.width = kImageWidth;
  header.height = kImageHeight;
  header.payload_bytes = kImageWidth * kImageHeight * 4U;
  header.producer_epoch = 11;
  header.source_sequence = 22;
  header.capture_timestamp_ns = 33;
  tcp_image::WireHeader wire{};
  if (tcp_image::encodeHeader(header, &wire) != tcp_image::HeaderStatus::Ok) {
    return 1;
  }
  const auto decoded = tcp_image::decodeHeader(wire.data(), wire.size());
  if (!decoded.ok() || decoded.header.width != kImageWidth ||
      decoded.header.height != kImageHeight ||
      decoded.header.source_sequence != 22) {
    return 2;
  }
  wire[0] = 0;
  return tcp_image::decodeHeader(wire.data(), wire.size()).status ==
                 tcp_image::HeaderStatus::InvalidMagic
             ? 0
             : 3;
}
