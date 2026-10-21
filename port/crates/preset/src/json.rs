// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reads on top of [`sas_common::json`] that name the key in the error.

use sas_common::json::{JsonError, Value};

use crate::Error;

impl From<JsonError> for Error {
    fn from(error: JsonError) -> Error {
        Error::Corrupted(error.0)
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
    object
        .get(key)?
        .to_f32()
        .map_err(Error::from)
        .map_err(|error| with_key(key, error))
}

/// `int x = object[key];`.
pub(crate) fn read_i32(object: &Value, key: &str) -> Result<i32, Error> {
    object
        .get(key)?
        .to_i32()
        .map_err(Error::from)
        .map_err(|error| with_key(key, error))
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
    fn reads_convert_like_nlohmann_and_name_the_key() {
        let data = sas_common::json::parse(r#"{"on": true, "level": -2.9, "name": 1}"#).unwrap();
        assert_eq!(read_f32(&data, "on").unwrap(), 1.0);
        assert_eq!(read_i32(&data, "level").unwrap(), -2);
        let Err(Error::Corrupted(message)) = read_f32(&data, "missing") else {
            panic!("missing key read")
        };
        assert!(message.starts_with("\"missing\""), "{message}");
        assert!(read_string(&data, "name").is_err());
    }
}
