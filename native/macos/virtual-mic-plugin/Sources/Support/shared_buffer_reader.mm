#include "shared_buffer_reader.h"

#include <algorithm>
#include <cstring>
#include <fstream>
#include <utility>

namespace {
constexpr std::size_t header_size_bytes() {
    return (6 * sizeof(std::uint32_t)) + (3 * sizeof(std::uint64_t));
}

std::uint32_t read_u32_le(const unsigned char *data, std::size_t &cursor) {
    const std::uint32_t value = static_cast<std::uint32_t>(data[cursor]) |
        (static_cast<std::uint32_t>(data[cursor + 1]) << 8U) |
        (static_cast<std::uint32_t>(data[cursor + 2]) << 16U) |
        (static_cast<std::uint32_t>(data[cursor + 3]) << 24U);
    cursor += 4;
    return value;
}

std::uint64_t read_u64_le(const unsigned char *data, std::size_t &cursor) {
    std::uint64_t value = 0;
    for (std::size_t index = 0; index < 8; ++index) {
        value |= static_cast<std::uint64_t>(data[cursor + index]) << (index * 8U);
    }
    cursor += 8;
    return value;
}

float read_f32_le(const unsigned char *data) {
    std::uint32_t raw = static_cast<std::uint32_t>(data[0]) |
        (static_cast<std::uint32_t>(data[1]) << 8U) |
        (static_cast<std::uint32_t>(data[2]) << 16U) |
        (static_cast<std::uint32_t>(data[3]) << 24U);
    float value = 0.0f;
    static_assert(sizeof(float) == sizeof(std::uint32_t), "float size mismatch");
    std::memcpy(&value, &raw, sizeof(float));
    return value;
}

} // namespace

SharedBufferReader::SharedBufferReader(std::string file_path)
    : file_path_(std::move(file_path)) {}

const std::string &SharedBufferReader::file_path() const {
    return file_path_;
}

bool SharedBufferReader::read_header(TvmSharedBufferHeader &header) const {
    std::ifstream input(file_path_, std::ios::binary);
    if (!input.is_open()) {
        return false;
    }

    unsigned char bytes[header_size_bytes()] = {};
    input.read(reinterpret_cast<char *>(bytes), static_cast<std::streamsize>(sizeof(bytes)));
    if (input.gcount() != static_cast<std::streamsize>(sizeof(bytes))) {
        return false;
    }

    std::size_t cursor = 0;
    header.magic = read_u32_le(bytes, cursor);
    header.version = read_u32_le(bytes, cursor);
    header.channel_count = read_u32_le(bytes, cursor);
    header.sample_rate = read_u32_le(bytes, cursor);
    header.capacity_frames = read_u32_le(bytes, cursor);
    header.reserved = read_u32_le(bytes, cursor);
    header.write_index_frames = read_u64_le(bytes, cursor);
    header.read_index_frames = read_u64_le(bytes, cursor);
    header.last_timestamp_ns = read_u64_le(bytes, cursor);

    return header.magic == TVM_SHARED_BUFFER_MAGIC && header.version == TVM_SHARED_BUFFER_VERSION;
}

std::size_t SharedBufferReader::consume_mono_frames(float *out_samples, std::size_t max_frames, std::uint64_t &timestamp_ns) const {
    if (out_samples == nullptr) {
        timestamp_ns = 0;
        return 0;
    }

    std::ifstream input(file_path_, std::ios::binary);
    if (!input.is_open()) {
        timestamp_ns = 0;
        std::fill(out_samples, out_samples + max_frames, 0.0f);
        return 0;
    }

    unsigned char header_bytes[header_size_bytes()] = {};
    input.read(reinterpret_cast<char *>(header_bytes), static_cast<std::streamsize>(sizeof(header_bytes)));
    if (input.gcount() != static_cast<std::streamsize>(sizeof(header_bytes))) {
        timestamp_ns = 0;
        std::fill(out_samples, out_samples + max_frames, 0.0f);
        return 0;
    }

    TvmSharedBufferHeader header {};
    std::size_t cursor = 0;
    header.magic = read_u32_le(header_bytes, cursor);
    header.version = read_u32_le(header_bytes, cursor);
    header.channel_count = read_u32_le(header_bytes, cursor);
    header.sample_rate = read_u32_le(header_bytes, cursor);
    header.capacity_frames = read_u32_le(header_bytes, cursor);
    header.reserved = read_u32_le(header_bytes, cursor);
    header.write_index_frames = read_u64_le(header_bytes, cursor);
    header.read_index_frames = read_u64_le(header_bytes, cursor);
    header.last_timestamp_ns = read_u64_le(header_bytes, cursor);

    if (header.magic != TVM_SHARED_BUFFER_MAGIC || header.version != TVM_SHARED_BUFFER_VERSION) {
        timestamp_ns = header.last_timestamp_ns;
        std::fill(out_samples, out_samples + max_frames, 0.0f);
        return 0;
    }

    const std::size_t channels = std::max<std::size_t>(header.channel_count, 1);
    const std::size_t sample_count = static_cast<std::size_t>(header.capacity_frames) * channels;
    std::vector<unsigned char> bytes(sample_count * sizeof(float));
    input.read(reinterpret_cast<char *>(bytes.data()), static_cast<std::streamsize>(bytes.size()));
    if (input.gcount() != static_cast<std::streamsize>(bytes.size())) {
        timestamp_ns = header.last_timestamp_ns;
        std::fill(out_samples, out_samples + max_frames, 0.0f);
        return 0;
    }

    std::vector<float> samples(sample_count, 0.0f);
    for (std::size_t index = 0; index < sample_count; ++index) {
        samples[index] = read_f32_le(bytes.data() + (index * sizeof(float)));
    }

    const std::size_t total_frames = samples.size() / channels;
    const std::size_t available_frames = std::min<std::size_t>(
        total_frames,
        header.write_index_frames > header.read_index_frames
            ? static_cast<std::size_t>(header.write_index_frames - header.read_index_frames)
            : 0);
    const std::size_t frames_to_copy = std::min(max_frames, available_frames);
    const std::size_t capacity_frames = static_cast<std::size_t>(header.capacity_frames);
    const std::size_t start_frame = capacity_frames == 0
        ? 0
        : static_cast<std::size_t>((header.write_index_frames - frames_to_copy) % capacity_frames);

    for (std::size_t frame = 0; frame < frames_to_copy; ++frame) {
        const std::size_t source_frame = capacity_frames == 0
            ? frame
            : (start_frame + frame) % capacity_frames;
        out_samples[frame] = samples[source_frame * channels];
    }
    for (std::size_t frame = frames_to_copy; frame < max_frames; ++frame) {
        out_samples[frame] = 0.0f;
    }

    timestamp_ns = header.last_timestamp_ns;
    return frames_to_copy;
}

std::vector<float> SharedBufferReader::read_all_samples(TvmSharedBufferHeader &header) const {
    std::ifstream input(file_path_, std::ios::binary);
    if (!input.is_open()) {
        return {};
    }
    if (!read_header(header)) {
        return {};
    }

    input.seekg(static_cast<std::streamoff>(header_size_bytes()), std::ios::beg);
    const std::size_t sample_count = static_cast<std::size_t>(header.capacity_frames) * std::max<std::size_t>(header.channel_count, 1);
    std::vector<unsigned char> bytes(sample_count * sizeof(float));
    input.read(reinterpret_cast<char *>(bytes.data()), static_cast<std::streamsize>(bytes.size()));
    if (input.gcount() != static_cast<std::streamsize>(bytes.size())) {
        return {};
    }

    std::vector<float> samples(sample_count, 0.0f);
    for (std::size_t index = 0; index < sample_count; ++index) {
        samples[index] = read_f32_le(bytes.data() + (index * sizeof(float)));
    }
    return samples;
}
