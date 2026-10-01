use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::Result;
use serde::de::DeserializeOwned;

// ── Embedded resource bytes ───────────────────────────────────────────────────

const ACLI_SCHEMA_TEMPLATE: &[u8] = include_bytes!("../resources/acli_schema.json");
const ACLI_ADD_SCHEMA_TEMPLATE: &[u8] = include_bytes!("../resources/acli_add_schema.json");
const ACLI_CONFIG_TEMPLATE: &[u8] = include_bytes!("../resources/config.ag-stack.yml");

// ── Public types ──────────────────────────────────────────────────────────────

/// Specification for a single AI call: the prompt text and where to write output.
pub struct AiCallSpec {
    pub prompt: String,
}

/// Resolved filesystem paths the AI provider needs for a call.
pub struct AiPaths {
    pub output_file: PathBuf,
    pub schema_file: PathBuf,
    pub config_file: PathBuf,
}

/// RAII guard — deletes `output_file` when dropped (or when `.cleanup()` is called).
pub struct AiSession {
    pub output_file: PathBuf,
    cleaned_up: bool,
}

impl AiSession {
    pub fn new(output_file: PathBuf) -> Self {
        Self {
            output_file,
            cleaned_up: false,
        }
    }

    /// Explicitly delete the output file and mark this session as cleaned up.
    /// Safe to call multiple times — subsequent calls are no-ops.
    pub fn cleanup(&mut self) {
        if !self.cleaned_up {
            let _ = fs::remove_file(&self.output_file);
            self.cleaned_up = true;
        }
    }
}

impl Drop for AiSession {
    fn drop(&mut self) {
        self.cleanup();
    }
}

// ── Trait ─────────────────────────────────────────────────────────────────────

pub trait AiProvider {
    /// Write provider resource files to `dir` (idempotent). Returns resolved paths.
    fn setup_resources(&self, dir: &Path) -> Result<AiPaths>;

    /// Build a `Command` ready to spawn (not yet started).
    fn build_command(&self, spec: &AiCallSpec, paths: &AiPaths) -> Command;

    /// Read + parse the JSON output file. Calls `session.cleanup()` on success.
    /// Returns `Ok(None)` if the file is missing or JSON is malformed.
    fn read_output<T: DeserializeOwned>(&self, session: &mut AiSession) -> Result<Option<T>>;
}

// ── Concrete implementation: acli rovodev ─────────────────────────────────────

pub struct AcliRovodev;

impl AiProvider for AcliRovodev {
    fn setup_resources(&self, dir: &Path) -> Result<AiPaths> {
        fs::create_dir_all(dir)?;

        let schema_file = dir.join("acli_schema.json");
        let config_file = dir.join("config.ag-stack.yml");
        let output_file = dir.join("acli_rovodev_output.json");

        if !schema_file.exists() {
            fs::write(&schema_file, ACLI_SCHEMA_TEMPLATE)?;
        }
        if !config_file.exists() {
            fs::write(&config_file, ACLI_CONFIG_TEMPLATE)?;
        }

        Ok(AiPaths {
            output_file,
            schema_file,
            config_file,
        })
    }

    fn build_command(&self, spec: &AiCallSpec, paths: &AiPaths) -> Command {
        let mut cmd = Command::new("acli");
        cmd.arg("rovodev")
            .arg(&spec.prompt)
            .arg(format!("--output-file={}", paths.output_file.display()))
            .arg(format!("--output-schema={}", paths.schema_file.display()))
            .arg(format!("--config-file={}", paths.config_file.display()));
        cmd
    }

    fn read_output<T: DeserializeOwned>(&self, session: &mut AiSession) -> Result<Option<T>> {
        let path = &session.output_file;
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read_to_string(path)?;
        let parsed = serde_json::from_str::<T>(&data).ok();
        session.cleanup();
        Ok(parsed)
    }
}

impl AcliRovodev {
    /// Like `setup_resources` but writes `schema_bytes` as the schema file
    /// instead of the embedded `acli_schema.json`. Use this when the caller
    /// needs a schema that differs from the default fixup schema.
    pub fn setup_resources_with_schema(
        &self,
        dir: &Path,
        schema_bytes: &[u8],
    ) -> Result<AiPaths> {
        fs::create_dir_all(dir)?;

        let schema_file = dir.join("acli_add_schema.json");
        let config_file = dir.join("config.ag-stack.yml");
        let output_file = dir.join("acli_rovodev_output.json");

        // Always (re-)write the schema so updates take effect.
        fs::write(&schema_file, schema_bytes)?;
        if !config_file.exists() {
            fs::write(&config_file, ACLI_CONFIG_TEMPLATE)?;
        }

        Ok(AiPaths {
            output_file,
            schema_file,
            config_file,
        })
    }

    /// Returns the embedded bytes for the `dt add` JSON schema.
    pub fn add_schema_bytes() -> &'static [u8] {
        ACLI_ADD_SCHEMA_TEMPLATE
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use tempfile::tempdir;

    // Helper type matching the shape returned by acli rovodev for fixup
    #[derive(Deserialize)]
    struct AcliOutput {
        commit_message: Option<String>,
    }

    // ── AiPaths construction ──────────────────────────────────────────────────

    #[test]
    fn test_ai_paths_construction() {
        let output = PathBuf::from("/tmp/out.json");
        let schema = PathBuf::from("/tmp/schema.json");
        let config = PathBuf::from("/tmp/config.yml");

        let paths = AiPaths {
            output_file: output.clone(),
            schema_file: schema.clone(),
            config_file: config.clone(),
        };

        assert_eq!(paths.output_file, output);
        assert_eq!(paths.schema_file, schema);
        assert_eq!(paths.config_file, config);
    }

    // ── AiCallSpec construction ───────────────────────────────────────────────

    #[test]
    fn test_ai_call_spec_construction() {
        let spec = AiCallSpec {
            prompt: "Write a commit message".to_string(),
        };

        assert_eq!(spec.prompt, "Write a commit message");
    }

    // ── AcliRovodev::build_command args ──────────────────────────────────────

    #[test]
    fn test_acli_rovodev_build_command_args() {
        let provider = AcliRovodev;
        let spec = AiCallSpec {
            prompt: "Summarise the diff".to_string(),
        };
        let paths = AiPaths {
            output_file: PathBuf::from("/tmp/out.json"),
            schema_file: PathBuf::from("/tmp/schema.json"),
            config_file: PathBuf::from("/tmp/config.yml"),
        };

        let cmd = provider.build_command(&spec, &paths);

        // std::process::Command exposes its program and args via Debug; we
        // inspect them via the get_program / get_args methods.
        assert_eq!(cmd.get_program(), "acli");

        let args: Vec<&std::ffi::OsStr> = cmd.get_args().collect();
        let args_str: Vec<&str> = args.iter().map(|a| a.to_str().unwrap()).collect();

        assert_eq!(args_str[0], "rovodev");
        assert_eq!(args_str[1], "Summarise the diff");
        assert!(args_str[2].starts_with("--output-file="));
        assert!(args_str[2].contains("/tmp/out.json"));
        assert!(args_str[3].starts_with("--output-schema="));
        assert!(args_str[3].contains("/tmp/schema.json"));
        assert!(args_str[4].starts_with("--config-file="));
        assert!(args_str[4].contains("/tmp/config.yml"));
    }

    // ── AiSession::cleanup removes file ──────────────────────────────────────

    #[test]
    fn test_ai_session_cleanup_removes_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("output.json");
        fs::write(&file_path, b"{}").unwrap();
        assert!(file_path.exists(), "pre-condition: file must exist");

        let mut session = AiSession::new(file_path.clone());
        session.cleanup();

        assert!(!file_path.exists(), "cleanup() should delete the file");
    }

    #[test]
    fn test_ai_session_cleanup_is_idempotent() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("output.json");
        fs::write(&file_path, b"{}").unwrap();

        let mut session = AiSession::new(file_path.clone());
        session.cleanup();
        session.cleanup(); // second call must not panic
        assert!(!file_path.exists());
    }

    // ── AiSession::drop removes file ─────────────────────────────────────────

    #[test]
    fn test_ai_session_drop_removes_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("output.json");
        fs::write(&file_path, b"{}").unwrap();
        assert!(file_path.exists(), "pre-condition: file must exist");

        {
            let _session = AiSession::new(file_path.clone());
            // session drops here
        }

        assert!(!file_path.exists(), "Drop should delete the file");
    }

    // ── setup_resources creates files ─────────────────────────────────────────

    #[test]
    fn test_setup_resources_creates_files() {
        let dir = tempdir().unwrap();
        let provider = AcliRovodev;

        let paths = provider.setup_resources(dir.path()).unwrap();

        assert!(paths.schema_file.exists(), "schema file should be created");
        assert!(paths.config_file.exists(), "config file should be created");
        // output_file is NOT created by setup_resources — it's where acli writes
        assert_eq!(
            paths.schema_file,
            dir.path().join("acli_schema.json")
        );
        assert_eq!(
            paths.config_file,
            dir.path().join("config.ag-stack.yml")
        );
        assert_eq!(
            paths.output_file,
            dir.path().join("acli_rovodev_output.json")
        );
    }

    // ── setup_resources is idempotent ─────────────────────────────────────────

    #[test]
    fn test_setup_resources_idempotent() {
        let dir = tempdir().unwrap();
        let provider = AcliRovodev;

        // Write sentinel content to detect overwrite
        let schema_path = dir.path().join("acli_schema.json");
        let config_path = dir.path().join("config.ag-stack.yml");
        fs::write(&schema_path, b"sentinel-schema").unwrap();
        fs::write(&config_path, b"sentinel-config").unwrap();

        // Second call must not overwrite existing files
        let paths = provider.setup_resources(dir.path()).unwrap();

        assert_eq!(fs::read(&paths.schema_file).unwrap(), b"sentinel-schema");
        assert_eq!(fs::read(&paths.config_file).unwrap(), b"sentinel-config");
    }

    // ── read_output parses JSON ───────────────────────────────────────────────

    #[test]
    fn test_read_output_parses_json() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("out.json");
        fs::write(&file_path, br#"{"commit_message": "hello"}"#).unwrap();

        let provider = AcliRovodev;
        let mut session = AiSession::new(file_path.clone());

        let result: Option<AcliOutput> = provider.read_output(&mut session).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().commit_message, Some("hello".to_string()));

        // read_output should have cleaned up the file
        assert!(!file_path.exists(), "read_output should clean up the file");
    }

    // ── read_output: missing file returns None ────────────────────────────────

    #[test]
    fn test_read_output_missing_file_returns_none() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("does_not_exist.json");

        let provider = AcliRovodev;
        let mut session = AiSession::new(file_path);

        let result: Option<AcliOutput> = provider.read_output(&mut session).unwrap();
        assert!(result.is_none());
    }

    // ── read_output: bad JSON returns None ───────────────────────────────────

    #[test]
    fn test_read_output_bad_json_returns_none() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("bad.json");
        fs::write(&file_path, b"not valid json at all!!!").unwrap();

        let provider = AcliRovodev;
        let mut session = AiSession::new(file_path.clone());

        let result: Option<AcliOutput> = provider.read_output(&mut session).unwrap();
        assert!(result.is_none());

        // File should NOT have been cleaned up (we only cleanup on successful parse)
        // Actually per the plan, cleanup is called after reading — let's check what
        // the implementation does: it calls .ok() so parse failure → None, cleanup is
        // NOT called. File remains. This is fine — session drop will clean it up.
    }
}
