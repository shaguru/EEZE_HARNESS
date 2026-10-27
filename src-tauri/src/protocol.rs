//! Stable subset generated from the companion 0.163.0 schemas; raw protocol stays in Rust.
use serde_json::{json, Value};
pub const CODEX_VERSION: &str = "codex-cli 0.163.0";
pub fn capabilities() -> Vec<String> {
    let schema: Value = serde_json::from_str(include_str!(
        "../../generated/json-schema/ClientRequest.json"
    ))
    .unwrap_or(json!({}));
    schema["oneOf"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|e| {
                    e["properties"]["method"]["enum"][0]
                        .as_str()
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}
pub fn approval_response(method: &str, decision: &str, params: &Value) -> Result<Value, String> {
    match method {
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval"
            if ["accept", "decline", "cancel"].contains(&decision) =>
        {
            Ok(json!({"decision":decision}))
        }
        "execCommandApproval" | "applyPatchApproval" => match decision {
            "accept" => Ok(json!({"decision":"approved"})),
            "decline" => Ok(
                json!({"decision":{"denied":{"rejection":"Declined by the user in RustPortableHarness"}}}),
            ),
            "cancel" => Ok(json!({"decision":"abort"})),
            _ => Err("Unsupported approval decision".into()),
        },
        "item/permissions/requestApproval" if decision == "accept" => {
            Ok(json!({"permissions":params["permissions"],"scope":"turn"}))
        }
        "item/permissions/requestApproval" if decision == "decline" => {
            Ok(json!({"permissions":{},"scope":"turn"}))
        }
        "mcpServer/elicitation/request" if decision == "decline" => Ok(json!({"action":"decline"})),
        _ => Err("Unsupported response. Cancel the turn to stop this operation.".into()),
    }
}
pub fn supports_approval(method: &str) -> bool {
    matches!(
        method,
        "item/commandExecution/requestApproval"
            | "item/fileChange/requestApproval"
            | "execCommandApproval"
            | "applyPatchApproval"
            | "item/permissions/requestApproval"
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decisions_use_exact_schemas() {
        assert_eq!(
            approval_response(
                "item/commandExecution/requestApproval",
                "accept",
                &json!({})
            )
            .unwrap(),
            json!({"decision":"accept"})
        );
        assert_eq!(
            approval_response("execCommandApproval", "cancel", &json!({})).unwrap()["decision"],
            "abort"
        );
        assert!(approval_response("unknown", "accept", &json!({})).is_err());
        assert!(approval_response(
            "item/fileChange/requestApproval",
            "acceptForSession",
            &json!({})
        )
        .is_err());
    }
    #[test]
    fn no_implicit_permission_grants() {
        assert_eq!(
            approval_response(
                "item/permissions/requestApproval",
                "decline",
                &json!({"permissions":{"network":{"enabled":true}}})
            )
            .unwrap()["permissions"],
            json!({})
        );
    }
}
