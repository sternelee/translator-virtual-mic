//! MadLad-400-3B-MT translation via Python subprocess (mlx-lm or transformers).
//!
//! Spawns `python3 scripts/madlad_translate.py` at backend creation time and
//! communicates over stdin/stdout JSON line protocol.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;

use crate::{MtLocalError, Result};

pub struct MadladBackend {
    model_id: String,
    #[allow(dead_code)]
    model_dir: String,
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    reader: Mutex<BufReader<std::process::ChildStdout>>,
}

impl MadladBackend {
    pub fn new(model_id: &str, model_dir: &Path) -> Result<Self> {
        let script_path = Self::find_script()?;
        let mut cmd = Command::new("python3");
        cmd.arg(&script_path)
            .arg("--model-id")
            .arg(model_id)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        // If model_dir points to a local checkout, set HF_HUB_OFFLINE and
        // TRANSFORMERS_OFFLINE so the script does not try to download.
        if model_dir.exists() {
            cmd.env("HF_HUB_OFFLINE", "1")
                .env("TRANSFORMERS_OFFLINE", "1");
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| MtLocalError::Io(e))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| MtLocalError::Model("failed to capture child stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| MtLocalError::Model("failed to capture child stdout".into()))?;
        let reader = BufReader::new(stdout);

        eprintln!("[mt-local/madlad] Spawned python3 {} --model-id {model_id}", script_path.display());

        Ok(Self {
            model_id: model_id.to_string(),
            model_dir: model_dir.to_string_lossy().to_string(),
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            reader: Mutex::new(reader),
        })
    }

    fn find_script() -> Result<std::path::PathBuf> {
        let candidates = [
            std::path::PathBuf::from("scripts/madlad_translate.py"),
            std::path::PathBuf::from("../scripts/madlad_translate.py"),
            std::path::PathBuf::from("../../scripts/madlad_translate.py"),
        ];
        for c in &candidates {
            if c.exists() {
                return Ok(c.canonicalize().unwrap_or_else(|_| c.clone()));
            }
        }
        Err(MtLocalError::Model(
            "madlad_translate.py not found. Looked in: scripts/, ../scripts/, ../../scripts/".into(),
        ))
    }

    fn send_request(&self, text: &str, source_lang: &str, target_lang: &str) -> Result<String> {
        let req = serde_json::json!({
            "text": text,
            "source_lang": source_lang,
            "target_lang": target_lang,
        });
        let line = req.to_string() + "\n";

        {
            let mut stdin = self.stdin.lock().unwrap();
            stdin
                .write_all(line.as_bytes())
                .map_err(|e| MtLocalError::Io(e))?;
            stdin.flush().map_err(|e| MtLocalError::Io(e))?;
        }

        let mut response_line = String::new();
        {
            let mut reader = self.reader.lock().unwrap();
            reader
                .read_line(&mut response_line)
                .map_err(|e| MtLocalError::Io(e))?;
        }

        let resp: serde_json::Value =
            serde_json::from_str(&response_line).map_err(|e| {
                MtLocalError::Inference(format!("invalid JSON from madlad script: {e}"))
            })?;

        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(MtLocalError::Inference(format!("madlad error: {err}")));
        }

        let translation = resp
            .get("translation")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(translation)
    }
}

impl Drop for MadladBackend {
    fn drop(&mut self) {
        let _ = self.child.lock().unwrap().kill();
    }
}

impl crate::LocalMtBackend for MadladBackend {
    fn translate(&self, text: &str, target_lang: &str) -> Result<String> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(String::new());
        }
        // MadLad uses 2-letter lang codes in its <2xx> prefix.
        let madlad_target = match target_lang {
            "zh" | "zh-CN" | "zho_Hans" => "zh",
            "zh-TW" | "zho_Hant" => "zh",
            "en" | "eng_Latn" => "en",
            "ja" | "jpn_Jpan" => "ja",
            "ko" | "kor_Hang" => "ko",
            "fr" | "fra_Latn" => "fr",
            "de" | "deu_Latn" => "de",
            "es" | "spa_Latn" => "es",
            other => other,
        };
        let result = self.send_request(text, "", madlad_target)?;
        Ok(result.trim().to_string())
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}
