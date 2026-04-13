use std::fmt;

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
                _ => TranslationProvider::None,
            };
        }
        if let Some(azure_voice_live) = AzureVoiceLiveConfig::from_json_lossy(raw, &config) {
            config.azure_voice_live = Some(azure_voice_live);
        }
        if let Some(openai_realtime) = OpenAIRealtimeConfig::from_json_lossy(raw, &config) {
            config.openai_realtime = Some(openai_realtime);
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
