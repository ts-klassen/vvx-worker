use crate::{
    tts::{EngineError, EngineResult, TtsEngine},
    TaskMessage,
};
use async_trait::async_trait;
use camino::{Utf8Path, Utf8PathBuf};
use std::{
    collections::HashMap,
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::task;
use voicevox_core::{
    blocking::{Onnxruntime, OpenJtalk, Synthesizer, VoiceModelFile},
    StyleId,
};
#[derive(Debug)]
pub struct VoicevoxConfig {
    pub onnxruntime_path: Option<PathBuf>,
    pub open_jtalk_dict_dir: Utf8PathBuf,
    pub model_dir: Utf8PathBuf,
}

pub struct VoicevoxTtsEngine {
    synthesizer: Arc<Mutex<Synthesizer<OpenJtalk>>>,
    model_paths: Arc<HashMap<u32, PathBuf>>,
}

impl VoicevoxTtsEngine {
    pub fn new(config: VoicevoxConfig) -> EngineResult<Self> {
        let VoicevoxConfig {
            onnxruntime_path,
            open_jtalk_dict_dir,
            model_dir,
        } = config;

        let ort_builder = Onnxruntime::load_once();
        let ort = match onnxruntime_path {
            Some(path) => ort_builder.filename(path).perform()?,
            None => ort_builder.perform()?,
        };

        let text_analyzer = OpenJtalk::new(open_jtalk_dict_dir.as_path())?;
        let synthesizer = Synthesizer::builder(ort)
            .text_analyzer(text_analyzer)
            .build()?;

        let model_paths = prepare_models(model_dir.as_path())?;

        if model_paths.is_empty() {
            return Err(EngineError::InvalidTask(format!(
                "no voice models discovered in {}",
                model_dir
            )));
        }

        Ok(Self {
            synthesizer: Arc::new(Mutex::new(synthesizer)),
            model_paths: Arc::new(model_paths),
        })
    }
}

#[async_trait]
impl TtsEngine for VoicevoxTtsEngine {
    async fn process_task(
        &self,
        _engine_id: u32,
        message: &TaskMessage,
    ) -> EngineResult<Option<String>> {
        let text = message
            .text
            .as_ref()
            .ok_or_else(|| EngineError::InvalidTask("missing text for synthesis".into()))?
            .to_owned();

        let output_dir = message
            .output_dir
            .as_ref()
            .ok_or_else(|| EngineError::InvalidTask("missing output directory".into()))?
            .to_owned();

        let filename = message
            .result_filename
            .clone()
            .unwrap_or_else(|| format!("{}.wav", message.eval_id));

        let synthesizer = Arc::clone(&self.synthesizer);
        let model_paths = Arc::clone(&self.model_paths);
        let style_id = message.speaker_id;

        let output_path = PathBuf::from(output_dir).join(filename);
        let output_path_clone = output_path.clone();

        let result_path = task::spawn_blocking(move || {
            let guard = synthesizer
                .lock()
                .map_err(|_| EngineError::Voicevox("synthesizer lock poisoned".into()))?;

            if !guard.is_loaded_model_by_style_id(StyleId(style_id)) {
                let path = model_paths.get(&style_id).ok_or_else(|| {
                    EngineError::InvalidTask(format!("unknown speaker/style id {}", style_id))
                })?;
                let voice_model = VoiceModelFile::open(path)?;
                guard.load_voice_model(&voice_model)?;
            }

            let bytes = guard.tts(&text, StyleId(style_id)).perform()?;
            drop(guard);

            if let Some(parent) = output_path_clone.parent() {
                fs::create_dir_all(parent)?;
            }

            fs::write(&output_path_clone, &bytes)?;

            Ok::<_, EngineError>(output_path_clone)
        })
        .await??;

        Ok(Some(result_path.to_string_lossy().into_owned()))
    }
}

fn prepare_models(root: &Utf8Path) -> EngineResult<HashMap<u32, PathBuf>> {
    let mut mapping = HashMap::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(dir.as_std_path()).map_err(|err| {
            EngineError::Io(io::Error::new(
                err.kind(),
                format!("failed to read model directory {}: {}", dir, err),
            ))
        })?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                if has_vvm_extension(path.as_path()) {
                    collect_styles(path.as_path(), &mut mapping)?;
                    continue;
                }

                let utf8 = Utf8PathBuf::from_path_buf(path.clone()).map_err(|_| {
                    EngineError::InvalidTask(format!(
                        "model directory path contains invalid UTF-8: {}",
                        path.display()
                    ))
                })?;

                stack.push(utf8);
            } else if path.is_file() && has_vvm_extension(path.as_path()) {
                collect_styles(path.as_path(), &mut mapping)?;
            }
        }
    }

    Ok(mapping)
}

fn collect_styles(path: &Path, mapping: &mut HashMap<u32, PathBuf>) -> EngineResult<()> {
    let voice_model = VoiceModelFile::open(path)?;
    for character in voice_model.metas() {
        for style in &character.styles {
            mapping
                .entry(style.id.0)
                .or_insert_with(|| path.to_path_buf());
        }
    }
    Ok(())
}

fn has_vvm_extension(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|ext| ext.eq_ignore_ascii_case("vvm"))
        .unwrap_or(false)
}
