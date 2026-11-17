use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::ser::PrettyFormatter;
use serde_json::Serializer;

use crate::error::Error;

/// `json.dump(..., indent=4)` without a trailing newline.
pub fn write_json(path: &Path, value: &impl Serialize) -> Result<(), Error> {
    let mut buf = Vec::new();
    let formatter = PrettyFormatter::with_indent(b"    ");
    let mut serializer = Serializer::with_formatter(&mut buf, formatter);
    value
        .serialize(&mut serializer)
        .map_err(|error| Error::new(error.to_string()))?;
    fs::write(path, buf).map_err(|error| Error::new(format!("writing {}: {error}", path.display())))
}

pub fn read_json(path: &Path) -> Result<serde_json::Value, Error> {
    let text = fs::read_to_string(path)
        .map_err(|error| Error::new(format!("{}: {error}", path.display())))?;
    serde_json::from_str(&text).map_err(|error| Error::new(format!("{}: {error}", path.display())))
}
