use std::fmt;
use yasec::Yasec;

#[derive(Yasec, Debug)]
pub(crate) struct Toleration {
    pub effect: String,
    pub key: String,
    pub operator: String,
}

impl fmt::Display for Toleration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "key: {}, operator: {}, effect: {})",
            self.key, self.operator, self.effect
        )
    }
}
