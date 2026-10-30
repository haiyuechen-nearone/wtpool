use super::*;

/// `value` without the annotations a schema may word freely, with the arrays whose order
/// means nothing (`required`, `enum` and `type`) sorted.
fn without_annotations(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(key, _)| {
                    !matches!(key.as_str(), "title" | "description" | "$id" | "$comment")
                })
                .map(|(key, value)| {
                    let mut value = without_annotations(value);
                    if let Value::Array(items) = &mut value
                        && matches!(key.as_str(), "required" | "enum" | "type")
                    {
                        items.sort_by_key(Value::to_string);
                    }
                    (key.clone(), value)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(without_annotations).collect()),
        other => other.clone(),
    }
}

/// Pushes every `properties` map in `value` to `found`, with its JSON pointer below `at`.
fn property_maps<'a>(value: &'a Value, at: &str, found: &mut Vec<(String, &'a Value)>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let path = format!("{at}/{key}");
                if key == "properties" {
                    found.push((path.clone(), child));
                }
                property_maps(child, &path, found);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                property_maps(child, &format!("{at}/{index}"), found);
            }
        }
        _ => {}
    }
}

/// The oracle's phase 4 schema, with the optional actor properties of phase 5a.
fn must_oracle_schema() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../verify/schema/status.phase-4.schema.json"
    );
    let mut schema: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for name in ["model", "launched_by"] {
        schema["$defs"]["actor"]["properties"][name] = json!({ "type": "string", "minLength": 1 });
    }
    schema
}

#[test]
fn status_schema__should_match_the_document_paragraph_of_the_spec() {
    // Given
    let oracle = must_oracle_schema();

    // When
    let generated = status_schema();

    // Then
    assert_eq!(
        without_annotations(&generated),
        without_annotations(&oracle)
    );
}

#[test]
fn status_schema__should_describe_every_property_in_one_line() {
    // Given
    let schema = status_schema();
    let mut maps = Vec::new();

    // When
    property_maps(&schema, "#", &mut maps);

    // Then
    assert_eq!(maps.len(), 15, "{maps:?}");
    for (at, properties) in maps {
        for (name, property) in properties.as_object().unwrap() {
            let description = property["description"].as_str().unwrap_or_default();
            assert!(
                !description.is_empty() && !description.contains('\n'),
                "{at}/{name}: {description:?}"
            );
        }
    }
}

#[test]
fn status_schema__should_leave_every_object_open() {
    // When
    let text = status_schema_text();

    // Then
    assert!(!text.contains("additionalProperties"), "{text}");
    let pool = &status_schema()["$defs"]["pool"];
    assert_eq!(
        pool["required"].as_array().unwrap().last().unwrap(),
        "repository"
    );
    assert_eq!(
        without_annotations(&pool["properties"]["repository"]),
        json!({
            "type": "object",
            "required": ["github", "name"],
            "properties": {
                "name": { "type": "string", "minLength": 1 },
                "github": { "type": ["null", "string"] },
            },
        })
    );
    let actor = &status_schema()["$defs"]["actor"]["properties"]["harness"];
    assert_eq!(
        without_annotations(actor),
        json!({ "type": "string", "minLength": 1 })
    );
}

#[test]
fn status_schema_text__should_be_the_pretty_schema_with_a_final_newline() {
    // When
    let text = status_schema_text();

    // Then
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed, status_schema());
    assert!(
        text.ends_with("}\n") && text.starts_with("{\n  \"$defs\""),
        "{text}"
    );
    assert!(status_schema()["title"].is_string() && status_schema()["$id"].is_string());
}

#[test]
fn definitions__should_merge_every_job_definition() {
    // When
    let definitions = definitions();
    let jobs = jobs::definitions();

    // Then
    assert_eq!(jobs.len(), 4);
    for (name, schema) in jobs {
        assert_eq!(definitions[name], schema, "{name}");
    }
    let added = definitions.as_object().map_or(0, |map| map.len());
    assert!(added > 4, "{added}");
}
