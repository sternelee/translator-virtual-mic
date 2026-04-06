#ifndef TRANSLATOR_VIRTUAL_MIC_RENDER_SOURCE_H
#define TRANSLATOR_VIRTUAL_MIC_RENDER_SOURCE_H

#include <cstddef>
#include <cstdint>
#include <string>

#include "Support/shared_buffer_reader.h"

struct TranslatorVirtualMicRenderResult {
    std::size_t frames_produced;
    std::size_t frames_silence_filled;
    std::uint64_t timestamp_ns;
    bool source_available;
    bool format_matches;
};

class TranslatorVirtualMicRenderSource {
public:
    TranslatorVirtualMicRenderSource(
        std::uint32_t expected_sample_rate,
        std::uint32_t expected_channel_count,
        std::string file_path = TVM_SHARED_BUFFER_FILE_PATH);

    const SharedBufferReader &reader() const;
    TranslatorVirtualMicRenderResult render(float *out_samples, std::size_t max_frames) const;
    bool probe_format(TvmSharedBufferHeader &header) const;

private:
    bool validate_format(const TvmSharedBufferHeader &header) const;

    SharedBufferReader reader_;
    std::uint32_t expected_sample_rate_;
    std::uint32_t expected_channel_count_;
};

#endif
