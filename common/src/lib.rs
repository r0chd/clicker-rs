pub mod ipc;

use evdev::KeyCode;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::str::FromStr;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Profile {
    pub name: String,

    #[serde(
        serialize_with = "serialize_activation_keys",
        deserialize_with = "deserialize_activation_keys"
    )]
    pub activation_keys: Vec<KeyCode>,
    #[serde(
        serialize_with = "serialize_repeat_key",
        deserialize_with = "deserialize_repeat_key",
        default = "default_repeat_key"
    )]
    pub repeat_key: KeyCode,

    pub cps: Cps,
    #[serde(default = "default_toggle")]
    pub toggle: bool,
    #[serde(default)]
    pub jitter: f32,

    #[serde(default = "default_hold_to_click")]
    pub hold_to_click: bool,
}

fn default_toggle() -> bool {
    true
}

fn default_repeat_key() -> KeyCode {
    KeyCode::BTN_LEFT
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Cps {
    pub target: f32,
    #[serde(default = "default_std_dev")]
    pub std_dev: f32,
}

fn default_std_dev() -> f32 {
    1.5
}

fn default_hold_to_click() -> bool {
    true
}

fn serialize_repeat_key<S>(key: &KeyCode, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    format!("{key:?}").serialize(s)
}

fn deserialize_repeat_key<'de, D>(d: D) -> Result<KeyCode, D::Error>
where
    D: Deserializer<'de>,
{
    let str = String::deserialize(d)?;
    KeyCode::from_str(&str).map_err(serde::de::Error::custom)
}

fn serialize_activation_keys<S>(keys: &Vec<KeyCode>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let strs: Vec<String> = keys.iter().map(|k| format!("{k:?}")).collect();
    strs.serialize(s)
}

fn deserialize_activation_keys<'de, D>(d: D) -> Result<Vec<KeyCode>, D::Error>
where
    D: Deserializer<'de>,
{
    let strs = Vec::<String>::deserialize(d)?;
    strs.into_iter()
        .map(|s| KeyCode::from_str(&s).map_err(serde::de::Error::custom))
        .collect()
}
