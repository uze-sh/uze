//! JSON in and out of the engine. The input, the context and the envelope
//! cross as `serde_json::Value`; inside, they are Rhai's own values.
//!
//! Both directions are bounded in depth by hand: a script can nest an
//! array inside itself as often as its size limit allows, and a recursive
//! conversion without a floor would turn that into a stack overflow on the
//! pool thread.

use rhai::{Array, Dynamic, FnPtr, Map};
use serde_json::{Map as JsonMap, Number, Value};

/// How deep a value may nest on its way in or out; `serde_json`'s own
/// parser stops at 128.
pub(crate) const MAX_DEPTH: usize = 128;

/// Why a value could not cross.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Crossing {
    TooDeep,
    NotJson(String),
}

impl std::fmt::Display for Crossing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooDeep => write!(f, "nested deeper than {MAX_DEPTH} levels"),
            Self::NotJson(type_name) => write!(f, "a `{type_name}` has no JSON form"),
        }
    }
}

pub(crate) fn to_dynamic(value: &Value) -> Result<Dynamic, Crossing> {
    to_dynamic_at(value, 0)
}

fn to_dynamic_at(value: &Value, depth: usize) -> Result<Dynamic, Crossing> {
    if depth > MAX_DEPTH {
        return Err(Crossing::TooDeep);
    }
    Ok(match value {
        Value::Null => Dynamic::UNIT,
        Value::Bool(flag) => Dynamic::from_bool(*flag),
        Value::Number(number) => number_to_dynamic(number),
        Value::String(text) => Dynamic::from(text.clone()),
        Value::Array(items) => {
            let array: Array = items
                .iter()
                .map(|item| to_dynamic_at(item, depth + 1))
                .collect::<Result<_, _>>()?;
            Dynamic::from_array(array)
        }
        Value::Object(fields) => {
            let mut map = Map::new();
            for (key, field) in fields {
                map.insert(key.as_str().into(), to_dynamic_at(field, depth + 1)?);
            }
            Dynamic::from_map(map)
        }
    })
}

fn number_to_dynamic(number: &Number) -> Dynamic {
    match number.as_i64() {
        Some(integer) => Dynamic::from_int(integer),
        None => Dynamic::from_float(number.as_f64().unwrap_or(f64::NAN)),
    }
}

pub(crate) fn to_json(value: &Dynamic) -> Result<Value, Crossing> {
    to_json_at(value, 0)
}

fn to_json_at(value: &Dynamic, depth: usize) -> Result<Value, Crossing> {
    if depth > MAX_DEPTH {
        return Err(Crossing::TooDeep);
    }
    if value.is_unit() {
        return Ok(Value::Null);
    }
    if let Ok(flag) = value.as_bool() {
        return Ok(Value::Bool(flag));
    }
    if let Ok(integer) = value.as_int() {
        return Ok(Value::from(integer));
    }
    if let Ok(float) = value.as_float() {
        return Number::from_f64(float)
            .map(Value::Number)
            .ok_or_else(|| Crossing::NotJson("non-finite number".to_owned()));
    }
    if let Ok(character) = value.as_char() {
        return Ok(Value::String(character.to_string()));
    }
    if value.is_string() {
        return Ok(Value::String(
            value.clone().into_string().unwrap_or_default(),
        ));
    }
    if let Some(array) = value.read_lock::<Array>() {
        return array
            .iter()
            .map(|item| to_json_at(item, depth + 1))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array);
    }
    if let Some(map) = value.read_lock::<Map>() {
        let mut object = JsonMap::new();
        for (key, field) in map.iter() {
            object.insert(key.to_string(), to_json_at(field, depth + 1)?);
        }
        return Ok(Value::Object(object));
    }
    Err(Crossing::NotJson(value.type_name().to_owned()))
}

/// Whether `value` nests arrays, maps and closures more than `levels`
/// deep: walked no further than `levels + 1`, so a value nested thousands
/// of levels is judged in as many frames as the bound, not as the value. A
/// closure holds what it captured as a container holds its items, and
/// Rhai's size limits do not look inside it.
pub(crate) fn deeper_than(value: &Dynamic, levels: usize) -> bool {
    if let Some(array) = value.read_lock::<Array>() {
        return levels == 0 || array.iter().any(|item| deeper_than(item, levels - 1));
    }
    if let Some(map) = value.read_lock::<Map>() {
        return levels == 0 || map.values().any(|item| deeper_than(item, levels - 1));
    }
    if let Some(function) = value.read_lock::<FnPtr>() {
        return function.is_curried()
            && (levels == 0
                || function
                    .iter_curry()
                    .any(|item| deeper_than(item, levels - 1)));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn depth_counts_the_containers_a_value_nests() {
        let mut value = Dynamic::from_int(1);
        assert!(!deeper_than(&value, 0));
        for _ in 0..3 {
            value = Dynamic::from_array(vec![value]);
        }
        let mut map = Map::new();
        map.insert("x".into(), value);
        let value = Dynamic::from_map(map);
        assert!(deeper_than(&value, 3));
        assert!(!deeper_than(&value, 4));
    }

    #[test]
    fn a_value_crosses_both_ways_unchanged() {
        let value = json!({"a": [1, 2.5, "x", null, true], "b": {"c": -3}});
        assert_eq!(to_json(&to_dynamic(&value).unwrap()).unwrap(), value);
    }

    #[test]
    fn a_value_nested_past_the_floor_is_refused() {
        let mut value = json!(0);
        for _ in 0..=MAX_DEPTH + 1 {
            value = json!([value]);
        }
        assert!(matches!(to_dynamic(&value), Err(Crossing::TooDeep)));
    }
}
