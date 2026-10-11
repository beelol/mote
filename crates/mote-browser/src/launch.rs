use anyhow::{bail, Result};
use std::path::{Component, Path, PathBuf};

pub const CHROME_VERSION: &str = "155.0.8059.39";

#[derive(Clone, Debug)]
pub struct LaunchConfig {
    pub(crate) executable: PathBuf,
    pub(crate) profile: PathBuf,
}

impl LaunchConfig {
    pub fn new(data: &Path, character: &str, executable: &Path) -> Result<Self> {
        if !data.is_absolute()
            || !executable.is_absolute()
            || data
                .components()
                .any(|part| matches!(part, Component::ParentDir))
            || data
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains("google/chrome")
            || character.is_empty()
            || !character
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            bail!("browser requires a private Mote data root and a character name");
        }
        Ok(Self {
            executable: executable.into(),
            profile: data.join("browser").join(character).join("local"),
        })
    }

    pub fn profile(&self) -> &Path {
        &self.profile
    }

    pub fn arguments(&self) -> Vec<String> {
        vec![
            format!("--user-data-dir={}", self.profile.display()),
            "--remote-debugging-address=127.0.0.1".into(),
            "--remote-debugging-port=0".into(),
            "--use-mock-keychain".into(),
            "--no-first-run".into(),
            "--no-default-browser-check".into(),
            "--disable-background-networking".into(),
            "--disable-component-update".into(),
            "--disable-sync".into(),
            "--enable-automation".into(),
            "--app=about:blank".into(),
        ]
    }

    pub fn verify_version(&self, version: &str) -> Result<()> {
        if version.trim() != format!("Google Chrome for Testing {CHROME_VERSION}") {
            bail!("only Chrome for Testing {CHROME_VERSION} may launch");
        }
        Ok(())
    }
}
