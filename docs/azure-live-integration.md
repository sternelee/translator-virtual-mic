# Azure Voice Live Integration

## Goal

Add a real-time speech-to-speech translation path that:

1. captures microphone PCM locally
2. streams audio to Azure Voice Live
3. receives translated target-language audio
4. converts translated audio into the virtual mic output format
5. writes translated PCM into the shared buffer consumed by the HAL plug-in

## Current status

Implemented in this phase:

- Azure Voice Live configuration parsing in `common`
- Azure Voice Live protocol bootstrap helpers in `session-core`
  - websocket URL builder
  - `session.update` event builder
  - `response.create` event builder
  - `input_audio_buffer.append` event builder for PCM16 audio
  - PCM float32 <-> PCM16 conversion helpers

Not yet implemented in this phase:

- websocket transport
- auth header injection for live sessions
- streaming receive loop for translated audio
- translated audio writeback into `output_ring` and shared output

## Recommended architecture

### Transport

- create a dedicated Azure Voice Live client task or thread
- open one websocket session per active engine session
- keep the websocket lifetime aligned with `engine_start` / `engine_stop`

### Upstream audio

- convert local mono float32 PCM into PCM16
- stream short audio chunks with `input_audio_buffer.append`
- use `input_audio_buffer.commit` only when server VAD is disabled

### Downstream audio

- accept translated PCM16 audio deltas from the service
- convert to float32
- resample to `48_000 Hz` when required
- push into `output_ring`
- mirror into shared output for the virtual microphone

## Config fields

Suggested runtime config keys:

- `translation_provider = "azure_voice_live"`
- `azure_voice_live_endpoint`
- `azure_voice_live_api_version`
- `azure_voice_live_model`
- `azure_voice_live_api_key`
- `azure_voice_live_voice_name`
- `azure_voice_live_source_locale`
- `azure_voice_live_target_locale`
- `azure_voice_live_enable_server_vad`

## References

- [Voice live API overview](https://learn.microsoft.com/azure/ai-services/speech-service/voice-assistants?tabs=jre)
- [Voice live quickstart](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/voice-live-quickstart)
- [Voice live how-to](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/voice-live-how-to)
