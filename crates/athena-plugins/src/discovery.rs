//! Plugin manifest discovery helpers.

use super::{
    validate_plugin_manifest, PluginError, PluginManager, PluginManifest, MAX_MANIFEST_BYTES,
};
use std::fs;
use std::path::Path;

impl PluginManager {
    pub fn discover_plugins(
        &self,
        dir: &Path,
    ) -> Result<Vec<Result<PluginManifest, PluginError>>, PluginError> {
        let entries = fs::read_dir(dir)?;
        let mut results = Vec::new();

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    results.push(Err(PluginError::ManifestIo(e)));
                    continue;
                }
            };

            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            // Reject oversized manifests before reading to bound memory usage.
            let metadata = match fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if metadata.len() > MAX_MANIFEST_BYTES {
                log::warn!(
                    "Skipping oversized plugin manifest ({} bytes): {}",
                    metadata.len(),
                    path.display()
                );
                continue;
            }

            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    results.push(Err(PluginError::ManifestIo(e)));
                    continue;
                }
            };

            match serde_json::from_str::<PluginManifest>(&content) {
                Ok(manifest) => {
                    if let Err(e) = validate_plugin_manifest(&manifest) {
                        results.push(Err(PluginError::ValidationFailed(format!(
                            "manifest {} failed validation: {}",
                            path.to_string_lossy(),
                            e
                        ))));
                    } else {
                        results.push(Ok(manifest));
                    }
                }
                Err(e) => results.push(Err(PluginError::ManifestParse {
                    path: path.to_string_lossy().into_owned(),
                    source: e,
                })),
            }
        }

        Ok(results)
    }

    pub fn discover_and_register(
        &self,
        dir: &Path,
    ) -> Result<(Vec<String>, Vec<PluginError>), PluginError> {
        let discovered = self.discover_plugins(dir)?;
        let mut registered = Vec::new();
        let mut errors = Vec::new();

        for result in discovered {
            match result {
                Ok(manifest) => match self.register_plugin(manifest) {
                    Ok(id) => registered.push(id),
                    Err(e) => errors.push(e),
                },
                Err(e) => errors.push(e),
            }
        }

        Ok((registered, errors))
    }
}
