extern crate wapc_guest as guest;
use guest::prelude::*;

use anyhow::anyhow;
use k8s_openapi::api::core::v1 as apicore;
use serde::{Deserialize, Serialize};
use serde_json::Result;

mod settings;
use settings::Settings;

mod taint;
use taint::Taint;

#[derive(Deserialize, Serialize, Debug)]
struct ValidationResponse {
    accepted: bool,
    message: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ValidationRequest {
    settings: Settings,
    request: serde_json::Value,
}

#[no_mangle]
pub extern "C" fn wapc_init() {
    register_function("validate", validate);
}

fn validate(payload: &[u8]) -> CallResult {
    let validation_req: ValidationRequest = match serde_json::from_slice(payload) {
        Ok(r) => r,
        Err(e) => {
            return Err(anyhow!(
                "Error decoding validation payload {}: {:?}",
                String::from_utf8_lossy(payload),
                e
            )
            .into());
        }
    };

    match validation_req
        .request
        .get("operation")
        .and_then(|o| o.as_str())
    {
        Some("CREATE") | Some("UPDATE") => {}
        _ => {
            return Ok(serde_json::to_vec(&ValidationResponse {
                accepted: true,
                message: None,
            })?);
        }
    }

    let raw_object = serde_json::to_string(&validation_req.request.get("object"))?;
    let pod: Result<apicore::Pod> = serde_json::from_str(&raw_object);
    if pod.is_err() {
        // Not a pod, the installed Kubernetes filter is forwarding
        // requests we cannot introspect, just accept them
        return Ok(serde_json::to_vec(&ValidationResponse {
            accepted: true,
            message: None,
        })?);
    }
    let podspec = pod.unwrap().spec.unwrap();

    let mut toleration_found = false;
    if let Some(tolerations) = podspec.tolerations {
        for toleration in tolerations.iter() {
            if toleration.key.as_deref() != Some(validation_req.settings.taint.key.as_str()) {
                continue;
            }
            // the toleration has the same key as the protected taint
            if toleration.operator.as_deref() == Some("Exists") {
                return Ok(serde_json::to_vec(&ValidationResponse {
                    accepted: false,
                    message: Some(format!(
                        "Nobody can use the protected taint '{}' with the operation 'Exists'",
                        validation_req.settings.taint.key
                    )),
                })?);
            }
            // it means the toleration operator is `Equal`
            if toleration.value.as_deref() == Some(validation_req.settings.taint.value.as_str()) {
                toleration_found = true;
                break;
            }
        }
    }

    if !toleration_found {
        return Ok(serde_json::to_vec(&ValidationResponse {
            accepted: true,
            message: None,
        })?);
    }

    // we can unwrap that, k8s always provides a value for it
    let req_user_info = validation_req.request.get("userInfo").unwrap();

    // does the author of the request belong to one of the groups that are
    // entitled to create Pods with this toleration?
    if !validation_req.settings.allowed_groups.is_empty() {
        if let Some(groups) = req_user_info.get("groups").and_then(|g| g.as_array()) {
            let req_groups = groups
                .iter()
                .map(|g: &serde_json::Value| String::from(g.as_str().unwrap()))
                .collect();
            let common_groups = validation_req
                .settings
                .allowed_groups
                .intersection(&req_groups);
            if common_groups.count() > 0 {
                return Ok(serde_json::to_vec(&ValidationResponse {
                    accepted: true,
                    message: None,
                })?);
            }
        }
    }

    // does the author of the request happen to be also one of the users who are
    // entitled to create Pods with this toleration?
    if !validation_req.settings.allowed_users.is_empty() {
        if let Some(username) = req_user_info.get("username").and_then(|u| u.as_str()) {
            if validation_req.settings.allowed_users.contains(username) {
                return Ok(serde_json::to_vec(&ValidationResponse {
                    accepted: true,
                    message: None,
                })?);
            }
        }
    }

    // The user is not entitled to create Pods with this toleration
    Ok(serde_json::to_vec(&ValidationResponse {
        accepted: false,
        message: Some(format!(
            "User not allowed to create Pods that tolerate the taint {}",
            validation_req.settings.taint
        )),
    })?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use std::fs::File;
    use std::io::BufReader;

    macro_rules! configuration {
        (key: $key:tt, value: $value:tt, allowed_users: $users:expr, allowed_groups: $groups:expr) => {
            Settings {
                taint: Taint {
                    key: String::from($key),
                    value: String::from($value),
                },
                allowed_users: $users.split(",").map(String::from).collect(),
                allowed_groups: $groups.split(",").map(String::from).collect(),
            };
        };
    }

    fn read_request_file(path: &str) -> Result<serde_json::Value> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let v = serde_json::from_reader(reader)?;

        Ok(v)
    }

    #[test]
    //fn allow_creation_because_of_matching_username() -> std::result::Result<(), std::io::Error> {
    fn allow_creation_because_of_matching_username() -> Result<()> {
        let vr = ValidationRequest {
            request: read_request_file("test_data/req_pod_with_equal_toleration.json")?,
            settings: configuration!(key: "dedicated", value: "tenantA", allowed_users: "admin,kubernetes-admin", allowed_groups: ""),
        };
        let payload = serde_json::to_vec(&vr)?;

        let raw_result = validate(&payload).unwrap();
        let result: ValidationResponse = serde_json::from_slice(&raw_result)?;

        assert!(result.accepted);
        assert_eq!(result.message, None);

        Ok(())
    }

    #[test]
    fn allow_creation_because_of_matching_group() -> Result<()> {
        let vr = ValidationRequest {
            request: read_request_file("test_data/req_pod_with_equal_toleration.json")?,
            settings: configuration!(key: "example-key", value: "tenantA", allowed_users: "", allowed_groups: "system:masters,my-admin-group"),
        };
        let payload = serde_json::to_vec(&vr)?;

        let raw_result = validate(&payload).unwrap();
        let result: ValidationResponse = serde_json::from_slice(&raw_result)?;

        assert!(result.accepted);
        assert_eq!(result.message, None);

        Ok(())
    }

    #[test]
    fn allow_creation_because_taint_is_not_tracked_by_policy() -> Result<()> {
        let vr = ValidationRequest {
            request: read_request_file("test_data/req_pod_with_equal_toleration.json")?,
            settings: configuration!(key: "another-key", value: "another-value", allowed_users: "alice,bob", allowed_groups: "power-users"),
        };
        let payload = serde_json::to_vec(&vr)?;

        let raw_result = validate(&payload).unwrap();
        let result: ValidationResponse = serde_json::from_slice(&raw_result)?;

        assert!(result.accepted);
        assert_eq!(result.message, None);

        Ok(())
    }

    #[test]
    fn allow_creation_because_pod_does_not_have_tolerations() -> Result<()> {
        let vr = ValidationRequest {
            request: read_request_file("test_data/req_pod_without_toleration.json")?,
            settings: configuration!(key: "dedicated", value: "tenantA", allowed_users: "alice,bob", allowed_groups: "power-users"),
        };
        let payload = serde_json::to_vec(&vr)?;

        let raw_result = validate(&payload).unwrap();
        let result: ValidationResponse = serde_json::from_slice(&raw_result)?;

        assert!(result.accepted);
        assert_eq!(result.message, None);

        Ok(())
    }

    #[test]
    fn reject_creation_because_of_not_allowed() -> Result<()> {
        let vr = ValidationRequest {
            request: read_request_file("test_data/req_pod_with_equal_toleration.json")?,
            settings: configuration!(key: "dedicated", value: "tenantA", allowed_users: "alice,bob", allowed_groups: "power-users"),
        };
        let payload = serde_json::to_vec(&vr)?;

        let raw_result = validate(&payload).unwrap();
        let result: ValidationResponse = serde_json::from_slice(&raw_result)?;

        assert!(!result.accepted);
        assert_eq!(
            result.message,
            Some("User not allowed to create Pods that tolerate the taint key: dedicated, value : tenantA".into()),
        );

        Ok(())
    }

    #[test]
    fn reject_creation_because_nobody_can_use_the_exists_toleration() -> Result<()> {
        let mut vr = ValidationRequest {
            request: read_request_file("test_data/req_pod_with_exists_toleration.json")?,
            settings: configuration!(key: "dedicated", value: "tenantA", allowed_users: "alice,bob", allowed_groups: "tenantB"),
        };
        let payload = serde_json::to_vec(&vr)?;

        let mut raw_result = validate(&payload).unwrap();
        let mut result: ValidationResponse = serde_json::from_slice(&raw_result)?;

        assert!(!result.accepted);
        assert_eq!(
            result.message,
            Some(
                "Nobody can use the protected taint \'dedicated\' with the operation \'Exists\'"
                    .into()
            ),
        );

        vr.settings = configuration!(key: "dedicated", value: "tenantA", allowed_users: "alice,bob", allowed_groups: "system:masters");
        raw_result = validate(&payload).unwrap();
        result = serde_json::from_slice(&raw_result)?;

        assert!(!result.accepted);
        assert_eq!(
            result.message,
            Some(
                "Nobody can use the protected taint \'dedicated\' with the operation \'Exists\'"
                    .into()
            ),
        );

        Ok(())
    }

    #[test]
    fn accept_because_delete_operation() -> Result<()> {
        let vr = ValidationRequest {
            request: read_request_file("test_data/req_delete.json")?,
            settings: configuration!(key: "dedicated", value: "tenantA", allowed_users: "alice,bob", allowed_groups: "power-users"),
        };
        let payload = serde_json::to_vec(&vr)?;

        let raw_result = validate(&payload).unwrap();
        let result: ValidationResponse = serde_json::from_slice(&raw_result)?;

        assert!(result.accepted);
        assert_eq!(result.message, None);

        Ok(())
    }

    #[test]
    fn accept_because_not_a_pod_related_operation() -> Result<()> {
        let vr = ValidationRequest {
            request: read_request_file("test_data/req_not_a_pod.json")?,
            settings: configuration!(key: "dedicated", value: "tenantA", allowed_users: "alice,bob", allowed_groups: "power-users"),
        };
        let payload = serde_json::to_vec(&vr)?;

        let raw_result = validate(&payload).unwrap();
        let result: ValidationResponse = serde_json::from_slice(&raw_result)?;

        assert!(result.accepted);
        assert_eq!(result.message, None);

        Ok(())
    }
}
