pub mod ipc;

use evdev::KeyCode;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::str::FromStr;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Profile {
    pub name: String,

    #[serde(
        serialize_with = "serialize_keys",
        deserialize_with = "deserialize_keys"
    )]
    pub keys: Vec<KeyCode>,

    pub cps: Cps,
    #[serde(default)]
    pub toggle: bool,
    #[serde(default)]
    pub jitter: f32,

    #[serde(default = "default_hold_to_click")]
    pub hold_to_click: bool,

    #[serde(default = "default_target_button")]
    pub target_button: String,
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

fn default_target_button() -> String {
    "MOUSE_LEFT".to_string()
}

fn default_hold_to_click() -> bool {
    true
}

fn serialize_keys<S>(keys: &Vec<KeyCode>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let strs: Vec<String> = keys.iter().map(|k| format!("{k:?}")).collect();
    strs.serialize(s)
}

fn deserialize_keys<'de, D>(d: D) -> Result<Vec<KeyCode>, D::Error>
where
    D: Deserializer<'de>,
{
    let strs = Vec::<String>::deserialize(d)?;
    strs.into_iter()
        .map(|s| KeyCode::from_str(&s).map_err(serde::de::Error::custom))
        .collect()
}
