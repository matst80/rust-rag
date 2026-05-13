
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(JsonSchema)]
pub struct Test {
    pub json_schema: Value,
}

fn main() {
    let schema = schema_for!(Test);
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
