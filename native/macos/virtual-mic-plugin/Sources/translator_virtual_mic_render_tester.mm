#include "translator_virtual_mic_render_source.h"

#include <algorithm>
#include <cstdlib>
#include <iostream>
#include <vector>

int main(int argc, char **argv) {
    const std::string file_path = argc > 1 ? argv[1] : TVM_SHARED_BUFFER_FILE_PATH;
    TranslatorVirtualMicRenderSource render_source(48000, 1, file_path);
    SharedBufferReader reader(file_path);

    TvmSharedBufferHeader header {};
    const bool format_ok = render_source.probe_format(header);
    std::vector<float> samples(16, 0.0f);
    const TranslatorVirtualMicRenderResult result = render_source.render(samples.data(), samples.size());
    TvmSharedBufferHeader consumed_header {};
    const bool consumed_header_ok = reader.read_header(consumed_header);

    std::cout << "file_path=" << file_path << '\n';
    std::cout << "format_ok=" << (format_ok ? "true" : "false") << '\n';
    std::cout << "header_sample_rate=" << header.sample_rate << '\n';
    std::cout << "header_channel_count=" << header.channel_count << '\n';
    std::cout << "frames_produced=" << result.frames_produced << '\n';
    std::cout << "frames_silence_filled=" << result.frames_silence_filled << '\n';
    std::cout << "timestamp_ns=" << result.timestamp_ns << '\n';
    std::cout << "source_available=" << (result.source_available ? "true" : "false") << '\n';
    std::cout << "format_matches=" << (result.format_matches ? "true" : "false") << '\n';
    std::cout << "read_index_after_render=" << (consumed_header_ok ? consumed_header.read_index_frames : 0) << '\n';
    std::cout << "write_index_at_render=" << result.write_index_frames << '\n';
    std::cout << "read_index_at_render=" << result.read_index_frames << '\n';
    std::cout << "first_samples=";
    for (std::size_t index = 0; index < std::min<std::size_t>(samples.size(), 8); ++index) {
        if (index > 0) {
            std::cout << ',';
        }
        std::cout << samples[index];
    }
    std::cout << '\n';

    return format_ok &&
            result.source_available &&
            result.format_matches &&
            consumed_header_ok &&
            result.frames_produced > 0
        ? EXIT_SUCCESS
        : EXIT_FAILURE;
}
