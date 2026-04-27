#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cat > native/macos/ffi-headers/engine_api.h <<'HEADER'
#ifndef TRANSLATOR_ENGINE_API_H
#define TRANSLATOR_ENGINE_API_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct EngineHandle EngineHandle;

typedef enum EngineMode {
    ENGINE_MODE_BYPASS = 0,
    ENGINE_MODE_TRANSLATE = 1,
    ENGINE_MODE_CAPTION_ONLY = 2,
    ENGINE_MODE_MUTE_ON_FAILURE = 3,
    ENGINE_MODE_FALLBACK_TO_BYPASS = 4
} EngineMode;

EngineHandle *engine_create(const char *config_json);
void engine_destroy(EngineHandle *handle);

int32_t engine_start(EngineHandle *handle);
int32_t engine_stop(EngineHandle *handle);
int32_t engine_set_target_language(EngineHandle *handle, const char *lang);
int32_t engine_set_mode(EngineHandle *handle, int32_t mode);
int32_t engine_enable_shared_output(EngineHandle *handle, int32_t capacity_frames, int32_t channels, int32_t sample_rate);

int32_t engine_push_input_pcm(
    EngineHandle *handle,
    const float *samples,
    int32_t frame_count,
    int32_t channels,
    int32_t sample_rate,
    uint64_t timestamp_ns
);

int32_t engine_push_translated_pcm(
    EngineHandle *handle,
    const float *samples,
    int32_t frame_count,
    int32_t channels,
    int32_t sample_rate,
    uint64_t timestamp_ns
);

int32_t engine_pull_output_pcm(
    EngineHandle *handle,
    float *out_samples,
    int32_t max_frames,
    int32_t channels,
    int32_t sample_rate,
    uint64_t *out_timestamp_ns
);

int32_t engine_read_shared_output_pcm(
    EngineHandle *handle,
    float *out_samples,
    int32_t max_frames,
    int32_t channels,
    uint64_t *out_timestamp_ns
);

const char *engine_get_last_error(EngineHandle *handle);
const char *engine_get_metrics_json(EngineHandle *handle);
const char *engine_get_shared_output_path(EngineHandle *handle);

#ifdef __cplusplus
}
#endif

#endif
HEADER
