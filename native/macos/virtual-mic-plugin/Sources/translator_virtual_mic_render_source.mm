#include "translator_virtual_mic_render_source.h"

#include <algorithm>
#include <utility>

TranslatorVirtualMicRenderSource::TranslatorVirtualMicRenderSource(
    std::uint32_t expected_sample_rate,
    std::uint32_t expected_channel_count,
    std::string file_path)
    : reader_(std::move(file_path)),
      expected_sample_rate_(expected_sample_rate),
      expected_channel_count_(expected_channel_count) {}

const SharedBufferReader &TranslatorVirtualMicRenderSource::reader() const {
    return reader_;
}

TranslatorVirtualMicRenderResult TranslatorVirtualMicRenderSource::render(float *out_samples, std::size_t max_frames) const {
    TranslatorVirtualMicRenderResult result {};
    result.frames_produced = 0;
    result.frames_silence_filled = max_frames;
    result.timestamp_ns = 0;
    result.source_available = false;
    result.format_matches = false;

    if (out_samples == nullptr || max_frames == 0) {
        return result;
    }

    TvmSharedBufferHeader header {};
    if (!reader_.read_header(header)) {
        std::fill(out_samples, out_samples + max_frames, 0.0f);
        return result;
    }

    result.source_available = true;
    result.format_matches = validate_format(header);
    if (!result.format_matches) {
        std::fill(out_samples, out_samples + max_frames, 0.0f);
        result.timestamp_ns = header.last_timestamp_ns;
        return result;
    }

    result.frames_produced = reader_.consume_mono_frames(out_samples, max_frames, result.timestamp_ns);
    result.frames_silence_filled = max_frames > result.frames_produced ? max_frames - result.frames_produced : 0;
    return result;
}

bool TranslatorVirtualMicRenderSource::probe_format(TvmSharedBufferHeader &header) const {
    if (!reader_.read_header(header)) {
        return false;
    }
    return validate_format(header);
}

bool TranslatorVirtualMicRenderSource::validate_format(const TvmSharedBufferHeader &header) const {
    return header.magic == TVM_SHARED_BUFFER_MAGIC &&
        header.version == TVM_SHARED_BUFFER_VERSION &&
        header.sample_rate == expected_sample_rate_ &&
        header.channel_count == expected_channel_count_;
}
