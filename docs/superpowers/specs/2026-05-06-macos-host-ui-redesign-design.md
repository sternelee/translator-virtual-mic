# macOS Host App UI Redesign — Configuration Interaction

## Context

The current `ContentView.swift` uses a single `NavigationSplitView` with a long sidebar containing all controls (audio, provider, STT models, MT models, TTS models, Kokoro CoreML, ElevenLabs, MiniMax, Sidecar — all in one scroll). This makes the UI hard to navigate and important controls (Start/Stop, driver install) are buried.

Goal: better visual hierarchy — make the UI intuitive and reduce cognitive load.

---

## Design

### Layout: Tab-based navigation

Replace the single sidebar with a `TabView` in the detail pane. The sidebar becomes a compact control strip.

```
┌──────────────────────────────────────────────┬─────────────────────────┐
│ Translator Virtual Mic        [Start][Stop]  │                         │
├──────────────────────────────────────────────┤  Detail (Tab Content)   │
│ [Audio] [Provider] [Models] [TTS] [Debug]    │                         │
│                                              │                         │
│  ←  Tab bar + control buttons in the         │                         │
│     toolbar area, detail pane holds tab       │                         │
│     content                                   │                         │
└──────────────────────────────────────────────┴─────────────────────────┘
```

- **Toolbar** (`NavigationStack` title): "Translator Virtual Mic" + Start/Stop buttons + plugin install status
- **Tab bar** (horizontal): Audio | Provider | Models | TTS | Debug
- **Detail pane**: tab content, scrolled

### Tab 1 — Audio

Grouped vertically:
- **Device** — Picker + Refresh button
- **Levels** — Input meter bar (realtime)
- **Gain Controls** — Input Gain slider, Limiter Threshold slider
- **Status** — shared buffer status line (monospaced)

### Tab 2 — Provider

Card-based provider selection with rich cards:

```
┌──────────────────────────────────────────────────┐
│ 🔄 OpenAI Realtime                              │
│ Real-time speech-to-speech via OpenAI API       │
│ [Requires: OPENAI_API_KEY]                      │
│ Status: ● configured / ○ missing                │
└──────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────┐
│ ☁️  Azure Voice Live                             │
│ Azure speech-to-speech translation               │
│ [Requires: AZURE_VOICELIVE_API_KEY]              │
│ Status: ● configured / ○ missing                │
└──────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────┐
│ 📝 Local Caption                                │
│ VAD → STT → MT → TTS (fully local)             │
│ Works offline, no API key needed                │
│ Status: ● ready / ○ models needed               │
└──────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────┐
│ 🔇 Off (Passthrough)                            │
│ No translation, raw mic to virtual device        │
└──────────────────────────────────────────────────┘
```

Selected provider gets a highlighted border. Target language picker appears below selected card.

### Tab 3 — Models

Split into collapsible sections:

**Local STT**
- Model picker (downloaded status shown)
- Download / Delete button
- VAD model status + Download VAD button

**Local MT** (only when Local Caption selected)
- Apple Translation toggle (macOS 15+) with info text
- If disabled: model picker + Download/Delete
- Language direction mismatch warning

**Local TTS** (only when TTS mode = local)
- Model picker + Download/Delete
- Speed slider

### Tab 4 — TTS

TTS mode picker (segmented or list):
- None / Local / Kokoro CoreML / ElevenLabs / MiniMax / Sidecar

Per-mode configuration:
- **Local**: model picker, speed
- **Kokoro CoreML**: models directory, voice picker, speed
- **ElevenLabs**: env var status indicators (API key, voice ID)
- **MiniMax**: env var status indicators
- **Sidecar**: server URL, engine, voice name

### Tab 5 — Debug

- Logs (live scrollable, last 200 lines)
- Metrics JSON (copyable)
- Translation state JSON (copyable)
- Caption state JSON (copyable)
- Shared buffer path + status text

### Status Strip (always visible in sidebar or toolbar)

- Plugin install status dot (green/red)
- "Install Driver" or "Uninstall" button
- Input level meter (compact)

---

## Implementation Notes

- `@State private var selectedTab: Int = 0` in `ContentView`
- Use `TabView(selection: $selectedTab)` in the detail pane
- Each tab content becomes a separate `View` struct (e.g., `AudioTabView`, `ProviderTabView`, `ModelsTabView`, `TtsTabView`, `DebugTabView`)
- Preserve all existing `AppViewModel` logic and bindings — this is a pure layout refactor
- Keep existing `DetailPanel` content in the Debug tab
- Target language picker appears contextually in Provider tab for providers that need it

---

## Success Criteria

1. All existing functionality preserved (no behavior change)
2. Tab navigation works correctly
3. Provider card selection works with immediate visual feedback
4. Start/Stop buttons accessible from every tab
5. Plugin install/uninstall accessible from every tab
6. Input level meter visible at all times
7. All existing bindings and `AppViewModel` methods unchanged