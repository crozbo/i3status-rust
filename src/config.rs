use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::Deref;
use std::rc::Rc;
use std::str::FromStr;

use serde::de::{Deserialize, Deserializer};
use serde_derive::Deserialize;
use toml::value;

use crate::de::*;
use crate::errors;
use crate::icons;
use crate::input::MouseButton;
use crate::themes::Theme;

#[derive(Debug)]
pub struct SharedConfig {
    pub theme: Rc<Theme>,
    icons: Rc<HashMap<String, String>>,
    pub scrolling: Scrolling,
}

impl SharedConfig {
    pub fn new(config: &Config) -> Self {
        let mut icons = config.icons.clone();
        // Apply `icons_format`
        for icon in icons.values_mut() {
            *icon = config.icons_format.replace("{icon}", icon);
        }
        Self {
            theme: Rc::new(config.theme.clone()),
            icons: Rc::new(icons),
            scrolling: config.scrolling,
        }
    }

    pub fn theme_override(&mut self, overrides: &HashMap<String, String>) -> errors::Result<()> {
        let mut theme = self.theme.as_ref().clone();
        for entry in overrides {
            match entry.0.as_str() {
                "idle_fg" => theme.idle_fg = Some(entry.1.to_string()),
                "idle_bg" => theme.idle_bg = Some(entry.1.to_string()),
                "info_fg" => theme.info_fg = Some(entry.1.to_string()),
                "info_bg" => theme.info_bg = Some(entry.1.to_string()),
                "good_fg" => theme.good_fg = Some(entry.1.to_string()),
                "good_bg" => theme.good_bg = Some(entry.1.to_string()),
                "warning_fg" => theme.warning_fg = Some(entry.1.to_string()),
                "warning_bg" => theme.warning_bg = Some(entry.1.to_string()),
                "critical_fg" => theme.critical_fg = Some(entry.1.to_string()),
                "critical_bg" => theme.critical_bg = Some(entry.1.to_string()),
                x => {
                    return Err(errors::ConfigurationError(
                        format!("Theme element \"{}\" cannot be overriden", x),
                        (String::new(), String::new()),
                    ))
                }
            }
        }
        self.theme = Rc::new(theme);
        Ok(())
    }

    pub fn get_icon(&self, icon: &str) -> Option<String> {
        // TODO return `Option<&String>`
        self.icons.get(icon).cloned()
    }
}

impl Default for SharedConfig {
    fn default() -> Self {
        Self {
            theme: Rc::new(Theme::default()),
            icons: Rc::new(icons::default()),
            scrolling: Scrolling::default(),
        }
    }
}

impl Clone for SharedConfig {
    fn clone(&self) -> Self {
        Self {
            theme: Rc::clone(&self.theme),
            icons: Rc::clone(&self.icons),
            scrolling: self.scrolling,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(default = "icons::default", deserialize_with = "deserialize_icons")]
    pub icons: HashMap<String, String>,

    #[serde(default = "Theme::default")]
    pub theme: Theme,

    #[serde(default = "Config::default_icons_format")]
    pub icons_format: String,

    #[serde(default = "Scrolling::default")]
    pub scrolling: Scrolling,
    /// Direction of scrolling, "natural" or "reverse".
    ///
    /// Configuring natural scrolling on input devices changes the way i3status-rust
    /// processes mouse wheel events: pushing the wheen away now is interpreted as downward
    /// motion which is undesired for sliders. Use "natural" to invert this.
    #[serde(rename = "block", deserialize_with = "deserialize_blocks")]
    pub blocks: Vec<(String, value::Value)>,
}

impl Config {
    fn default_icons_format() -> String {
        " {icon} ".to_string()
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            icons: icons::default(),
            theme: Theme::default(),
            icons_format: Config::default_icons_format(),
            scrolling: Scrolling::default(),
            blocks: Vec::new(),
        }
    }
}

#[derive(Deserialize, Copy, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Scrolling {
    Reverse,
    Natural,
}

#[derive(Copy, Clone, Debug)]
pub enum LogicalDirection {
    Up,
    Down,
}

impl Scrolling {
    pub fn to_logical_direction(self, button: MouseButton) -> Option<LogicalDirection> {
        use LogicalDirection::*;
        use MouseButton::*;
        use Scrolling::*;
        match (self, button) {
            (Reverse, WheelUp) | (Natural, WheelDown) => Some(Up),
            (Reverse, WheelDown) | (Natural, WheelUp) => Some(Down),
            _ => None,
        }
    }
}

impl Default for Scrolling {
    fn default() -> Self {
        Scrolling::Reverse
    }
}

fn deserialize_blocks<'de, D>(deserializer: D) -> Result<Vec<(String, value::Value)>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut blocks: Vec<(String, value::Value)> = Vec::new();
    let raw_blocks: Vec<value::Table> = Deserialize::deserialize(deserializer)?;
    for mut entry in raw_blocks {
        if let Some(name) = entry.remove("block") {
            if let Some(name) = name.as_str() {
                blocks.push((name.to_owned(), value::Value::Table(entry)))
            }
        }
    }

    Ok(blocks)
}

fn deserialize_icons<'de, D>(deserializer: D) -> Result<HashMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    map_type!(Icons, String;
              s => Ok(Icons(icons::get_icons(s).ok_or(format!("cannot find icon set called '{}'", s))?)));

    deserializer.deserialize_any(MapType::<Icons, String>(PhantomData, PhantomData))
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::util::deserialize_file;
    use assert_fs::prelude::{FileWriteStr, PathChild};
    use assert_fs::TempDir;

    #[test]
    fn test_load_config_legacy() {
        let temp_dir = TempDir::new().unwrap();
        let config_file_path = temp_dir.child("status.toml");
        config_file_path
            .write_str(
                concat!(
                    "icons = \"awesome\"\n",
                    "theme = \"solarized-dark\"\n",
                    "[[block]]\n",
                    "block = \"load\"\n",
                    "interval = 1\n",
                    "format = \"{1m}\"",
                )
                .as_ref(),
            )
            .unwrap();
        let config: Result<Config, _> = deserialize_file(config_file_path.path());
        config.unwrap();
    }

    #[test]
    fn test_load_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_file_path = temp_dir.child("status.toml");
        config_file_path
            .write_str(
                concat!(
                    "icons = \"awesome\"\n",
                    "[theme]\n",
                    "name = \"solarized-dark\"\n",
                    "[[block]]\n",
                    "block = \"load\"\n",
                    "interval = 1\n",
                    "format = \"{1m}\"",
                )
                .as_ref(),
            )
            .unwrap();
        let config: Result<Config, _> = deserialize_file(config_file_path.path());
        config.unwrap();
    }
}
