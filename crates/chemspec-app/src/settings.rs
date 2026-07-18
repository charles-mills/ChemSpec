//! Versioned, cross-platform application preference persistence.
//!
//! This file stores presentation and provider choices only. Credentials are
//! deliberately outside the settings contract.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

const SETTINGS_SCHEMA_VERSION: u32 = 1;
const MAX_SETTINGS_BYTES: u64 = 64 * 1024;
static SETTINGS_WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppMode {
    Local,
    CodexBinary,
    Api,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChemicalLabels {
    Formulae,
    Names,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppSettings {
    pub app_mode: AppMode,
    pub chemical_labels: ChemicalLabels,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            app_mode: AppMode::Local,
            chemical_labels: ChemicalLabels::Formulae,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsFile {
    schema_version: u32,
    app_mode: AppMode,
    chemical_labels: ChemicalLabels,
}

impl From<AppSettings> for SettingsFile {
    fn from(settings: AppSettings) -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            app_mode: settings.app_mode,
            chemical_labels: settings.chemical_labels,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadOutcome {
    Missing,
    Loaded(AppSettings),
    Invalid(String),
}

pub fn load() -> LoadOutcome {
    settings_path().map_or_else(LoadOutcome::Invalid, |path| load_from(&path))
}

pub fn save(settings: AppSettings) -> Result<(), String> {
    let path = settings_path()?;
    save_to(&path, settings)
}

fn settings_path() -> Result<PathBuf, String> {
    if let Some(directory) = std::env::var_os("CHEMSPEC_CONFIG_DIR").filter(|path| !path.is_empty())
    {
        return Ok(PathBuf::from(directory).join("settings.json"));
    }
    ProjectDirs::from("dev", "charlesmills", "chemspec")
        .map(|directories| directories.config_dir().join("settings.json"))
        .ok_or_else(|| "the operating system did not provide a settings directory".to_owned())
}

fn load_from(path: &Path) -> LoadOutcome {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return LoadOutcome::Missing,
        Err(error) => return LoadOutcome::Invalid(format!("could not inspect settings: {error}")),
    };
    if metadata.len() > MAX_SETTINGS_BYTES {
        return LoadOutcome::Invalid("settings file is unexpectedly large".to_owned());
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return LoadOutcome::Invalid(format!("could not read settings: {error}")),
    };
    let file = match serde_json::from_slice::<SettingsFile>(&bytes) {
        Ok(file) => file,
        Err(error) => return LoadOutcome::Invalid(format!("settings are not valid: {error}")),
    };
    if file.schema_version != SETTINGS_SCHEMA_VERSION {
        return LoadOutcome::Invalid(format!(
            "settings schema {} is not supported",
            file.schema_version
        ));
    }
    LoadOutcome::Loaded(AppSettings {
        app_mode: file.app_mode,
        chemical_labels: file.chemical_labels,
    })
}

fn save_to(path: &Path, settings: AppSettings) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "settings path has no parent directory".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("could not create settings folder: {error}"))?;
    let bytes = serde_json::to_vec_pretty(&SettingsFile::from(settings))
        .map_err(|error| format!("could not encode settings: {error}"))?;
    let sequence = SETTINGS_WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(".settings-{}-{sequence}.tmp", std::process::id()));
    fs::write(&temporary, bytes).map_err(|error| format!("could not write settings: {error}"))?;
    if let Err(error) = atomic_replace(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("could not save settings: {error}"));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn atomic_replace(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(temporary, destination)
}

#[cfg(target_os = "windows")]
fn atomic_replace(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    if !destination.exists() {
        return fs::rename(temporary, destination);
    }
    let sequence = SETTINGS_WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let backup = destination.with_extension(format!("backup-{sequence}"));
    fs::rename(destination, &backup)?;
    if let Err(error) = fs::rename(temporary, destination) {
        let _ = fs::rename(&backup, destination);
        return Err(error);
    }
    let _ = fs::remove_file(backup);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "chemspec-settings-{name}-{}-{}",
            std::process::id(),
            SETTINGS_WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn missing_settings_are_a_first_launch() {
        let directory = test_directory("missing");
        assert_eq!(
            load_from(&directory.join("settings.json")),
            LoadOutcome::Missing
        );
    }

    #[test]
    fn settings_round_trip_through_an_atomic_file() {
        let directory = test_directory("round-trip");
        let path = directory.join("settings.json");
        let expected = AppSettings {
            app_mode: AppMode::CodexBinary,
            chemical_labels: ChemicalLabels::Names,
        };
        save_to(&path, expected).expect("settings save");
        assert_eq!(load_from(&path), LoadOutcome::Loaded(expected));
        fs::remove_dir_all(directory).expect("remove settings test directory");
    }

    #[test]
    fn malformed_and_future_settings_fail_closed() {
        let directory = test_directory("invalid");
        fs::create_dir_all(&directory).expect("settings test directory");
        let path = directory.join("settings.json");
        fs::write(&path, b"not json").expect("invalid settings fixture");
        assert!(matches!(load_from(&path), LoadOutcome::Invalid(_)));
        fs::write(
            &path,
            br#"{"schema_version":2,"app_mode":"local","chemical_labels":"formulae"}"#,
        )
        .expect("future settings fixture");
        assert!(matches!(load_from(&path), LoadOutcome::Invalid(_)));
        fs::remove_dir_all(directory).expect("remove settings test directory");
    }

    #[test]
    fn replacing_settings_never_leaves_a_partial_document() {
        let directory = test_directory("replace");
        let path = directory.join("settings.json");
        save_to(&path, AppSettings::default()).expect("initial settings save");
        let updated = AppSettings {
            app_mode: AppMode::Local,
            chemical_labels: ChemicalLabels::Names,
        };
        save_to(&path, updated).expect("replacement settings save");
        assert_eq!(load_from(&path), LoadOutcome::Loaded(updated));
        let temporary_files = fs::read_dir(&directory)
            .expect("settings directory")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".settings-")
            })
            .count();
        assert_eq!(temporary_files, 0);
        fs::remove_dir_all(directory).expect("remove settings test directory");
    }
}
