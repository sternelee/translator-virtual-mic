use std::path::{Path, PathBuf};

pub fn paraformer_model_paths(model_dir: &Path) -> (PathBuf, PathBuf) {
    (
        model_dir.join("model.int8.onnx"),
        model_dir.join("tokens.txt"),
    )
}

pub fn moonshine_model_paths(
    model_dir: &Path,
) -> (PathBuf, PathBuf, PathBuf, PathBuf, PathBuf) {
    (
        model_dir.join("preprocess.onnx"),
        model_dir.join("encode.int8.onnx"),
        model_dir.join("uncached_decode.int8.onnx"),
        model_dir.join("cached_decode.int8.onnx"),
        model_dir.join("tokens.txt"),
    )
}

pub fn fire_red_asr_model_paths(model_dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    (
        model_dir.join("encoder.int8.onnx"),
        model_dir.join("decoder.int8.onnx"),
        model_dir.join("tokens.txt"),
    )
}

pub fn zipformer_ctc_model_paths(model_dir: &Path) -> (PathBuf, PathBuf) {
    (
        model_dir.join("model.int8.onnx"),
        model_dir.join("tokens.txt"),
    )
}

/// Return a path under `models_root/<model_id>/` for arbitrary file names. Useful
/// for callers that already know the file layout of a backend.
pub fn model_file_path(models_root: &Path, model_id: &str, relative: &str) -> PathBuf {
    models_root.join(model_id).join(relative)
}

/// Returns true when every file declared in the registry for the given
/// model id exists on disk under `models_root/<model_id>/`.
pub fn is_model_downloaded(models_root: &Path, model_id: &str) -> bool {
    let info = match crate::registry::get_model(model_id) {
        Some(info) => info,
        None => return false,
    };
    info.files.iter().all(|f| {
        let path = models_root.join(model_id).join(f.relative_path);
        path.exists() && path.metadata().map(|m| m.len() > 0).unwrap_or(false)
    })
}
