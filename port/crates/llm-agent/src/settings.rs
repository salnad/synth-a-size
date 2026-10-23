//! The user's chat settings: whether the chat is on, and which provider and
//! model it uses. The chat is off until the user turns it on; with it off,
//! the plugin builds no agent and makes no network requests (D13). The API
//! key is never stored here (the plugin keeps it in the macOS Keychain).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::provider::{ProviderConfig, ProviderKind};

/// File name of the settings, in [`default_dir`].
pub const SETTINGS_FILE: &str = "chat.json";

/// Per-user chat settings, saved as JSON. Every field has a default, so a
/// missing or partial file loads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatSettings {
    /// Off by default (owner decision, D13).
    pub enabled: bool,
    /// `anthropic` (the default) or `openai` (any OpenAI-compatible server).
    pub provider: String,
    /// Overrides the provider's default model id.
    pub model: Option<String>,
    /// Overrides the provider's default API base URL.
    pub base_url: Option<String>,
}

impl Default for ChatSettings {
    fn default() -> Self {
        ChatSettings {
            enabled: false,
            provider: "anthropic".to_string(),
            model: None,
            base_url: None,
        }
    }
}

impl ChatSettings {
    /// The settings in `path`. A missing file gives the defaults (chat off);
    /// a file that cannot be read or parsed also gives the defaults, with the
    /// reason, so a damaged file never turns the chat on.
    pub fn load(path: &Path) -> (ChatSettings, Option<String>) {
        match std::fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(settings) => (settings, None),
                Err(error) => (
                    ChatSettings::default(),
                    Some(format!("{}: {error}", path.display())),
                ),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (ChatSettings::default(), None)
            }
            Err(error) => (
                ChatSettings::default(),
                Some(format!("{}: {error}", path.display())),
            ),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text + "\n").map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The provider to use, if the chat is on and `api_key` is usable. A
    /// local OpenAI-compatible server (a base URL on localhost) needs no key.
    pub fn provider_config(&self, api_key: Option<String>) -> Result<ProviderConfig, String> {
        if !self.enabled {
            return Err("the chat is off".to_string());
        }
        let kind = ProviderKind::parse(&self.provider)
            .ok_or_else(|| format!("unknown provider {:?}", self.provider))?;
        let mut config = ProviderConfig::new(kind, api_key.filter(|key| !key.is_empty()));
        if let Some(model) = &self.model {
            config.model = model.clone();
        }
        if let Some(url) = &self.base_url {
            config.base_url = url.clone();
        }
        let local =
            config.base_url.contains("://localhost") || config.base_url.contains("://127.0.0.1");
        if config.api_key.is_none() && !local {
            return Err(format!("no API key for {}", self.provider));
        }
        Ok(config)
    }
}

/// The folder for per-user settings: `~/Library/Application Support/synth-a-size`
/// on macOS, `$XDG_CONFIG_HOME/synth-a-size` (or `~/.config/synth-a-size`)
/// elsewhere. `None` without a home directory.
pub fn default_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        return home.map(|home| home.join("Library/Application Support/synth-a-size"));
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| home.map(|home| home.join(".config")))
        .map(|dir| dir.join("synth-a-size"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("sas-chat-settings-{name}-{}", std::process::id()))
    }

    #[test]
    fn the_chat_is_off_by_default_and_without_a_file() {
        assert!(!ChatSettings::default().enabled);
        let (settings, problem) = ChatSettings::load(&temp("missing").join(SETTINGS_FILE));
        assert!(!settings.enabled);
        assert!(problem.is_none());
        let error = settings.provider_config(Some("key".into())).unwrap_err();
        assert!(error.contains("off"));
    }

    #[test]
    fn a_damaged_file_leaves_the_chat_off() {
        let dir = temp("damaged");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(SETTINGS_FILE);
        std::fs::write(&path, "{\"enabled\": tru").unwrap();
        let (settings, problem) = ChatSettings::load(&path);
        assert!(!settings.enabled);
        assert!(problem.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_round_trip_and_build_a_provider() {
        let dir = temp("roundtrip");
        let path = dir.join(SETTINGS_FILE);
        let settings = ChatSettings {
            enabled: true,
            provider: "openai".into(),
            model: Some("local-model".into()),
            base_url: Some("http://localhost:11434/v1".into()),
        };
        settings.save(&path).unwrap();
        assert_eq!(ChatSettings::load(&path), (settings.clone(), None));
        // A local server needs no key.
        let config = settings.provider_config(None).unwrap();
        assert_eq!(config.model, "local-model");
        // A hosted provider does.
        let hosted = ChatSettings {
            enabled: true,
            ..ChatSettings::default()
        };
        assert!(hosted
            .provider_config(None)
            .unwrap_err()
            .contains("API key"));
        assert!(hosted.provider_config(Some("k".into())).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_partial_file_fills_in_defaults() {
        let parsed: ChatSettings = serde_json::from_str("{\"enabled\": true}").unwrap();
        assert!(parsed.enabled);
        assert_eq!(parsed.provider, "anthropic");
    }
}
