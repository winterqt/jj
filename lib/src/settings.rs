// Copyright 2020 The Jujutsu Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![allow(missing_docs)]

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;

use rand::prelude::*;
use rand_chacha::ChaCha20Rng;
use serde::Deserialize;

use crate::backend::ChangeId;
use crate::backend::Commit;
use crate::backend::Signature;
use crate::backend::Timestamp;
use crate::config::ConfigGetError;
use crate::config::ConfigGetResultExt as _;
use crate::config::ConfigTable;
use crate::config::ConfigValue;
use crate::config::StackedConfig;
use crate::config::ToConfigNamePath;
use crate::fmt_util::binary_prefix;
use crate::fsmonitor::FsmonitorSettings;
use crate::signing::SignBehavior;

#[derive(Debug, Clone)]
pub struct UserSettings {
    config: Arc<StackedConfig>,
    data: Arc<UserSettingsData>,
    rng: Arc<JJRng>,
}

#[derive(Debug)]
struct UserSettingsData {
    user_name: String,
    user_email: String,
    commit_timestamp: Option<Timestamp>,
    operation_timestamp: Option<Timestamp>,
    operation_hostname: String,
    operation_username: String,
    signing_behavior: SignBehavior,
    signing_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GitSettings {
    pub auto_local_bookmark: bool,
    pub abandon_unreachable_commits: bool,
    pub executable_path: PathBuf,
    pub write_change_id_header: bool,
}

impl GitSettings {
    pub fn from_settings(settings: &UserSettings) -> Result<Self, ConfigGetError> {
        Ok(GitSettings {
            auto_local_bookmark: settings.get_bool("git.auto-local-bookmark")?,
            abandon_unreachable_commits: settings.get_bool("git.abandon-unreachable-commits")?,
            executable_path: settings.get("git.executable-path")?,
            write_change_id_header: settings.get("git.write-change-id-header")?,
        })
    }
}

impl Default for GitSettings {
    fn default() -> Self {
        GitSettings {
            auto_local_bookmark: false,
            abandon_unreachable_commits: true,
            executable_path: PathBuf::from("git"),
            write_change_id_header: true,
        }
    }
}

/// Commit signing settings, describes how to and if to sign commits.
#[derive(Debug, Clone)]
pub struct SignSettings {
    /// What to actually do, see [SignBehavior].
    pub behavior: SignBehavior,
    /// The email address to compare against the commit author when determining
    /// if the existing signature is "our own" in terms of the sign behavior.
    pub user_email: String,
    /// The signing backend specific key, to be passed to the signing backend.
    pub key: Option<String>,
}

impl SignSettings {
    /// Check if a commit should be signed according to the configured behavior
    /// and email.
    pub fn should_sign(&self, commit: &Commit) -> bool {
        match self.behavior {
            SignBehavior::Drop => false,
            SignBehavior::Keep => {
                commit.secure_sig.is_some() && commit.author.email == self.user_email
            }
            SignBehavior::Own => commit.author.email == self.user_email,
            SignBehavior::Force => true,
        }
    }
}

fn to_timestamp(value: ConfigValue) -> Result<Timestamp, Box<dyn std::error::Error + Send + Sync>> {
    // Since toml_edit::Datetime isn't the date-time type used across our code
    // base, we accept both string and date-time types.
    if let Some(s) = value.as_str() {
        Ok(Timestamp::from_zoned(s.parse()?))
    } else if let Some(d) = value.as_datetime() {
        // It's easier to re-parse the TOML date-time expression.
        let s = d.to_string();
        Ok(Timestamp::from_zoned(s.parse()?))
    } else {
        let ty = value.type_name();
        Err(format!("invalid type: {ty}, expected a date-time").into())
    }
}

impl UserSettings {
    pub fn from_config(config: StackedConfig) -> Result<Self, ConfigGetError> {
        let rng_seed = config.get::<u64>("debug.randomness-seed").optional()?;
        Self::from_config_and_rng(config, Arc::new(JJRng::new(rng_seed)))
    }

    fn from_config_and_rng(config: StackedConfig, rng: Arc<JJRng>) -> Result<Self, ConfigGetError> {
        let user_name = config.get("user.name")?;
        let user_email = config.get("user.email")?;
        let commit_timestamp = config
            .get_value_with("debug.commit-timestamp", to_timestamp)
            .optional()?;
        let operation_timestamp = config
            .get_value_with("debug.operation-timestamp", to_timestamp)
            .optional()?;
        let operation_hostname = config.get("operation.hostname")?;
        let operation_username = config.get("operation.username")?;
        let signing_behavior = config.get("signing.behavior")?;
        let signing_key = config.get("signing.key").optional()?;
        let data = UserSettingsData {
            user_name,
            user_email,
            commit_timestamp,
            operation_timestamp,
            operation_hostname,
            operation_username,
            signing_behavior,
            signing_key,
        };
        Ok(UserSettings {
            config: Arc::new(config),
            data: Arc::new(data),
            rng,
        })
    }

    /// Like [`UserSettings::from_config()`], but retains the internal state.
    ///
    /// This ensures that no duplicated change IDs are generated within the
    /// current process. New `debug.randomness-seed` value is ignored.
    pub fn with_new_config(&self, config: StackedConfig) -> Result<Self, ConfigGetError> {
        Self::from_config_and_rng(config, self.rng.clone())
    }

    pub fn get_rng(&self) -> Arc<JJRng> {
        self.rng.clone()
    }

    pub fn user_name(&self) -> &str {
        &self.data.user_name
    }

    // Must not be changed to avoid git pushing older commits with no set name
    pub const USER_NAME_PLACEHOLDER: &'static str = "(no name configured)";

    pub fn user_email(&self) -> &str {
        &self.data.user_email
    }

    pub fn fsmonitor_settings(&self) -> Result<FsmonitorSettings, ConfigGetError> {
        FsmonitorSettings::from_settings(self)
    }

    // Must not be changed to avoid git pushing older commits with no set email
    // address
    pub const USER_EMAIL_PLACEHOLDER: &'static str = "(no email configured)";

    pub fn commit_timestamp(&self) -> Option<Timestamp> {
        self.data.commit_timestamp
    }

    pub fn operation_timestamp(&self) -> Option<Timestamp> {
        self.data.operation_timestamp
    }

    pub fn operation_hostname(&self) -> &str {
        &self.data.operation_hostname
    }

    pub fn operation_username(&self) -> &str {
        &self.data.operation_username
    }

    pub fn signature(&self) -> Signature {
        let timestamp = self.data.commit_timestamp.unwrap_or_else(Timestamp::now);
        Signature {
            name: self.user_name().to_owned(),
            email: self.user_email().to_owned(),
            timestamp,
        }
    }

    /// Returns low-level config object.
    ///
    /// You should typically use `settings.get_<type>()` methods instead.
    pub fn config(&self) -> &StackedConfig {
        &self.config
    }

    pub fn git_settings(&self) -> Result<GitSettings, ConfigGetError> {
        GitSettings::from_settings(self)
    }

    // separate from sign_settings as those two are needed in pretty different
    // places
    pub fn signing_backend(&self) -> Result<Option<String>, ConfigGetError> {
        let backend = self.get_string("signing.backend")?;
        Ok((backend != "none").then_some(backend))
    }

    pub fn sign_settings(&self) -> SignSettings {
        SignSettings {
            behavior: self.data.signing_behavior,
            user_email: self.data.user_email.clone(),
            key: self.data.signing_key.clone(),
        }
    }
}

/// General-purpose accessors.
impl UserSettings {
    /// Looks up value of the specified type `T` by `name`.
    pub fn get<'de, T: Deserialize<'de>>(
        &self,
        name: impl ToConfigNamePath,
    ) -> Result<T, ConfigGetError> {
        self.config.get(name)
    }

    /// Looks up string value by `name`.
    pub fn get_string(&self, name: impl ToConfigNamePath) -> Result<String, ConfigGetError> {
        self.get(name)
    }

    /// Looks up integer value by `name`.
    pub fn get_int(&self, name: impl ToConfigNamePath) -> Result<i64, ConfigGetError> {
        self.get(name)
    }

    /// Looks up boolean value by `name`.
    pub fn get_bool(&self, name: impl ToConfigNamePath) -> Result<bool, ConfigGetError> {
        self.get(name)
    }

    /// Looks up generic value by `name`.
    pub fn get_value(&self, name: impl ToConfigNamePath) -> Result<ConfigValue, ConfigGetError> {
        self.config.get_value(name)
    }

    /// Looks up value by `name`, converts it by using the given function.
    pub fn get_value_with<T, E: Into<Box<dyn std::error::Error + Send + Sync>>>(
        &self,
        name: impl ToConfigNamePath,
        convert: impl FnOnce(ConfigValue) -> Result<T, E>,
    ) -> Result<T, ConfigGetError> {
        self.config.get_value_with(name, convert)
    }

    /// Looks up sub table by `name`.
    ///
    /// Use `table_keys(prefix)` and `get([prefix, key])` instead if table
    /// values have to be converted to non-generic value type.
    pub fn get_table(&self, name: impl ToConfigNamePath) -> Result<ConfigTable, ConfigGetError> {
        self.config.get_table(name)
    }

    /// Returns iterator over sub table keys at `name`.
    pub fn table_keys(&self, name: impl ToConfigNamePath) -> impl Iterator<Item = &str> {
        self.config.table_keys(name)
    }
}

/// This Rng uses interior mutability to allow generating random values using an
/// immutable reference. It also fixes a specific seedable RNG for
/// reproducibility.
#[derive(Debug)]
pub struct JJRng(Mutex<ChaCha20Rng>);
impl JJRng {
    pub fn new_change_id(&self, length: usize) -> ChangeId {
        let mut rng = self.0.lock().unwrap();
        let random_bytes = (0..length).map(|_| rng.random::<u8>()).collect();
        ChangeId::new(random_bytes)
    }

    /// Creates a new RNGs. Could be made public, but we'd like to encourage all
    /// RNGs references to point to the same RNG.
    fn new(seed: Option<u64>) -> Self {
        Self(Mutex::new(JJRng::internal_rng_from_seed(seed)))
    }

    fn internal_rng_from_seed(seed: Option<u64>) -> ChaCha20Rng {
        match seed {
            Some(seed) => ChaCha20Rng::seed_from_u64(seed),
            None => ChaCha20Rng::from_os_rng(),
        }
    }
}

/// A size in bytes optionally formatted/serialized with binary prefixes
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct HumanByteSize(pub u64);

impl std::fmt::Display for HumanByteSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (value, prefix) = binary_prefix(self.0 as f32);
        write!(f, "{value:.1}{prefix}B")
    }
}

impl FromStr for HumanByteSize {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse() {
            Ok(bytes) => Ok(HumanByteSize(bytes)),
            Err(_) => {
                let bytes = parse_human_byte_size(s)?;
                Ok(HumanByteSize(bytes))
            }
        }
    }
}

impl TryFrom<ConfigValue> for HumanByteSize {
    type Error = &'static str;

    fn try_from(value: ConfigValue) -> Result<Self, Self::Error> {
        if let Some(n) = value.as_integer() {
            let n = u64::try_from(n).map_err(|_| "Integer out of range")?;
            Ok(HumanByteSize(n))
        } else if let Some(s) = value.as_str() {
            s.parse()
        } else {
            Err("Expected a positive integer or a string in '<number><unit>' form")
        }
    }
}

fn parse_human_byte_size(v: &str) -> Result<u64, &'static str> {
    let digit_end = v.find(|c: char| !c.is_ascii_digit()).unwrap_or(v.len());
    if digit_end == 0 {
        return Err("must start with a number");
    }
    let (digits, trailing) = v.split_at(digit_end);
    let exponent = match trailing.trim_start() {
        "" | "B" => 0,
        unit => {
            const PREFIXES: [char; 8] = ['K', 'M', 'G', 'T', 'P', 'E', 'Z', 'Y'];
            let Some(prefix) = PREFIXES.iter().position(|&x| unit.starts_with(x)) else {
                return Err("unrecognized unit prefix");
            };
            let ("" | "B" | "i" | "iB") = &unit[1..] else {
                return Err("unrecognized unit");
            };
            prefix as u32 + 1
        }
    };
    // A string consisting only of base 10 digits is either a valid u64 or really
    // huge.
    let factor = digits.parse::<u64>().unwrap_or(u64::MAX);
    Ok(factor.saturating_mul(1024u64.saturating_pow(exponent)))
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    #[test]
    fn byte_size_parse() {
        assert_eq!(parse_human_byte_size("0"), Ok(0));
        assert_eq!(parse_human_byte_size("42"), Ok(42));
        assert_eq!(parse_human_byte_size("42B"), Ok(42));
        assert_eq!(parse_human_byte_size("42 B"), Ok(42));
        assert_eq!(parse_human_byte_size("42K"), Ok(42 * 1024));
        assert_eq!(parse_human_byte_size("42 K"), Ok(42 * 1024));
        assert_eq!(parse_human_byte_size("42 KB"), Ok(42 * 1024));
        assert_eq!(parse_human_byte_size("42 KiB"), Ok(42 * 1024));
        assert_eq!(
            parse_human_byte_size("42 LiB"),
            Err("unrecognized unit prefix")
        );
        assert_eq!(parse_human_byte_size("42 KiC"), Err("unrecognized unit"));
        assert_eq!(parse_human_byte_size("42 KC"), Err("unrecognized unit"));
        assert_eq!(
            parse_human_byte_size("KiB"),
            Err("must start with a number")
        );
        assert_eq!(parse_human_byte_size(""), Err("must start with a number"));
    }

    #[test]
    fn byte_size_from_config_value() {
        assert_eq!(
            HumanByteSize::try_from(ConfigValue::from(42)).unwrap(),
            HumanByteSize(42)
        );
        assert_eq!(
            HumanByteSize::try_from(ConfigValue::from("42K")).unwrap(),
            HumanByteSize(42 * 1024)
        );
        assert_matches!(
            HumanByteSize::try_from(ConfigValue::from(-1)),
            Err("Integer out of range")
        );
    }
}
