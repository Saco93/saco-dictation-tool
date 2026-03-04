use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use chrono::Utc;
use common::config::DebugWavConfig;

#[derive(Debug, Clone)]
pub struct DebugWavRecorder {
    cfg: DebugWavConfig,
}

impl DebugWavRecorder {
    #[must_use]
    pub fn new(cfg: DebugWavConfig) -> Self {
        Self { cfg }
    }

    pub fn is_enabled(&self) -> bool {
        self.cfg.enabled
    }

    pub async fn maybe_write(
        &self,
        output_dir: &Path,
        samples: &[i16],
        sample_rate_hz: u32,
    ) -> Result<Option<PathBuf>, String> {
        if !self.cfg.enabled {
            return Ok(None);
        }

        let output_dir = output_dir.to_path_buf();
        let cleanup_dir = output_dir.clone();
        let samples = samples.to_vec();

        let path = tokio::task::spawn_blocking(move || {
            fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
            let filename = format!("utterance-{}.wav", Utc::now().format("%Y%m%dT%H%M%S%.3f"));
            let filepath = output_dir.join(filename);

            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: sample_rate_hz,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };

            let mut writer = hound::WavWriter::create(&filepath, spec)
                .map_err(|e| format!("failed to create wav file: {e}"))?;

            for sample in samples {
                writer
                    .write_sample(sample)
                    .map_err(|e| format!("failed writing wav sample: {e}"))?;
            }

            writer
                .finalize()
                .map_err(|e| format!("failed finalizing wav file: {e}"))?;

            Ok::<PathBuf, String>(filepath)
        })
        .await
        .map_err(|e| e.to_string())??;

        self.cleanup(cleanup_dir.as_path())?;

        Ok(Some(path))
    }

    fn cleanup(&self, output_dir: &Path) -> Result<(), String> {
        let ttl = Duration::from_secs(self.cfg.ttl_hours.saturating_mul(3600));
        let now = SystemTime::now();

        let mut entries = Vec::new();
        for entry in fs::read_dir(output_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let metadata = entry.metadata().map_err(|e| e.to_string())?;
            if !metadata.is_file() {
                continue;
            }

            if now
                .duration_since(metadata.modified().unwrap_or(now))
                .unwrap_or_default()
                > ttl
            {
                let _ = fs::remove_file(entry.path());
                continue;
            }

            entries.push((
                entry.path(),
                metadata.len(),
                metadata.modified().unwrap_or(now),
            ));
        }

        let max_bytes = self.cfg.size_cap_mb.saturating_mul(1024 * 1024);
        let mut total_bytes: u64 = entries.iter().map(|(_, size, _)| *size).sum();

        if total_bytes <= max_bytes {
            return Ok(());
        }

        entries.sort_by_key(|(_, _, modified)| *modified);
        for (path, size, _) in entries {
            if total_bytes <= max_bytes {
                break;
            }

            let _ = fs::remove_file(path);
            total_bytes = total_bytes.saturating_sub(size);
        }

        Ok(())
    }
}
