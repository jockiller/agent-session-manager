pub mod dsh;
pub mod grok;
pub mod omp;
pub mod opencode;
pub mod pi;
pub mod zcode;

use serde_json::{Map, Value};

pub(crate) fn json_object(value: Option<&Value>) -> Map<String, Value> {
    match value {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    }
}

pub(crate) fn take_object(root: &mut Map<String, Value>, key: &str) -> Map<String, Value> {
    match root.remove(key) {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

