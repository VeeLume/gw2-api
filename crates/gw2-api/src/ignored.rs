//! Reporting of fields the models drop, for the CLI's `check` command.
//!
//! `serde_ignored` sees fields dropped by derived `Deserialize` impls, but not inside
//! hand-written ones that go through `serde_json::Value` (item details, tagged unions,
//! `Item` itself). Those parsers report here instead. Outside [`collect`] every call is
//! a cheap no-op, so the normal parsing path is unaffected.

use std::cell::RefCell;

use serde::de::DeserializeOwned;
use serde_json::Value;

thread_local! {
    static SINK: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// Run `f`, collecting every field reported as ignored while it runs.
pub fn collect<R>(f: impl FnOnce() -> R) -> (R, Vec<String>) {
    let previous = SINK.with(|s| s.borrow_mut().replace(Vec::new()));
    let result = f();
    let collected = SINK.with(|s| std::mem::replace(&mut *s.borrow_mut(), previous));
    (result, collected.unwrap_or_default())
}

/// Whether a [`collect`] is running on this thread.
pub fn active() -> bool {
    SINK.with(|s| s.borrow().is_some())
}

/// Record one ignored field path.
pub fn report(path: impl Into<String>) {
    SINK.with(|s| {
        if let Some(sink) = s.borrow_mut().as_mut() {
            sink.push(path.into());
        }
    });
}

/// `serde_json::from_value`, reporting fields `T` drops as `"{context}.{path}"`.
pub fn from_value<T: DeserializeOwned>(context: &str, v: Value) -> Result<T, serde_json::Error> {
    if !active() {
        return serde_json::from_value(v);
    }
    serde_ignored::deserialize(v, |path| report(format!("{context}.{path}")))
}

/// Report the keys of `object` that are not in `known` (and not `"type"`), as
/// `"{context}.{key}"`. For parsers that pick fields out of a JSON object by hand.
pub fn report_unknown_keys(context: &str, object: &Value, known: &[&str]) {
    if !active() {
        return;
    }
    if let Some(map) = object.as_object() {
        for key in map.keys() {
            if key != "type" && !known.contains(&key.as_str()) {
                report(format!("{context}.{key}"));
            }
        }
    }
}
