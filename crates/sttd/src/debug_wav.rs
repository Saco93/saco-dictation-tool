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

#[cfg(test)]
mod tests {
    use std::{fs::OpenOptions, time::Duration};

    use tempfile::tempdir;

    use super::DebugWavRecorder;
    use common::config::DebugWavConfig;

    #[tokio::test]
    async fn disabled_debug_wav_never_writes_files() {
        let temp = tempdir().expect("tempdir");
        let recorder = DebugWavRecorder::new(DebugWavConfig {
            enabled: false,
            directory: temp.path().display().to_string(),
            ttl_hours: 24,
            size_cap_mb: 10,
        });

        let output = recorder
            .maybe_write(temp.path(), &[1, 2, 3, 4], 16_000)
            .await
            .expect("disabled write path should succeed");
        assert!(output.is_none());
        assert_eq!(
            std::fs::read_dir(temp.path())
                .expect("list output dir")
                .count(),
            0
        );
    }

    #[tokio::test]
    async fn enabled_debug_wav_prunes_stale_and_oversize_artifacts() {
        let temp = tempdir().expect("tempdir");
        let stale = temp.path().join("stale.wav");
        std::fs::write(&stale, vec![1_u8; 16]).expect("write stale file");
        let stale_file = OpenOptions::new()
            .write(true)
            .open(&stale)
            .expect("open stale file");
        stale_file
            .set_modified(std::time::SystemTime::now() - Duration::from_secs(2 * 3_600))
            .expect("set stale modified time");

        let oversized = temp.path().join("oversized.wav");
        std::fs::write(&oversized, vec![2_u8; 2 * 1_024 * 1_024]).expect("write oversized file");

        let recorder = DebugWavRecorder::new(DebugWavConfig {
            enabled: true,
            directory: temp.path().display().to_string(),
            ttl_hours: 1,
            size_cap_mb: 1,
        });

        let written_path = recorder
            .maybe_write(temp.path(), &[0_i16; 1_600], 16_000)
            .await
            .expect("write should succeed")
            .expect("enabled mode should create wav");

        assert!(written_path.exists(), "new debug wav should remain");
        assert!(!stale.exists(), "stale artifact should be pruned by TTL");
        assert!(
            !oversized.exists(),
            "oversized artifact should be pruned by size cap"
        );
    }
}
