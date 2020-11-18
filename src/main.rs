#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate yasec_derive;
extern crate yasec;
use yasec::Yasec;

use k8s_openapi::api::core::v1 as apicore;

use serde_json::Result;
use std::collections::HashSet;
use std::io::{self, Read};

mod config;
use config::Config;

mod toleration;
use toleration::Toleration;

#[derive(Serialize, Debug)]
struct ValidationResponse {
    accepted: bool,
    message: Option<String>,
}

fn main() -> std::result::Result<(), std::io::Error> {
    let mut data = String::new();
    io::stdin().read_to_string(&mut data)?;

    if let Ok(config) = Config::init() {
        let response: ValidationResponse = eval(&data, &config)?;
        println!("{}", serde_json::to_string(&response)?);
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "could not initialize config object",
        ));
    }

    Ok(())
}

fn eval(raw: &str, config: &Config) -> Result<ValidationResponse> {
    let request: serde_json::Value = serde_json::from_str(raw)?;

    match request.get("operation").and_then(|o| o.as_str()) {
        Some("CREATE") | Some("UPDATE") => {}
        _ => {
            return Ok(ValidationResponse {
                accepted: true,
                message: None,
            });
        }
    }

    let raw_object = serde_json::to_string(&request.get("object"))?;
    let pod: Result<apicore::Pod> = serde_json::from_str(&raw_object);
    if pod.is_err() {
        // Not a pod, the installed Kubernetes filter is forwarding
        // requests we cannot introspect, just accept them
        return Ok(ValidationResponse {
            accepted: true,
            message: None,
        });
    }
    let podspec = pod.unwrap().spec.unwrap();

    let toleration_found = podspec.tolerations.map_or(false, |tolerations| {
        tolerations.iter().any(|t| {
            t.key.as_deref() == Some(config.toleration.key.as_str())
                && t.operator.as_deref() == Some(config.toleration.operator.as_str())
                && t.effect.as_deref() == Some(config.toleration.effect.as_str())
        })
    });

    if !toleration_found {
        return Ok(ValidationResponse {
            accepted: true,
            message: None,
        });
    }

    // we can unwrap that, k8s always provides a value for it
    let req_user_info = request.get("userInfo").unwrap();

    // does the author of the request belong to one of the groups that are
    // entitled to create Pods with this toleration?
    let allowed_groups: HashSet<&str> = config.allowed_groups();
    if !allowed_groups.is_empty() {
        if let Some(groups) = req_user_info.get("groups").and_then(|g| g.as_array()) {
            let req_groups = groups
                .iter()
                .map(|g: &serde_json::Value| g.as_str().unwrap())
                .collect();
            let common_groups = allowed_groups.intersection(&req_groups);
            if common_groups.count() > 0 {
                return Ok(ValidationResponse {
                    accepted: true,
                    message: None,
                });
            }
        }
    }

    // does the author of the request happen to be also one of the users who are
    // entitled to create Pods with this toleration?
    let allowed_users: HashSet<&str> = config.allowed_users();
    if !allowed_users.is_empty() {
        if let Some(username) = req_user_info.get("username").and_then(|u| u.as_str()) {
            if allowed_users.contains(username) {
                return Ok(ValidationResponse {
                    accepted: true,
                    message: None,
                });
            }
        }
    }

    // The user is not entitled to create Pods with this toleration
    Ok(ValidationResponse {
        accepted: false,
        message: Some(format!(
            "User not allowed to create Pod objects with toleration: {}",
            config.toleration
        )),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    macro_rules! configuration {
        (key: $key:tt, operator: $operator:tt, allowed_users: $users:expr, allowed_groups: $groups:expr) => {
            Config {
                toleration: Toleration {
                    effect: String::from("NoSchedule"),
                    key: String::from($key),
                    operator: String::from($operator),
                },
                allowed_users: $users,
                allowed_groups: $groups,
            };
        };
    }

    #[test]
    fn allow_creation_because_of_matching_username() -> std::result::Result<(), std::io::Error> {
        let mut file = File::open("test_data/req_pod_with_toleration.json")?;
        let mut raw = String::new();
        file.read_to_string(&mut raw)?;

        let config = configuration!(key: "example-key", operator: "Exists", allowed_users: Some("admin,kubernetes-admin".into()), allowed_groups: None);
        let result = eval(&raw, &config)?;

        assert!(result.accepted);
        assert_eq!(result.message, None);

        Ok(())
    }

    #[test]
    fn allow_creation_because_of_matching_group() -> std::result::Result<(), std::io::Error> {
        let mut file = File::open("test_data/req_pod_with_toleration.json")?;
        let mut raw = String::new();
        file.read_to_string(&mut raw)?;

        let config = configuration!(key: "example-key", operator: "Exists", allowed_users: None, allowed_groups: Some("system:masters,my-admin-group".into()));
        let result = eval(&raw, &config)?;

        assert!(result.accepted);
        assert_eq!(result.message, None);

        Ok(())
    }

    #[test]
    fn allow_creation_because_taint_is_not_tracked_by_policy(
    ) -> std::result::Result<(), std::io::Error> {
        let mut file = File::open("test_data/req_pod_with_toleration.json")?;
        let mut raw = String::new();
        file.read_to_string(&mut raw)?;

        let config = configuration!(key: "another-key", operator: "Exists", allowed_users: Some("alice,bob".into()), allowed_groups: Some("power-users".into()));
        let result = eval(&raw, &config)?;

        assert!(result.accepted);
        assert_eq!(result.message, None);

        Ok(())
    }

    #[test]
    fn allow_creation_because_pod_does_not_have_tolerations(
    ) -> std::result::Result<(), std::io::Error> {
        let mut file = File::open("test_data/req_pod_without_toleration.json")?;
        let mut raw = String::new();
        file.read_to_string(&mut raw)?;

        let config = configuration!(key: "another-key", operator: "Exists", allowed_users: Some("alice,bob".into()), allowed_groups: Some("power-users".into()));
        let result = eval(&raw, &config)?;

        assert!(result.accepted);
        assert_eq!(result.message, None);

        Ok(())
    }

    #[test]
    fn reject_creation_because_of_not_allowed() -> std::result::Result<(), std::io::Error> {
        let mut file = File::open("test_data/req_pod_with_toleration.json")?;
        let mut raw = String::new();
        file.read_to_string(&mut raw)?;

        let config = configuration!(key: "example-key", operator: "Exists", allowed_users: Some("alice,bob".into()), allowed_groups: Some("power-users".into()));
        let result = eval(&raw, &config)?;

        assert!(!result.accepted);
        assert_eq!(
            result.message,
            Some("User not allowed to create Pod objects with toleration: key: example-key, operator: Exists, effect: NoSchedule)".into()),
        );

        Ok(())
    }

    #[test]
    fn accept_because_delete_operation() -> std::result::Result<(), std::io::Error> {
        let mut file = File::open("test_data/req_delete.json")?;
        let mut raw = String::new();
        file.read_to_string(&mut raw)?;

        let config = configuration!(key: "example-key", operator: "Exists", allowed_users: Some("alice,bob".into()), allowed_groups: Some("power-users".into()));
        let result = eval(&raw, &config)?;

        assert!(result.accepted);
        assert_eq!(result.message, None);

        Ok(())
    }
}
