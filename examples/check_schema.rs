
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn metadata_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let serde_json::Value::Object(map) = serde_json::json!({
        "type": "object",
        "additionalProperties": true,
        "description": "Free-form JSON object of string-keyed metadata.",
    }) else {
        unreachable!()
    };
    schemars::Schema::from(map)
}

#[derive(JsonSchema)]
pub struct Test {
    #[schemars(schema_with = "metadata_schema")]
    pub json_schema: Value,
}

fn main() {
    let schema = schema_for!(Test);
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
