pub mod ipc;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Profile {
    #[serde(default)]
    pub keys: HashMap<String, KeyConfig>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct KeyConfig {
    #[serde(default)]
    pub cps: u32,
    #[serde(default)]
    pub toggle: bool,
    #[serde(default)]
    pub jitter: u32,
}
