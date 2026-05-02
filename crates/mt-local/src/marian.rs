//! MarianMT / NLLB-200 encoder-decoder translation via ONNX Runtime.

use std::path::Path;
use std::sync::Mutex;

use ndarray::{Array2, ArrayD, IxDyn};
use ort::{inputs, session::Session, value::Tensor};
use tokenizers::Tokenizer;

use crate::registry::{get_model, iso_to_nllb, MtModelFamily};
use crate::{MtLocalError, Result};

const DEFAULT_MAX_LENGTH: usize = 256;

pub struct MarianBackend {
    model_id: String,
    family: MtModelFamily,
    src_lang: String,
    encoder: Mutex<Session>,
    decoder: Mutex<Session>,
    #[allow(dead_code)]
    has_merged_decoder: bool,
    tokenizer: Tokenizer,
}

impl MarianBackend {
    pub fn new(model_id: &str, dir: &Path, src_lang: &str) -> Result<Self> {
        eprintln!("[mt-local] loading model_id={model_id} dir={dir:?} src_lang={src_lang}");

        let family = get_model(model_id)
            .map(|m| m.family)
            .unwrap_or(MtModelFamily::OpusMt);

        // Encoder
        let encoder_path = dir.join("encoder_model.onnx");
        if !encoder_path.exists() {
            return Err(MtLocalError::Model(format!(
                "encoder_model.onnx not found in {}",
                dir.display()
            )));
        }
        let encoder = Session::builder()
            .map_err(|e| MtLocalError::Model(format!("ort session builder: {e}")))?
            .with_intra_threads(2)
            .map_err(|e| MtLocalError::Model(format!("ort threads: {e}")))?
            .commit_from_file(&encoder_path)
            .map_err(|e| MtLocalError::Model(format!("load encoder: {e}")))?;
        eprintln!("[mt-local] encoder loaded");

        // Decoder: prefer merged (with past KV cache)
        let merged_path = dir.join("decoder_model_merged.onnx");
        let decoder_path = dir.join("decoder_model.onnx");
        let (decoder, has_merged_decoder) = if merged_path.exists() {
            let s = Session::builder()
                .map_err(|e| MtLocalError::Model(format!("ort session builder: {e}")))?
                .with_intra_threads(2)
                .map_err(|e| MtLocalError::Model(format!("ort threads: {e}")))?
                .commit_from_file(&merged_path)
                .map_err(|e| MtLocalError::Model(format!("load merged decoder: {e}")))?;
            eprintln!("[mt-local] merged decoder loaded");
            (s, true)
        } else if decoder_path.exists() {
            let s = Session::builder()
                .map_err(|e| MtLocalError::Model(format!("ort session builder: {e}")))?
                .with_intra_threads(2)
                .map_err(|e| MtLocalError::Model(format!("ort threads: {e}")))?
                .commit_from_file(&decoder_path)
                .map_err(|e| MtLocalError::Model(format!("load decoder: {e}")))?;
            eprintln!("[mt-local] decoder (no-past) loaded");
            (s, false)
        } else {
            return Err(MtLocalError::Model(format!(
                "neither decoder_model_merged.onnx nor decoder_model.onnx found in {}",
                dir.display()
            )));
        };

        // Tokenizer
        let tok_path = dir.join("tokenizer.json");
        if !tok_path.exists() {
            return Err(MtLocalError::Tokenizer(format!(
                "tokenizer.json not found in {}",
                dir.display()
            )));
        }
        let tokenizer = Tokenizer::from_file(&tok_path)
            .map_err(|e| MtLocalError::Tokenizer(format!("load tokenizer: {e}")))?;
        eprintln!("[mt-local] tokenizer loaded");

        Ok(Self {
            model_id: model_id.to_string(),
            family,
            src_lang: src_lang.to_string(),
            encoder: Mutex::new(encoder),
            decoder: Mutex::new(decoder),
            has_merged_decoder,
            tokenizer,
        })
    }

    /// Tokenise `text`, prepending the NLLB source-language token ID when needed.
    ///
    /// For NLLB-200, the source-language tag must be prepended as a special
    /// *token ID* (not as raw text) to guarantee correct tokenisation.  We look
    /// up the ID in the vocabulary and insert it before encoding the text.
    fn encode_text(&self, text: &str) -> Result<(Vec<i64>, usize)> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| MtLocalError::Tokenizer(format!("encode: {e}")))?;
        let mut ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();

        if matches!(self.family, MtModelFamily::Nllb200) {
            let nllb_src = iso_to_nllb(&self.src_lang);
            if let Some(&tok_id) = self.tokenizer.get_vocab(true).get(nllb_src.as_str()) {
                ids.insert(0, tok_id as i64);
            }
        }

        // Append EOS token for encoder (MarianMT convention).
        let eos_id = self.eos_token_id();
        if ids.last() != Some(&eos_id) {
            ids.push(eos_id);
        }

        let len = ids.len();
        Ok((ids, len))
    }

    fn run_encoder(&self, input_ids: &[i64]) -> Result<ArrayD<f32>> {
        let seq_len = input_ids.len();
        let ids_arr = Array2::from_shape_vec((1, seq_len), input_ids.to_vec())
            .map_err(|e| MtLocalError::Inference(format!("shape: {e}")))?;
        let mask_arr = Array2::<i64>::ones((1, seq_len));

        let ids_tensor = Tensor::from_array(ids_arr)
            .map_err(|e| MtLocalError::Inference(format!("ids tensor: {e}")))?;
        let mask_tensor = Tensor::from_array(mask_arr)
            .map_err(|e| MtLocalError::Inference(format!("mask tensor: {e}")))?;

        let mut enc_session = self.encoder.lock().unwrap();
        let outputs = enc_session
            .run(inputs![
                "input_ids" => ids_tensor,
                "attention_mask" => mask_tensor
            ])
            .map_err(|e| MtLocalError::Inference(format!("encoder run: {e}")))?;

        let (shape, data) = outputs["last_hidden_state"]
            .try_extract_tensor::<f32>()
            .map_err(|e| MtLocalError::Inference(format!("extract hidden: {e}")))?;
        let dims: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
        let hidden = ArrayD::from_shape_vec(IxDyn(&dims), data.to_vec())
            .map_err(|e| MtLocalError::Inference(format!("reshape hidden: {e}")))?;
        Ok(hidden)
    }

    fn greedy_decode(
        &self,
        encoder_hidden: &ArrayD<f32>,
        _input_ids_len: usize,
        tgt_lang_token_id: Option<i64>,
    ) -> Result<Vec<i64>> {
        let bos_id: i64 = tgt_lang_token_id.unwrap_or_else(|| self.bos_token_id());
        let eos_id: i64 = self.eos_token_id();

        let mut generated: Vec<i64> = vec![bos_id];
        let enc_seq_len = encoder_hidden.shape()[1];
        let enc_shape = encoder_hidden.shape().to_vec();

        for _step in 0..DEFAULT_MAX_LENGTH {
            let dec_len = generated.len();
            let dec_ids = Array2::from_shape_vec((1, dec_len), generated.clone())
                .map_err(|e| MtLocalError::Inference(format!("dec shape: {e}")))?;
            let enc_mask = Array2::<i64>::ones((1, enc_seq_len));

            let dec_ids_t = Tensor::from_array(dec_ids)
                .map_err(|e| MtLocalError::Inference(format!("dec_ids tensor: {e}")))?;
            let enc_mask_t = Tensor::from_array(enc_mask)
                .map_err(|e| MtLocalError::Inference(format!("enc_mask tensor: {e}")))?;
            let enc_h_arr = ArrayD::from_shape_vec(
                IxDyn(&enc_shape),
                encoder_hidden.iter().cloned().collect(),
            )
            .map_err(|e| MtLocalError::Inference(format!("enc_h reshape: {e}")))?;
            let enc_h_t = Tensor::from_array(enc_h_arr)
                .map_err(|e| MtLocalError::Inference(format!("enc_h tensor: {e}")))?;

            let mut dec_session = self.decoder.lock().unwrap();
            let outputs = dec_session
                .run(inputs![
                    "input_ids" => dec_ids_t,
                    "encoder_attention_mask" => enc_mask_t,
                    "encoder_hidden_states" => enc_h_t
                ])
                .map_err(|e| MtLocalError::Inference(format!("decoder run: {e}")))?;

            let (logits_shape, logits_data) = outputs["logits"]
                .try_extract_tensor::<f32>()
                .map_err(|e| MtLocalError::Inference(format!("extract logits: {e}")))?;

            let vocab_size = logits_shape[2] as usize;
            // Decoder without KV cache outputs logits for the last step only
            // (shape [1, 1, vocab_size]), while merged decoder outputs all steps
            // (shape [1, dec_len, vocab_size]).  Use the last vocab_size entries.
            let last_step_start = logits_data.len().saturating_sub(vocab_size);
            let last_step = &logits_data[last_step_start..];

            let next_id = last_step
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i as i64)
                .unwrap_or(eos_id);

            if next_id == eos_id {
                break;
            }
            generated.push(next_id);
        }

        // Strip BOS
        if generated.first() == Some(&bos_id) {
            generated.remove(0);
        }
        Ok(generated)
    }

    fn bos_token_id(&self) -> i64 {
        if let Some(tok) = self.tokenizer.get_vocab(true).get("<pad>") {
            return *tok as i64;
        }
        if let Some(tok) = self.tokenizer.get_vocab(true).get("[BOS]") {
            return *tok as i64;
        }
        0
    }

    fn eos_token_id(&self) -> i64 {
        if let Some(tok) = self.tokenizer.get_vocab(true).get("</s>") {
            return *tok as i64;
        }
        if let Some(tok) = self.tokenizer.get_vocab(true).get("[EOS]") {
            return *tok as i64;
        }
        2
    }

    fn tgt_lang_token_id(&self, tgt_lang: &str) -> Option<i64> {
        match self.family {
            MtModelFamily::Nllb200 => {
                let nllb_tag = iso_to_nllb(tgt_lang);
                self.tokenizer
                    .get_vocab(true)
                    .get(nllb_tag.as_str())
                    .map(|&id| id as i64)
            }
            MtModelFamily::OpusMt => None,
        }
    }

    fn decode_ids(&self, ids: &[i64]) -> Result<String> {
        let u32_ids: Vec<u32> = ids.iter().map(|&id| id as u32).collect();
        self.tokenizer
            .decode(&u32_ids, true)
            .map_err(|e| MtLocalError::Tokenizer(format!("decode: {e}")))
    }
}

impl crate::LocalMtBackend for MarianBackend {
    fn translate(&self, text: &str, target_lang: &str) -> Result<String> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(String::new());
        }
        eprintln!(
            "[mt-local] translate: len={} src={} tgt={target_lang}",
            text.len(),
            self.src_lang
        );

        let (input_ids, _seq_len) = self.encode_text(text)?;
        let encoder_hidden = self.run_encoder(&input_ids)?;
        let tgt_token_id = self.tgt_lang_token_id(target_lang);
        let output_ids = self.greedy_decode(&encoder_hidden, input_ids.len(), tgt_token_id)?;
        let result = self.decode_ids(&output_ids)?;

        eprintln!(
            "[mt-local] translated: '{}'",
            &result[..result.len().min(80)]
        );
        Ok(result.trim().to_string())
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}

#[cfg(test)]
mod tests {
    use crate::registry::iso_to_nllb;

    #[test]
    fn iso_to_nllb_basic() {
        assert_eq!(iso_to_nllb("zh"), "zho_Hans");
        assert_eq!(iso_to_nllb("en"), "eng_Latn");
        assert_eq!(iso_to_nllb("ja"), "jpn_Jpan");
    }

    #[test]
    #[ignore = "requires model files"]
    fn opus_mt_zh_en_smoke() {
        use crate::LocalMtBackend;
        use std::path::PathBuf;
        let model_dir = PathBuf::from(std::env::var("HOME").unwrap())
            .join("Library/Application Support/translator-virtual-mic/models/opus-mt-zh-en");
        if !model_dir.exists() {
            eprintln!("Skipping: model dir not found at {:?}", model_dir);
            return;
        }
        let backend = super::MarianBackend::new("opus-mt-zh-en", &model_dir, "zh").unwrap();
        let result = backend.translate("你好世界", "en").unwrap();
        eprintln!("Translation result: '{}'", result);
        assert!(!result.is_empty());
    }
}
