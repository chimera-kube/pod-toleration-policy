use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Settings {
    pub taint: crate::Taint,

    #[serde(default)]
    pub allowed_groups: HashSet<String>,

    #[serde(default)]
    pub allowed_users: HashSet<String>,
}
