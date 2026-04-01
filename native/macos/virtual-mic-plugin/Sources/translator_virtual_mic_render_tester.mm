#include "translator_virtual_mic_render_source.h"

#include <algorithm>
#include <cstdlib>
#include <iostream>
#include <vector>

int main(int argc, char **argv) {
    const std::string file_path = argc > 1 ? argv[1] : TVM_SHARED_BUFFER_FILE_PATH;
    TranslatorVirtualMicRenderSource render_source(48000, 1, file_path);

    TvmSharedBufferHeader header {};
    const bool format_ok = render_source.probe_format(header);
    std::vector<float> samples(16, 0.0f);
    const TranslatorVirtualMicRenderResult result = render_source.render(samples.data(), samples.size());

    std::cout << "file_path=" << file_path << '\n';
    std::cout << "format_ok=" << (format_ok ? "true" : "false") << '\n';
    std::cout << "header_sample_rate=" << header.sample_rate << '\n';
    std::cout << "header_channel_count=" << header.channel_count << '\n';
    std::cout << "frames_produced=" << result.frames_produced << '\n';
    std::cout << "frames_silence_filled=" << result.frames_silence_filled << '\n';
    std::cout << "timestamp_ns=" << result.timestamp_ns << '\n';
    std::cout << "format_matches=" << (result.format_matches ? "true" : "false") << '\n';
    std::cout << "first_samples=";
    for (std::size_t index = 0; index < std::min<std::size_t>(samples.size(), 8); ++index) {
        if (index > 0) {
            std::cout << ',';
        }
        std::cout << samples[index];
    }
    std::cout << '\n';

    return format_ok && result.format_matches ? EXIT_SUCCESS : EXIT_FAILURE;
}
