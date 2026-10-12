//! Checks a model file against the size and SHA-256 its recipe promises.

use craft_ai_features::ModelFile;
use craft_ai_model::{AiError, ErrorKind};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::download::{Phase, Progress};

/// Hashes `path` and compares it with the recipe. Reports progress as `Verifying`. Fails with
/// `Cancelled` when `cancel` is set, and with `Provider` when the size or hash differs.
pub fn verify_file(
    path: &Path,
    file: &ModelFile,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
) -> Result<(), AiError> {
    let unreadable = |_| {
        AiError::new(
            ErrorKind::Internal,
            "could not read the model file to check it",
        )
    };
    let mut input = std::fs::File::open(path).map_err(unreadable)?;
    let size = input.metadata().map_err(unreadable)?.len();
    if size != file.size_bytes {
        return Err(mismatch());
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut done = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(AiError::new(ErrorKind::Cancelled, "check cancelled"));
        }
        let n = input.read(&mut buffer).map_err(unreadable)?;
        if n == 0 {
            break;
        }
        hasher.update(buffer.get(..n).unwrap_or_default());
        done += n as u64;
        progress(Progress {
            phase: Phase::Verifying,
            done,
            total: size,
        });
    }
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if digest == file.sha256.to_ascii_lowercase() {
        Ok(())
    } else {
        Err(mismatch())
    }
}

fn mismatch() -> AiError {
    AiError::new(
        ErrorKind::Provider,
        "the model file does not match the size and checksum it should have",
    )
}
