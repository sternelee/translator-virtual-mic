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

#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub source_language: String,
    pub target_language: String,
    pub input_sample_rate: u32,
    pub output_sample_rate: u32,
    pub channels: u16,
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
            mode: EngineMode::Bypass,
            raw_config_json: "{}".to_string(),
        }
    }
}

impl EngineConfig {
    pub fn from_json_lossy(raw: &str) -> Self {
        let mut config = Self::default();
        config.raw_config_json = raw.to_string();

        if raw.contains("\"fallback_mode\":\"mute\"") || raw.contains("fallback_mode = \"mute\"") {
            config.mode = EngineMode::MuteOnFailure;
        }
        if raw.contains("\"target\":\"zh\"") || raw.contains("target = \"zh\"") {
            config.target_language = "zh".to_string();
        }

        config
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
