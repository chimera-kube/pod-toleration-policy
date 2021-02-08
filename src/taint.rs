use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Deserialize, Serialize, Debug)]
pub(crate) struct Taint {
    pub key: String,
    pub value: String,
}

impl fmt::Display for Taint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "key: {}, value : {}", self.key, self.value)
    }
}
