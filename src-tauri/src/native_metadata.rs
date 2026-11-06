//! Preserve model capabilities across source routing and polluted client caches.
use serde_json::Value;

pub fn merge_reasoning(target: &mut Value, source: &Value) {
    if !source["supported_reasoning_levels"].as_array().is_some_and(|a| !a.is_empty()) {
        return;
    }
    for key in ["supported_reasoning_levels", "default_reasoning_level",
        "supports_reasoning_effort_updates", "supports_reasoning_summaries",
        "supports_reasoning_summary_parameter", "multi_agent_version", "multi_agent_reasoning_effort"] {
        if let Some(value) = source.get(key) { target[key] = value.clone(); }
    }
}

pub fn repair_empty_reasoning(entry: &mut Value) {
    if entry["supported_reasoning_levels"].as_array().is_some_and(|a| !a.is_empty()) { return; }
    let snapshot: Value = serde_json::from_str(include_str!("../../reference/native-reasoning.json"))
        .expect("validated native reasoning snapshot");
    // Exact, verified IDs only. Never invent capabilities for unknown models.
    if let Some(source) = snapshot["models"].as_array().unwrap().iter()
        .find(|m| m["slug"] == entry["slug"]) {
        merge_reasoning(entry, source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test] fn repairs_known_empty_lists_without_changing_luna_or_unknown() {
        for slug in ["gpt-6-astra", "gpt-6-sol", "gpt-6.1-sol"] {
            let mut row=json!({"slug":slug,"supported_reasoning_levels":[]});
            repair_empty_reasoning(&mut row);
            assert_eq!(row["supported_reasoning_levels"].as_array().unwrap().len(),6);
            assert_eq!(row["supported_reasoning_levels"][5]["effort"],"ultra");
        }
        for slug in ["gpt-6-luna","unknown"] {
            let mut row=json!({"slug":slug,"supported_reasoning_levels":[]});
            let before=row.clone();repair_empty_reasoning(&mut row);assert_eq!(row,before);
        }
        let mut row=json!({"slug":"gpt-6-sol","supported_reasoning_levels":[{"effort":"medium"}]});
        let before=row.clone();repair_empty_reasoning(&mut row);assert_eq!(row,before);
    }
    #[test] fn empty_relay_metadata_never_erases_native_capabilities() {
        let mut native=json!({"supported_reasoning_levels":[{"effort":"high"}],"default_reasoning_level":"high"});
        let before=native.clone();merge_reasoning(&mut native,&json!({"supported_reasoning_levels":[]}));
        assert_eq!(native,before);
        let mut relay=json!({"supported_reasoning_levels":[],"default_reasoning_level":"low"});
        merge_reasoning(&mut relay,&native);assert_eq!(relay,native);
    }
}
