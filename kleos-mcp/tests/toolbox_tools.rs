//! The toolbox tools must be advertised over MCP with schemas a model can fill
//! in blind. A tool absent from `DAILY_TOOL_NAMES` never reaches `tools/list`,
//! and a schema that shrugs (`additionalProperties: true`) tells the model
//! nothing about what to send.

use kleos_client::{find_by_name, resolve_tool_name};
use kleos_mcp::tools::registry;
use serde_json::Value;

/// Canonical name -> (HTTP path, fields the caller must supply).
const TOOLBOX_ROUTES: &[(&str, &str, &[&str])] = &[
    (
        "toolbox.index",
        "/toolbox/entries",
        &["key", "kind", "name", "summary"],
    ),
    ("toolbox.find", "/toolbox/find", &["query"]),
    ("toolbox.get", "/toolbox/entries/{id}", &["id"]),
    ("toolbox.list", "/toolbox/entries", &[]),
    ("toolbox.forget", "/toolbox/entries/{id}", &["id"]),
    ("toolbox.reindex", "/toolbox/reindex", &[]),
];

/// The subset a daily driver needs. Forget and reindex stay reachable over HTTP
/// and by explicit name, but do not clutter the model's tool list.
const ADVERTISED: &[&str] = &[
    "toolbox_index",
    "toolbox_find",
    "toolbox_get",
    "toolbox_list",
];

#[test]
fn every_toolbox_route_is_registered_and_dispatchable() {
    for (name, path, required) in TOOLBOX_ROUTES {
        let route = find_by_name(name).unwrap_or_else(|| panic!("{name} missing from ROUTES"));
        assert_eq!(route.path, *path, "{name} points at the wrong path");
        assert!(
            resolve_tool_name(&name.replace('.', "_")).is_some(),
            "{name} must resolve from its underscore form"
        );

        let schema: Value =
            serde_json::from_str(route.input_schema).expect("toolbox schema is valid JSON");
        assert_eq!(schema["type"], "object", "{name} schema must be an object");
        assert!(
            schema["additionalProperties"].is_null(),
            "{name} must declare its fields, not wave them through"
        );
        let properties = schema["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("{name} declares no properties"));
        for field in *required {
            assert!(
                properties.contains_key(*field),
                "{name} schema is missing the {field} property"
            );
            assert!(
                schema["required"]
                    .as_array()
                    .is_some_and(|r| r.iter().any(|v| v == field)),
                "{name} must mark {field} as required"
            );
        }
    }
}

#[test]
fn the_daily_toolbox_tools_are_advertised() {
    let names: Vec<String> = registry()
        .iter()
        .map(|t| t["name"].as_str().unwrap_or_default().to_string())
        .collect();
    for expected in ADVERTISED {
        assert!(
            names.iter().any(|n| n == expected),
            "{expected} is not in tools/list: the daily tool list is out of sync"
        );
    }
}
