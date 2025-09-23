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

    pub cps: u32,
    #[serde(default)]
    pub toggle: bool,
    #[serde(default)]
    pub jitter: u32,
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
