#include "shared_buffer_protocol.h"
#include "Support/shared_buffer_reader.h"
#include "translator_virtual_mic_render_source.h"

// Placeholder for a future Audio Server Plug-in implementation.
// The real plug-in will read the file-backed shared output bridge and expose it
// as a virtual input device through HAL object callbacks.

namespace {
SharedBufferReader gSharedBufferReader;
TranslatorVirtualMicRenderSource gRenderSource(48000, 1);
}
