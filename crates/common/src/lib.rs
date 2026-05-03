use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineMode {
    Bypass,
    Translate,
    CaptionOnly,
    MuteOnFailure,
    FallbackToBypass,
}

impl EngineMode {
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Bypass),
            1 => Some(Self::Translate),
            2 => Some(Self::CaptionOnly),
            3 => Some(Self::MuteOnFailure),
            4 => Some(Self::FallbackToBypass),
            _ => None,
        }
    }

    pub fn as_i32(&self) -> i32 {
        match self {
            Self::Bypass => 0,
            Self::Translate => 1,
            Self::CaptionOnly => 2,
            Self::MuteOnFailure => 3,
            Self::FallbackToBypass => 4,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TranslationProvider {
    None,
    AzureVoiceLive,
    OpenAIRealtime,
    ElevenLabs,
}

#[derive(Clone, Debug)]
pub struct AzureVoiceLiveConfig {
    pub endpoint: String,
    pub api_version: String,
    pub model: String,
    pub api_key: String,
    pub api_key_env: String,
    pub voice_name: String,
    pub voice_type: String,
    pub source_locale: String,
    pub target_locale: String,
    pub enable_server_vad: bool,
}

#[derive(Clone, Debug)]
pub struct OpenAIRealtimeConfig {
    pub endpoint: String,
    pub model: String,
    pub api_key: String,
    pub api_key_env: String,
    pub voice_name: String,
    pub source_locale: String,
    pub target_locale: String,
    pub enable_server_vad: bool,
}

#[derive(Clone, Debug)]
pub struct LocalSttConfig {
    pub enabled: bool,
    pub model_id: String,
    pub model_dir: PathBuf,
    pub vad_model_path: PathBuf,
    pub vad_threshold: f32,
    pub language: String,
    pub partial_interval_ms: u64,
    pub max_partial_window_seconds: f32,
    pub overlap_tail_ms: u64,
    pub skip_partial_if_busy: bool,
}

impl Default for LocalSttConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model_id: "paraformer-zh".to_string(),
            model_dir: PathBuf::from(""),
            vad_model_path: PathBuf::from(""),
            vad_threshold: 0.5,
            language: "auto".to_string(),
            partial_interval_ms: 500,
            max_partial_window_seconds: 5.0,
            overlap_tail_ms: 300,
            skip_partial_if_busy: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MtConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub api_key: String,
    pub api_key_env: String,
    pub model: String,
    pub target_language: String,
}

impl Default for MtConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            api_key: String::new(),
            api_key_env: "OPENAI_API_KEY".to_string(),
            model: "gpt-4o-mini".to_string(),
            target_language: "en".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TtsConfig {
    pub enabled: bool,
    /// "kokoro-en" | "kokoro-zh-en" | "melo-tts-zh" | "vits-mms-en" | custom
    pub model_id: String,
    pub model_dir: std::path::PathBuf,
    /// Speaker id (sid) — 0 for single-speaker models
    pub speaker_id: i32,
    /// Playback speed multiplier (1.0 = normal)
    pub speed: f32,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model_id: "kokoro-en".to_string(),
            model_dir: std::path::PathBuf::from(""),
            speaker_id: 0,
            speed: 1.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CosyVoiceTtsConfig {
    pub enabled: bool,
    /// HTTP base URL of the CosyVoice FastAPI server.
    pub endpoint: String,
    /// Path to reference voice WAV (5–10 s, mono 16kHz recommended).
    pub prompt_wav_path: std::path::PathBuf,
    /// Transcript of the reference WAV (improves naturalness; can be empty).
    pub prompt_text: String,
}

impl Default for CosyVoiceTtsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "http://127.0.0.1:50000".to_string(),
            prompt_wav_path: {
                let home = std::env::var("HOME").unwrap_or_default();
                std::path::PathBuf::from(home)
                    .join(".translator_virtual_mic")
                    .join("ref_voice.wav")
            },
            prompt_text: String::new(),
        }
    }
}

impl CosyVoiceTtsConfig {
    pub fn from_json_lossy(raw: &str, _engine_config: &EngineConfig) -> Option<Self> {
        let enabled = extract_bool_value(raw, "cosyvoice_tts_enabled").unwrap_or(false);
        let mentions = raw.contains("cosyvoice_tts_");
        if !enabled && !mentions {
            return None;
        }
        let mut cfg = Self::default();
        cfg.enabled = enabled;
        if let Some(ep) = extract_string_value(raw, "cosyvoice_tts_endpoint") {
            cfg.endpoint = ep;
        }
        if let Some(wav) = extract_string_value(raw, "cosyvoice_tts_prompt_wav_path") {
            cfg.prompt_wav_path = std::path::PathBuf::from(expand_tilde(&wav));
        }
        if let Some(pt) = extract_string_value(raw, "cosyvoice_tts_prompt_text") {
            cfg.prompt_text = pt;
        }
        Some(cfg)
    }
}

/// Config for ElevenLabs cloud TTS used inside the local caption pipeline.
#[derive(Clone, Debug)]
pub struct ElevenLabsTtsConfig {
    pub enabled: bool,
    /// ElevenLabs API key (`ELEVENLABS_API_KEY`).
    pub api_key: String,
    /// Voice ID of the cloned / chosen voice.
    pub voice_id: String,
    /// Model ID, e.g. `"eleven_multilingual_v2"` or `"eleven_turbo_v2_5"`.
    pub model_id: String,
}

impl Default for ElevenLabsTtsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_key: String::new(),
            voice_id: String::new(),
            model_id: "eleven_multilingual_v2".to_string(),
        }
    }
}

impl ElevenLabsTtsConfig {
    pub fn from_json_lossy(raw: &str) -> Option<Self> {
        let enabled = extract_bool_value(raw, "elevenlabs_tts_enabled").unwrap_or(false);
        let mentions = raw.contains("elevenlabs_tts_");
        if !enabled && !mentions {
            return None;
        }
        let mut cfg = Self::default();
        cfg.enabled = enabled;
        if let Some(k) = extract_string_value(raw, "elevenlabs_tts_api_key") {
            cfg.api_key = k;
        }
        if let Some(v) = extract_string_value(raw, "elevenlabs_tts_voice_id") {
            cfg.voice_id = v;
        }
        if let Some(m) = extract_string_value(raw, "elevenlabs_tts_model_id") {
            cfg.model_id = m;
        }
        Some(cfg)
    }
}

/// Config for MiniMax cloud TTS used inside the local caption pipeline.
#[derive(Clone, Debug)]
pub struct MiniMaxTtsConfig {
    pub enabled: bool,
    /// MiniMax API host, e.g. `"https://api.minimaxi.com"`.
    pub api_host: String,
    /// MiniMax API key (`MINIMAX_API_KEY`).
    pub api_key: String,
    /// Voice ID, e.g. `"male-qn-qingse"`.
    pub voice_id: String,
    /// Model ID, e.g. `"speech-01-turbo"`.
    pub model: String,
    /// Emotion: `"happy"`, `"sad"`, `"angry"`, etc.
    pub emotion: String,
    pub speed: f32,
    pub vol: f32,
    pub pitch: i32,
    pub sample_rate: u32,
}

impl Default for MiniMaxTtsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_host: String::new(),
            api_key: String::new(),
            voice_id: String::new(),
            model: String::new(),
            emotion: String::new(),
            speed: 1.0,
            vol: 1.0,
            pitch: 0,
            sample_rate: 24_000,
        }
    }
}

impl MiniMaxTtsConfig {
    pub fn from_json_lossy(raw: &str) -> Option<Self> {
        let enabled = extract_bool_value(raw, "minimax_tts_enabled").unwrap_or(false);
        let mentions = raw.contains("minimax_tts_");
        if !enabled && !mentions {
            return None;
        }
        let mut cfg = Self::default();
        cfg.enabled = enabled;
        if let Some(k) = extract_string_value(raw, "minimax_tts_api_key") {
            cfg.api_key = k;
        }
        if let Some(v) = extract_string_value(raw, "minimax_tts_voice_id") {
            cfg.voice_id = v;
        }
        if let Some(m) = extract_string_value(raw, "minimax_tts_model") {
            cfg.model = m;
        }
        if let Some(e) = extract_string_value(raw, "minimax_tts_emotion") {
            cfg.emotion = e;
        }
        if let Some(h) = extract_string_value(raw, "minimax_tts_api_host") {
            cfg.api_host = h;
        }
        if let Some(s) = extract_f32_value(raw, "minimax_tts_speed") {
            cfg.speed = s;
        }
        if let Some(v) = extract_f32_value(raw, "minimax_tts_vol") {
            cfg.vol = v;
        }
        if let Some(p) = extract_i32_value(raw, "minimax_tts_pitch") {
            cfg.pitch = p;
        }
        if let Some(sr) = extract_u32_value(raw, "minimax_tts_sample_rate") {
            cfg.sample_rate = sr;
        }
        Some(cfg)
    }
}

#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub source_language: String,
    pub target_language: String,
    pub input_sample_rate: u32,
    pub output_sample_rate: u32,
    pub channels: u16,
    pub input_gain_db: f32,
    pub limiter_threshold_db: f32,
    pub translation_provider: TranslationProvider,
    pub azure_voice_live: Option<AzureVoiceLiveConfig>,
    pub openai_realtime: Option<OpenAIRealtimeConfig>,
    pub local_stt: Option<LocalSttConfig>,
    pub mt: Option<MtConfig>,
    pub tts: Option<TtsConfig>,
    pub local_mt: Option<LocalMtConfig>,
    pub cosyvoice_tts: Option<CosyVoiceTtsConfig>,
    pub elevenlabs_tts: Option<ElevenLabsTtsConfig>,
    pub minimax_tts: Option<MiniMaxTtsConfig>,
    pub mode: EngineMode,
    pub raw_config_json: String,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            source_language: "auto".to_string(),
            target_language: "en".to_string(),
            input_sample_rate: 48_000,
            output_sample_rate: 48_000,
            channels: 1,
            input_gain_db: 0.0,
            limiter_threshold_db: -1.0,
            translation_provider: TranslationProvider::None,
            azure_voice_live: None,
            openai_realtime: None,
            local_stt: None,
            mt: None,
            tts: None,
            local_mt: None,
            cosyvoice_tts: None,
            elevenlabs_tts: None,
            minimax_tts: None,
            mode: EngineMode::Bypass,
            raw_config_json: "{}".to_string(),
        }
    }
}

impl EngineConfig {
    pub fn from_json_lossy(raw: &str) -> Self {
        let mut config = Self {
            raw_config_json: raw.to_string(),
            ..Self::default()
        };

        if raw.contains("\"fallback_mode\":\"mute\"") || raw.contains("fallback_mode = \"mute\"") {
            config.mode = EngineMode::MuteOnFailure;
        }
        if raw.contains("\"mode\":\"translate\"") || raw.contains("mode = \"translate\"") {
            config.mode = EngineMode::Translate;
        }
        if raw.contains("\"mode\":\"caption_only\"") || raw.contains("mode = \"caption_only\"") {
            config.mode = EngineMode::CaptionOnly;
        }
        if raw.contains("\"mode\":\"mute_on_failure\"")
            || raw.contains("mode = \"mute_on_failure\"")
        {
            config.mode = EngineMode::MuteOnFailure;
        }
        if raw.contains("\"mode\":\"fallback_to_bypass\"")
            || raw.contains("mode = \"fallback_to_bypass\"")
        {
            config.mode = EngineMode::FallbackToBypass;
        }
        if raw.contains("\"target\":\"zh\"") || raw.contains("target = \"zh\"") {
            config.target_language = "zh".to_string();
        }
        if let Some(input_gain_db) = extract_f32_value(raw, "input_gain_db") {
            config.input_gain_db = input_gain_db;
        }
        if let Some(limiter_threshold_db) = extract_f32_value(raw, "limiter_threshold_db") {
            config.limiter_threshold_db = limiter_threshold_db;
        }
        if let Some(provider) = extract_string_value(raw, "translation_provider") {
            config.translation_provider = match provider.as_str() {
                "azure_voice_live" => TranslationProvider::AzureVoiceLive,
                "openai_realtime" => TranslationProvider::OpenAIRealtime,
                "eleven_labs" => TranslationProvider::ElevenLabs,
                _ => TranslationProvider::None,
            };
        }
        if let Some(azure_voice_live) = AzureVoiceLiveConfig::from_json_lossy(raw, &config) {
            config.azure_voice_live = Some(azure_voice_live);
        }
        if let Some(openai_realtime) = OpenAIRealtimeConfig::from_json_lossy(raw, &config) {
            config.openai_realtime = Some(openai_realtime);
        }
        if let Some(local_stt) = LocalSttConfig::from_json_lossy(raw, &config) {
            config.local_stt = Some(local_stt);
        }
        if let Some(mt) = MtConfig::from_json_lossy(raw, &config) {
            config.mt = Some(mt);
        }
        if let Some(tts) = TtsConfig::from_json_lossy(raw, &config) {
            config.tts = Some(tts);
        }
        if let Some(local_mt) = LocalMtConfig::from_json_lossy(raw, &config) {
            config.local_mt = Some(local_mt);
        }
        if let Some(cosyvoice_tts) = CosyVoiceTtsConfig::from_json_lossy(raw, &config) {
            config.cosyvoice_tts = Some(cosyvoice_tts);
        }
        if let Some(elevenlabs_tts) = ElevenLabsTtsConfig::from_json_lossy(raw) {
            config.elevenlabs_tts = Some(elevenlabs_tts);
        }
        if let Some(minimax_tts) = MiniMaxTtsConfig::from_json_lossy(raw) {
            config.minimax_tts = Some(minimax_tts);
        }

        config
    }
}

impl AzureVoiceLiveConfig {
    pub fn from_json_lossy(raw: &str, engine_config: &EngineConfig) -> Option<Self> {
        let endpoint = extract_string_value(raw, "azure_voice_live_endpoint")?;
        let api_version = extract_string_value(raw, "azure_voice_live_api_version")
            .unwrap_or_else(|| "2025-10-01".to_string());
        let model = extract_string_value(raw, "azure_voice_live_model")
            .unwrap_or_else(|| "gpt-realtime".to_string());
        let api_key = extract_string_value(raw, "azure_voice_live_api_key").unwrap_or_default();
        let api_key_env = extract_string_value(raw, "azure_voice_live_api_key_env")
            .or_else(|| extract_string_value(raw, "api_key_env"))
            .unwrap_or_else(|| "AZURE_VOICELIVE_API_KEY".to_string());
        let voice_name =
            extract_string_value(raw, "azure_voice_live_voice_name").unwrap_or_else(|| {
                locale_default_voice(&azure_target_locale_from_config(raw, engine_config))
                    .to_string()
            });
        let voice_type = extract_string_value(raw, "azure_voice_live_voice_type")
            .unwrap_or_else(|| "azure-standard".to_string());
        let source_locale = extract_string_value(raw, "azure_voice_live_source_locale")
            .unwrap_or_else(|| azure_source_locale_from_config(raw, engine_config));
        let target_locale = azure_target_locale_from_config(raw, engine_config);
        let enable_server_vad =
            extract_bool_value(raw, "azure_voice_live_enable_server_vad").unwrap_or(true);

        Some(Self {
            endpoint,
            api_version,
            model,
            api_key,
            api_key_env,
            voice_name,
            voice_type,
            source_locale,
            target_locale,
            enable_server_vad,
        })
    }
}

impl OpenAIRealtimeConfig {
    pub fn from_json_lossy(raw: &str, engine_config: &EngineConfig) -> Option<Self> {
        let has_openai_settings =
            raw.contains("\"openai_realtime_") || raw.contains("openai_realtime_");
        if engine_config.translation_provider != TranslationProvider::OpenAIRealtime
            && !has_openai_settings
        {
            return None;
        }

        let endpoint = extract_string_value(raw, "openai_realtime_endpoint")
            .unwrap_or_else(|| "wss://api.openai.com/v1/realtime".to_string());
        let model = extract_string_value(raw, "openai_realtime_model")
            .unwrap_or_else(|| "gpt-realtime".to_string());
        let api_key = extract_string_value(raw, "openai_realtime_api_key").unwrap_or_default();
        let api_key_env = extract_string_value(raw, "openai_realtime_api_key_env")
            .or_else(|| extract_string_value(raw, "api_key_env"))
            .unwrap_or_else(|| "OPENAI_API_KEY".to_string());
        let voice_name = extract_string_value(raw, "openai_realtime_voice_name")
            .unwrap_or_else(|| "marin".to_string());
        let source_locale = extract_string_value(raw, "openai_realtime_source_locale")
            .unwrap_or_else(|| source_locale_from_config(raw, engine_config));
        let target_locale = extract_string_value(raw, "openai_realtime_target_locale")
            .unwrap_or_else(|| target_locale_from_config(raw, engine_config));
        let enable_server_vad =
            extract_bool_value(raw, "openai_realtime_enable_server_vad").unwrap_or(true);

        Some(Self {
            endpoint,
            model,
            api_key,
            api_key_env,
            voice_name,
            source_locale,
            target_locale,
            enable_server_vad,
        })
    }
}

impl LocalSttConfig {
    pub fn from_json_lossy(raw: &str, engine_config: &EngineConfig) -> Option<Self> {
        let enabled = extract_bool_value(raw, "local_stt_enabled").unwrap_or(false);
        let mentions_local_stt = raw.contains("local_stt_") || raw.contains("\"local_stt_");
        if !enabled && !mentions_local_stt {
            return None;
        }

        let mut cfg = Self::default();
        cfg.enabled = enabled;
        if let Some(model_id) = extract_string_value(raw, "local_stt_model_id") {
            cfg.model_id = model_id;
        }
        if let Some(model_dir) = extract_string_value(raw, "local_stt_model_dir") {
            cfg.model_dir = PathBuf::from(expand_tilde(&model_dir));
        }
        if let Some(vad_model_path) = extract_string_value(raw, "local_stt_vad_model_path") {
            cfg.vad_model_path = PathBuf::from(expand_tilde(&vad_model_path));
        }
        if let Some(vad_threshold) = extract_f32_value(raw, "local_stt_vad_threshold") {
            cfg.vad_threshold = vad_threshold;
        }
        cfg.language = extract_string_value(raw, "local_stt_language")
            .unwrap_or_else(|| engine_config.source_language.clone());
        cfg.partial_interval_ms =
            extract_u64_value(raw, "local_stt_partial_interval_ms").unwrap_or(500);
        cfg.max_partial_window_seconds =
            extract_f32_value(raw, "local_stt_max_partial_window_seconds").unwrap_or(5.0);
        cfg.overlap_tail_ms = extract_u64_value(raw, "local_stt_overlap_tail_ms").unwrap_or(300);
        cfg.skip_partial_if_busy =
            extract_bool_value(raw, "local_stt_skip_partial_if_busy").unwrap_or(true);
        Some(cfg)
    }
}

impl MtConfig {
    pub fn from_json_lossy(raw: &str, engine_config: &EngineConfig) -> Option<Self> {
        let enabled = extract_bool_value(raw, "mt_enabled").unwrap_or(false);
        let mentions_mt = raw.contains("\"mt_")
            || raw.contains("mt_endpoint")
            || raw.contains("mt_model")
            || raw.contains("mt_api_key");
        if !enabled && !mentions_mt {
            return None;
        }

        let mut cfg = Self::default();
        cfg.enabled = enabled;
        if let Some(endpoint) = extract_string_value(raw, "mt_endpoint") {
            cfg.endpoint = endpoint;
        }
        if let Some(api_key) = extract_string_value(raw, "mt_api_key") {
            cfg.api_key = api_key;
        }
        if let Some(api_key_env) = extract_string_value(raw, "mt_api_key_env") {
            cfg.api_key_env = api_key_env;
        }
        if let Some(model) = extract_string_value(raw, "mt_model") {
            cfg.model = model;
        }
        cfg.target_language = extract_string_value(raw, "mt_target_language")
            .unwrap_or_else(|| engine_config.target_language.clone());
        Some(cfg)
    }
}

impl TtsConfig {
    pub fn from_json_lossy(raw: &str, _engine_config: &EngineConfig) -> Option<Self> {
        let enabled = extract_bool_value(raw, "tts_enabled").unwrap_or(false);
        let mentions_tts =
            raw.contains("\"tts_") || raw.contains("tts_model_id") || raw.contains("tts_model_dir");
        if !enabled && !mentions_tts {
            return None;
        }

        let mut cfg = Self::default();
        cfg.enabled = enabled;
        if let Some(model_id) = extract_string_value(raw, "tts_model_id") {
            cfg.model_id = model_id;
        }
        if let Some(model_dir) = extract_string_value(raw, "tts_model_dir") {
            cfg.model_dir = std::path::PathBuf::from(expand_tilde(&model_dir));
        }
        if let Some(sid) = extract_f32_value(raw, "tts_speaker_id") {
            cfg.speaker_id = sid as i32;
        }
        if let Some(speed) = extract_f32_value(raw, "tts_speed") {
            cfg.speed = speed;
        }
        Some(cfg)
    }
}

#[derive(Clone, Debug)]
pub struct LocalMtConfig {
    pub enabled: bool,
    /// Model identifier, e.g. "opus-mt-zh-en", "nllb-200-distilled-600M"
    pub model_id: String,
    /// Directory containing model files (encoder_model.onnx, tokenizer.json, …)
    pub model_dir: std::path::PathBuf,
    /// Source language ISO 639-1 code ("zh", "ja", "auto")
    pub source_lang: String,
    /// Target language ISO 639-1 code ("en", "zh")
    pub target_lang: String,
}

impl Default for LocalMtConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model_id: "opus-mt-zh-en".to_string(),
            model_dir: std::path::PathBuf::from(""),
            source_lang: "auto".to_string(),
            target_lang: "en".to_string(),
        }
    }
}

impl LocalMtConfig {
    pub fn from_json_lossy(raw: &str, engine_config: &EngineConfig) -> Option<Self> {
        let enabled = extract_bool_value(raw, "local_mt_enabled").unwrap_or(false);
        let mentions = raw.contains("local_mt_");
        if !enabled && !mentions {
            return None;
        }
        let mut cfg = Self::default();
        cfg.enabled = enabled;
        if let Some(model_id) = extract_string_value(raw, "local_mt_model_id") {
            cfg.model_id = model_id;
        }
        if let Some(model_dir) = extract_string_value(raw, "local_mt_model_dir") {
            cfg.model_dir = std::path::PathBuf::from(expand_tilde(&model_dir));
        }
        cfg.source_lang = extract_string_value(raw, "local_mt_source_lang")
            .unwrap_or_else(|| engine_config.source_language.clone());
        cfg.target_lang = extract_string_value(raw, "local_mt_target_lang")
            .unwrap_or_else(|| engine_config.target_language.clone());
        Some(cfg)
    }
}

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::Path::new(&home)
                .join(rest)
                .to_string_lossy()
                .into_owned();
        }
    }
    path.to_string()
}

fn extract_f32_value(raw: &str, key: &str) -> Option<f32> {
    let patterns = [format!("\"{key}\":"), format!("{key} = ")];
    for pattern in patterns {
        let start = raw.find(&pattern)? + pattern.len();
        let value = raw[start..]
            .chars()
            .skip_while(|ch| ch.is_whitespace())
            .take_while(|ch| ch.is_ascii_digit() || matches!(ch, '.' | '-' | '+'))
            .collect::<String>();
        if let Ok(parsed) = value.parse::<f32>() {
            return Some(parsed);
        }
    }
    None
}

fn extract_u64_value(raw: &str, key: &str) -> Option<u64> {
    let patterns = [format!("\"{key}\":"), format!("{key} = ")];
    for pattern in patterns {
        let start = raw.find(&pattern)? + pattern.len();
        let value = raw[start..]
            .chars()
            .skip_while(|ch| ch.is_whitespace())
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if let Ok(parsed) = value.parse::<u64>() {
            return Some(parsed);
        }
    }
    None
}

fn extract_i32_value(raw: &str, key: &str) -> Option<i32> {
    let patterns = [format!("\"{key}\":"), format!("{key} = ")];
    for pattern in patterns {
        let start = raw.find(&pattern)? + pattern.len();
        let value = raw[start..]
            .chars()
            .skip_while(|ch| ch.is_whitespace())
            .take_while(|ch| ch.is_ascii_digit() || matches!(ch, '-' | '+'))
            .collect::<String>();
        if let Ok(parsed) = value.parse::<i32>() {
            return Some(parsed);
        }
    }
    None
}

fn extract_u32_value(raw: &str, key: &str) -> Option<u32> {
    let patterns = [format!("\"{key}\":"), format!("{key} = ")];
    for pattern in patterns {
        let start = raw.find(&pattern)? + pattern.len();
        let value = raw[start..]
            .chars()
            .skip_while(|ch| ch.is_whitespace())
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if let Ok(parsed) = value.parse::<u32>() {
            return Some(parsed);
        }
    }
    None
}

fn extract_string_value(raw: &str, key: &str) -> Option<String> {
    let json_pattern = format!("\"{key}\":");
    if let Some(start) = raw.find(&json_pattern) {
        let slice = &raw[start + json_pattern.len()..];
        let quote_start = slice.find('"')?;
        let remainder = &slice[quote_start + 1..];
        let quote_end = remainder.find('"')?;
        return Some(remainder[..quote_end].to_string());
    }

    let toml_pattern = format!("{key} = ");
    if let Some(start) = raw.find(&toml_pattern) {
        let slice = &raw[start + toml_pattern.len()..];
        let quote_start = slice.find('"')?;
        let remainder = &slice[quote_start + 1..];
        let quote_end = remainder.find('"')?;
        return Some(remainder[..quote_end].to_string());
    }

    None
}

fn extract_bool_value(raw: &str, key: &str) -> Option<bool> {
    let patterns = [format!("\"{key}\":"), format!("{key} = ")];
    for pattern in patterns {
        let start = raw.find(&pattern)? + pattern.len();
        let value = raw[start..]
            .chars()
            .skip_while(|ch| ch.is_whitespace())
            .take_while(|ch| ch.is_ascii_alphabetic())
            .collect::<String>();
        match value.as_str() {
            "true" => return Some(true),
            "false" => return Some(false),
            _ => {}
        }
    }
    None
}

fn source_locale_from_config(raw: &str, config: &EngineConfig) -> String {
    if let Some(source_locale) = extract_string_value(raw, "source_locale") {
        return source_locale;
    }
    language_to_locale(&config.source_language).to_string()
}

fn target_locale_from_config(raw: &str, config: &EngineConfig) -> String {
    if let Some(target_locale) = extract_string_value(raw, "target_locale") {
        return target_locale;
    }
    language_to_locale(&config.target_language).to_string()
}

fn azure_source_locale_from_config(raw: &str, config: &EngineConfig) -> String {
    if let Some(source_locale) = extract_string_value(raw, "azure_voice_live_source_locale") {
        return source_locale;
    }
    source_locale_from_config(raw, config)
}

fn azure_target_locale_from_config(raw: &str, config: &EngineConfig) -> String {
    if let Some(target_locale) = extract_string_value(raw, "azure_voice_live_target_locale") {
        return target_locale;
    }
    target_locale_from_config(raw, config)
}

fn language_to_locale(language: &str) -> &'static str {
    match language {
        "zh" => "zh-CN",
        "ja" => "ja-JP",
        "en" => "en-US",
        "auto" => "auto",
        _ => "en-US",
    }
}

fn locale_default_voice(locale: &str) -> &'static str {
    match locale {
        "zh-CN" => "zh-CN-XiaoxiaoNeural",
        "ja-JP" => "ja-JP-NanamiNeural",
        _ => "en-US-Ava:DragonHDLatestNeural",
    }
}

#[derive(Clone, Debug)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Clone, Debug)]
pub struct AudioFrame {
    pub timestamp_ns: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub data: Vec<f32>,
}

impl AudioFrame {
    pub fn frames(&self) -> usize {
        if self.channels == 0 {
            return 0;
        }
        self.data.len() / usize::from(self.channels)
    }
}

#[derive(Debug, Clone)]
pub struct EngineError {
    message: String,
}

impl EngineError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for EngineError {}

pub type Result<T> = std::result::Result<T, EngineError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eleven_labs_provider_parsed_from_json() {
        let config = EngineConfig::from_json_lossy(r#"{"translation_provider":"eleven_labs"}"#);
        assert_eq!(config.translation_provider, TranslationProvider::ElevenLabs);
    }

    #[test]
    fn local_stt_parsed_from_json() {
        let config = EngineConfig::from_json_lossy(
            r#"{"local_stt_enabled":true,"local_stt_model_id":"paraformer-zh","local_stt_vad_threshold":0.4}"#,
        );
        let stt = config.local_stt.expect("local_stt should parse");
        assert!(stt.enabled);
        assert_eq!(stt.model_id, "paraformer-zh");
        assert!((stt.vad_threshold - 0.4).abs() < 1e-6);
    }

    #[test]
    fn cosyvoice_tts_parsed_from_json() {
        let config = EngineConfig::from_json_lossy(
            r#"{"cosyvoice_tts_enabled":true,"cosyvoice_tts_endpoint":"http://127.0.0.1:50000","cosyvoice_tts_prompt_text":"hello"}"#,
        );
        let cv = config.cosyvoice_tts.expect("cosyvoice_tts should parse");
        assert!(cv.enabled);
        assert_eq!(cv.endpoint, "http://127.0.0.1:50000");
        assert_eq!(cv.prompt_text, "hello");
    }

    #[test]
    fn cosyvoice_tts_absent_when_not_mentioned() {
        let config = EngineConfig::from_json_lossy(r#"{}"#);
        assert!(config.cosyvoice_tts.is_none());
    }

    #[test]
    fn mt_parsed_from_json() {
        let config = EngineConfig::from_json_lossy(
            r#"{"mt_enabled":true,"mt_model":"qwen2.5-7b","mt_endpoint":"http://localhost:8080/v1/chat/completions"}"#,
        );
        let mt = config.mt.expect("mt should parse");
        assert!(mt.enabled);
        assert_eq!(mt.model, "qwen2.5-7b");
        assert_eq!(mt.endpoint, "http://localhost:8080/v1/chat/completions");
    }
}
