#ifndef TRANSLATOR_VIRTUAL_MIC_SHARED_BUFFER_H
#define TRANSLATOR_VIRTUAL_MIC_SHARED_BUFFER_H

#include <stdint.h>

#define TVM_SHARED_BUFFER_MAGIC 0x314D5654u
#define TVM_SHARED_BUFFER_VERSION 1u
#define TVM_SHARED_BUFFER_NAME "translator_virtual_mic_output"
#define TVM_SHARED_BUFFER_FILE_PATH "/tmp/translator_virtual_mic/shared_output.bin"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct TvmSharedBufferHeader {
    uint32_t magic;
    uint32_t version;
    uint32_t channel_count;
    uint32_t sample_rate;
    uint32_t capacity_frames;
    uint32_t reserved;
    uint64_t write_index_frames;
    uint64_t read_index_frames;
    uint64_t last_timestamp_ns;
} TvmSharedBufferHeader;

#ifdef __cplusplus
}
#endif

#endif
