#pragma once

#include <daedalus_sim_sdk/talos_v1.hpp>

#include <array>
#include <cstddef>
#include <cstdint>

namespace daedalus::sim::sdk::v1::tcp_image {

inline constexpr std::uint32_t kWireMagic = 0x54494d47U;
inline constexpr std::uint16_t kWireVersion = 1;
inline constexpr std::uint16_t kWireHeaderBytes = 64;
inline constexpr std::size_t kMaxPayloadBytes =
    static_cast<std::size_t>(kImageWidth) * kImageHeight * 4U;

enum class PixelFormat : std::uint16_t { Rgb24 = 1, Rgba32 = 2 };

struct FrameHeader {
  PixelFormat format = PixelFormat::Rgb24;
  std::uint16_t flags = 0;
  std::uint32_t width = 0;
  std::uint32_t height = 0;
  std::uint32_t payload_bytes = 0;
  std::uint64_t producer_epoch = 0;
  std::uint64_t source_sequence = 0;
  std::uint64_t capture_timestamp_ns = 0;
  std::uint64_t reserved0 = 0;
  std::uint64_t reserved1 = 0;
};

using WireHeader = std::array<std::uint8_t, kWireHeaderBytes>;

enum class HeaderStatus {
  Ok,
  NullOutput,
  WireSizeMismatch,
  InvalidMagic,
  UnsupportedVersion,
  InvalidHeaderBytes,
  UnsupportedFormat,
  NonzeroFlags,
  InvalidDimensions,
  InvalidPayloadBytes,
  InvalidIdentity,
  NonzeroReserved,
};

struct HeaderDecodeResult {
  HeaderStatus status = HeaderStatus::WireSizeMismatch;
  FrameHeader header{};
  [[nodiscard]] bool ok() const noexcept { return status == HeaderStatus::Ok; }
};

[[nodiscard]] inline std::uint32_t channelsFor(PixelFormat format) noexcept {
  switch (format) {
    case PixelFormat::Rgb24:
      return 3U;
    case PixelFormat::Rgba32:
      return 4U;
  }
  return 0U;
}

[[nodiscard]] inline bool checkedPayloadBytes(
    std::uint32_t width, std::uint32_t height, PixelFormat format,
    std::uint32_t* payload_bytes) noexcept {
  if (payload_bytes == nullptr || width == 0 || height == 0 ||
      width > kImageWidth || height > kImageHeight) {
    return false;
  }
  const std::uint32_t channels = channelsFor(format);
  const std::uint64_t bytes = static_cast<std::uint64_t>(width) * height * channels;
  if (channels == 0 || bytes == 0 || bytes > kMaxPayloadBytes) {
    return false;
  }
  *payload_bytes = static_cast<std::uint32_t>(bytes);
  return true;
}

[[nodiscard]] inline HeaderStatus validateHeader(
    const FrameHeader& header) noexcept {
  if (header.flags != 0) return HeaderStatus::NonzeroFlags;
  std::uint32_t expected = 0;
  if (!checkedPayloadBytes(header.width, header.height, header.format, &expected)) {
    return HeaderStatus::InvalidDimensions;
  }
  if (header.payload_bytes != expected) return HeaderStatus::InvalidPayloadBytes;
  if (header.producer_epoch == 0 || header.source_sequence == 0 ||
      header.capture_timestamp_ns == 0) {
    return HeaderStatus::InvalidIdentity;
  }
  if (header.reserved0 != 0 || header.reserved1 != 0) {
    return HeaderStatus::NonzeroReserved;
  }
  return HeaderStatus::Ok;
}

namespace detail {
inline void writeU16(WireHeader& wire, std::size_t offset,
                     std::uint16_t value) noexcept {
  wire[offset] = static_cast<std::uint8_t>(value >> 8U);
  wire[offset + 1U] = static_cast<std::uint8_t>(value);
}
inline void writeU32(WireHeader& wire, std::size_t offset,
                     std::uint32_t value) noexcept {
  for (std::size_t i = 0; i < 4U; ++i) {
    wire[offset + i] = static_cast<std::uint8_t>(value >> ((3U - i) * 8U));
  }
}
inline void writeU64(WireHeader& wire, std::size_t offset,
                     std::uint64_t value) noexcept {
  for (std::size_t i = 0; i < 8U; ++i) {
    wire[offset + i] = static_cast<std::uint8_t>(value >> ((7U - i) * 8U));
  }
}
inline std::uint16_t readU16(const std::uint8_t* wire,
                             std::size_t offset) noexcept {
  return static_cast<std::uint16_t>((wire[offset] << 8U) | wire[offset + 1U]);
}
inline std::uint32_t readU32(const std::uint8_t* wire,
                             std::size_t offset) noexcept {
  std::uint32_t value = 0;
  for (std::size_t i = 0; i < 4U; ++i) value = (value << 8U) | wire[offset + i];
  return value;
}
inline std::uint64_t readU64(const std::uint8_t* wire,
                             std::size_t offset) noexcept {
  std::uint64_t value = 0;
  for (std::size_t i = 0; i < 8U; ++i) value = (value << 8U) | wire[offset + i];
  return value;
}
}  // namespace detail

[[nodiscard]] inline HeaderStatus encodeHeader(
    const FrameHeader& header, WireHeader* wire) noexcept {
  if (wire == nullptr) return HeaderStatus::NullOutput;
  const HeaderStatus status = validateHeader(header);
  if (status != HeaderStatus::Ok) return status;
  wire->fill(0);
  detail::writeU32(*wire, 0, kWireMagic);
  detail::writeU16(*wire, 4, kWireVersion);
  detail::writeU16(*wire, 6, kWireHeaderBytes);
  detail::writeU16(*wire, 8, static_cast<std::uint16_t>(header.format));
  detail::writeU16(*wire, 10, header.flags);
  detail::writeU32(*wire, 12, header.width);
  detail::writeU32(*wire, 16, header.height);
  detail::writeU32(*wire, 20, header.payload_bytes);
  detail::writeU64(*wire, 24, header.producer_epoch);
  detail::writeU64(*wire, 32, header.source_sequence);
  detail::writeU64(*wire, 40, header.capture_timestamp_ns);
  return HeaderStatus::Ok;
}

[[nodiscard]] inline HeaderDecodeResult decodeHeader(
    const std::uint8_t* wire, std::size_t wire_size) noexcept {
  HeaderDecodeResult result{};
  if (wire == nullptr || wire_size != kWireHeaderBytes) return result;
  if (detail::readU32(wire, 0) != kWireMagic) {
    result.status = HeaderStatus::InvalidMagic;
    return result;
  }
  if (detail::readU16(wire, 4) != kWireVersion) {
    result.status = HeaderStatus::UnsupportedVersion;
    return result;
  }
  if (detail::readU16(wire, 6) != kWireHeaderBytes) {
    result.status = HeaderStatus::InvalidHeaderBytes;
    return result;
  }
  const auto raw_format = detail::readU16(wire, 8);
  if (raw_format != static_cast<std::uint16_t>(PixelFormat::Rgb24) &&
      raw_format != static_cast<std::uint16_t>(PixelFormat::Rgba32)) {
    result.status = HeaderStatus::UnsupportedFormat;
    return result;
  }
  result.header.format = static_cast<PixelFormat>(raw_format);
  result.header.flags = detail::readU16(wire, 10);
  result.header.width = detail::readU32(wire, 12);
  result.header.height = detail::readU32(wire, 16);
  result.header.payload_bytes = detail::readU32(wire, 20);
  result.header.producer_epoch = detail::readU64(wire, 24);
  result.header.source_sequence = detail::readU64(wire, 32);
  result.header.capture_timestamp_ns = detail::readU64(wire, 40);
  result.header.reserved0 = detail::readU64(wire, 48);
  result.header.reserved1 = detail::readU64(wire, 56);
  result.status = validateHeader(result.header);
  return result;
}

}  // namespace daedalus::sim::sdk::v1::tcp_image
