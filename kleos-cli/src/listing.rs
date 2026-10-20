use serde_json::Value;

/// Keeps rows whose `is_superseded` is absent, null or false.
///
/// The CLI asks the server for `exclude_superseded=true`, but a server that
/// predates the parameter ignores it and returns retired rows, flagged `true`.
/// A flag of any other type is treated as retired. Against such a server the
/// page can come back shorter than `--limit`.
pub(crate) fn live_rows(items: Vec<Value>) -> Vec<Value> {
    items
        .into_iter()
        .filter(|item| {
            matches!(
                item.get("is_superseded"),
                None | Some(Value::Null) | Some(Value::Bool(false))
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rows_from_a_server_that_ignores_the_parameter_are_filtered() {
        let old_server_page = vec![
            json!({ "id": 5, "is_superseded": false }),
            json!({ "id": 4, "is_superseded": true }),
            json!({ "id": 3 }),
            json!({ "id": 2, "is_superseded": null }),
            json!({ "id": 1, "is_superseded": "yes" }),
        ];
        let kept: Vec<i64> = live_rows(old_server_page)
            .iter()
            .map(|r| r["id"].as_i64().unwrap())
            .collect();
        assert_eq!(kept, vec![5, 3, 2]);
    }
}
