// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the chat's API keys live: the macOS Keychain (owner's working
//! default), never plugin state, presets, settings files or logs. An
//! environment variable (`SAS_LLM_API_KEY`, or the provider's own such as
//! `ANTHROPIC_API_KEY`) takes precedence, for development.

/// Keychain service name for the keys; the account is the provider.
pub const KEYCHAIN_SERVICE: &str = "synth-a-size chat";

/// Reads and stores API keys by provider (`anthropic`, `openai`).
pub trait KeyStore: Send + Sync {
    fn get(&self, provider: &str) -> Option<String>;
    fn set(&self, provider: &str, key: &str) -> Result<(), String>;
}

/// The environment variable a provider's key can come from.
fn provider_variable(provider: &str) -> Option<&'static str> {
    match provider {
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        "openai" => Some("OPENAI_API_KEY"),
        _ => None,
    }
}

/// Environment variables first, then the Keychain on macOS.
pub struct SystemKeys;

impl KeyStore for SystemKeys {
    fn get(&self, provider: &str) -> Option<String> {
        let from_env = std::env::var("SAS_LLM_API_KEY")
            .ok()
            .or_else(|| provider_variable(provider).and_then(|name| std::env::var(name).ok()))
            .filter(|key| !key.is_empty());
        from_env.or_else(|| keychain::get(provider))
    }

    fn set(&self, provider: &str, key: &str) -> Result<(), String> {
        keychain::set(provider, key)
    }
}

#[cfg(target_os = "macos")]
mod keychain {
    use security_framework::passwords::{get_generic_password, set_generic_password};

    use super::KEYCHAIN_SERVICE;

    pub fn get(provider: &str) -> Option<String> {
        let bytes = get_generic_password(KEYCHAIN_SERVICE, provider).ok()?;
        String::from_utf8(bytes).ok().filter(|key| !key.is_empty())
    }

    pub fn set(provider: &str, key: &str) -> Result<(), String> {
        set_generic_password(KEYCHAIN_SERVICE, provider, key.as_bytes())
            .map_err(|error| format!("cannot store the key in the Keychain: {error}"))
    }
}

#[cfg(not(target_os = "macos"))]
mod keychain {
    pub fn get(_provider: &str) -> Option<String> {
        None
    }

    pub fn set(provider: &str, _key: &str) -> Result<(), String> {
        let variable = super::provider_variable(provider).unwrap_or("SAS_LLM_API_KEY");
        Err(format!(
            "this build cannot store keys; set {variable} in the environment instead"
        ))
    }
}
