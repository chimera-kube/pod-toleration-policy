use std::collections::HashSet;
use yasec::Yasec;

#[derive(Yasec, Debug)]
pub(crate) struct Config {
    pub toleration: crate::Toleration,
    pub allowed_groups: Option<String>,
    pub allowed_users: Option<String>,
}

impl Config {
    pub(crate) fn allowed_groups(&self) -> HashSet<&str> {
        self.allowed_groups
            .as_ref()
            .map_or_else(HashSet::new, |ag| ag.split(',').collect())
    }

    pub(crate) fn allowed_users(&self) -> HashSet<&str> {
        self.allowed_users
            .as_ref()
            .map_or_else(HashSet::new, |au| au.split(',').collect())
    }
}
