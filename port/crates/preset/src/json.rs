// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! nlohmann::json conversions the loader needs on top of
//! [`sas_common::json`].
//!
//! Upstream reads numbers with implicit conversions (`float x = j;`), which
//! go through nlohmann 3.1.2's `from_json(const json&, ArithmeticType&)`.
//! Unlike `get_arithmetic_value`, that overload also accepts booleans.

use sas_common::json::{JsonError, Value};

use crate::Error;

impl From<JsonError> for Error {
    fn from(error: JsonError) -> Error {
        Error::Corrupted(error.0)
    }
}

/// `float x = value;`.
pub(crate) fn implicit_f32(value: &Value) -> Result<f32, Error> {
    match value {
        Value::Bool(flag) => Ok(f32::from(u8::from(*flag))),
        other => Ok(other.to_f32()?),
    }
}

/// `int x = value;`.
pub(crate) fn implicit_i32(value: &Value) -> Result<i32, Error> {
    match value {
        Value::Bool(flag) => Ok(i32::from(*flag)),
        other => Ok(other.to_i32()?),
    }
}

fn with_key(key: &str, error: Error) -> Error {
    match error {
        Error::Corrupted(detail) => Error::Corrupted(format!("\"{key}\": {detail}")),
        other => other,
    }
}

/// `float x = object[key];`.
pub(crate) fn read_f32(object: &Value, key: &str) -> Result<f32, Error> {
    implicit_f32(object.get(key)?).map_err(|error| with_key(key, error))
}

/// `int x = object[key];`.
pub(crate) fn read_i32(object: &Value, key: &str) -> Result<i32, Error> {
    implicit_i32(object.get(key)?).map_err(|error| with_key(key, error))
}

/// `std::string x = object[key];`.
pub(crate) fn read_string(object: &Value, key: &str) -> Result<String, Error> {
    let text = object
        .get(key)?
        .to_str()
        .map_err(|error| with_key(key, error.into()))?;
    Ok(text.to_string())
}

/// Checks that `value[key] = …` is legal (nlohmann throws on anything but
/// `null` and objects), so [`Value::set`] cannot panic.
pub(crate) fn ensure_object(value: &Value) -> Result<(), Error> {
    if value.is_null() || value.is_object() {
        Ok(())
    } else {
        Err(Error::Corrupted(format!(
            "cannot use operator[] with {}",
            value.type_name()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implicit_conversions_accept_booleans() {
        assert_eq!(implicit_f32(&Value::Bool(true)).unwrap(), 1.0);
        assert_eq!(implicit_i32(&Value::Float(-2.9)).unwrap(), -2);
        assert!(implicit_f32(&Value::Null).is_err());
        assert!(implicit_f32(&Value::from("1")).is_err());
    }
}
