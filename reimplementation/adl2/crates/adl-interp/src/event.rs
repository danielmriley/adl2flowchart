//! The event model (SPEC_LANGUAGE §4.1) and its JSONL deserialization.
//!
//! An **event** is: for each base collection (Jet, Electron, Muon, ...),
//! a finite *ordered* list of objects with real-valued properties; plus
//! event scalars (the MET vector → `MET.pt`/`MET.phi`, scalar `HT`, ...)
//! and trigger flags ∈ {0, 1}.
//!
//! One JSON object per line:
//!
//! ```json
//! {"Jet": [{"pt": 100.0, "eta": 1.2, "phi": 0.3, "m": 10.0, "btag": 1}],
//!  "MET": {"pt": 80.0, "phi": 0.4},
//!  "HT": 210.0,
//!  "triggers": {"mu_trig": 1}}
//! ```
//!
//! - array values are collections (keys canonicalized through the
//!   base-collection spelling map, case-insensitively);
//! - a MET-family key (`MET`, `MissingET`, ...) with an object value is
//!   the event MET vector; a bare number is its `pt` magnitude;
//! - the `weight` key (SPEC_EVENT_PIPELINE §4) is the event's input
//!   weight, a number; absent means 1.0;
//! - other numeric values are event scalars (`HT`, ...);
//! - the `triggers` key holds the trigger flags, each of which must be
//!   0 or 1.
//!
//! Per PHASE0, collections must arrive **pT-descending**; the loader
//! validates this and refuses unordered input — it never re-sorts.

use adl_sema::ExtDecls;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;

/// One object of a collection: a bag of real-valued properties, keyed by
/// the *canonical* property identity (same canonicalization the resolver
/// uses, so `pt`/`pT`/`Pt` in the input all land on one key).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EventObject {
    props: BTreeMap<String, f64>,
}

impl EventObject {
    /// Property value by canonical key (see [`ExtDecls::prop_canon`]).
    #[must_use]
    pub fn get(&self, canon_key: &str) -> Option<f64> {
        self.props.get(canon_key).copied()
    }

    /// All properties, in deterministic (sorted-key) order.
    pub fn properties(&self) -> impl Iterator<Item = (&str, f64)> {
        self.props.iter().map(|(k, &v)| (k.as_str(), v))
    }
}

/// A deserialized event record (SPEC_LANGUAGE §4.1).
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// Ordered object lists, keyed by lowercase canonical base name
    /// (`"jet"`, `"electron"`, ...). Absent collection = empty.
    pub collections: BTreeMap<String, Vec<EventObject>>,
    /// The event MET vector's components, keyed canonically (`pt`, `phi`).
    /// Empty when the record carries no MET.
    pub met: BTreeMap<String, f64>,
    /// Per-event scalars (`ht`, ...), keyed by lowercase name.
    pub scalars: BTreeMap<String, f64>,
    /// Trigger flags ∈ {0, 1}, keyed by lowercase name.
    pub triggers: BTreeMap<String, f64>,
    /// The input event weight (SPEC_EVENT_PIPELINE §4): the JSONL
    /// top-level `weight` key, or what an ingestion profile mapped there
    /// (Delphes `Event.Weight`, NanoAOD `genWeight`). Absent input = 1.0.
    /// Negative weights are legitimate (NLO generators).
    pub weight: f64,
}

impl Default for Event {
    fn default() -> Self {
        Event {
            collections: BTreeMap::new(),
            met: BTreeMap::new(),
            scalars: BTreeMap::new(),
            triggers: BTreeMap::new(),
            weight: 1.0,
        }
    }
}

/// Event-deserialization error. `line` is 1-based within the JSONL input.
#[derive(Debug, Clone, PartialEq)]
pub enum EventError {
    /// The line is not valid JSON.
    Json { line: usize, message: String },
    /// The JSON does not have the expected shape.
    Shape { line: usize, message: String },
    /// A collection is not pT-descending (re-sort is OFF per PHASE0:
    /// events must arrive ordered; the loader validates, never sorts).
    NotPtDescending {
        line: usize,
        collection: String,
        index: usize,
    },
    /// A trigger flag is not 0 or 1.
    BadTriggerFlag {
        line: usize,
        name: String,
        value: f64,
    },
}

impl fmt::Display for EventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventError::Json { line, message } => {
                write!(f, "line {line}: invalid JSON: {message}")
            }
            EventError::Shape { line, message } => write!(f, "line {line}: {message}"),
            EventError::NotPtDescending {
                line,
                collection,
                index,
            } => write!(
                f,
                "line {line}: collection `{collection}` is not pT-descending at index {index} \
                 (events must arrive ordered; re-sort is OFF)"
            ),
            EventError::BadTriggerFlag { line, name, value } => write!(
                f,
                "line {line}: trigger flag `{name}` is {value}; flags must be 0 or 1"
            ),
        }
    }
}

impl std::error::Error for EventError {}

/// Parse a single JSONL line into an [`Event`].
///
/// # Errors
/// Returns an [`EventError`] (reported as line 1) when the text is not a
/// well-shaped, pT-ordered event record.
pub fn parse_event(text: &str, ext: &ExtDecls) -> Result<Event, EventError> {
    event_from_line(1, text, ext)
}

/// Parse a whole JSONL document (one event per non-blank line).
///
/// # Errors
/// Returns the first [`EventError`] encountered, with its 1-based line.
pub fn read_jsonl(text: &str, ext: &ExtDecls) -> Result<Vec<Event>, EventError> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| event_from_line(i + 1, l, ext))
        .collect()
}

fn shape(line: usize, message: impl Into<String>) -> EventError {
    EventError::Shape {
        line,
        message: message.into(),
    }
}

fn event_from_line(line: usize, text: &str, ext: &ExtDecls) -> Result<Event, EventError> {
    let value: Value = serde_json::from_str(text).map_err(|e| EventError::Json {
        line,
        message: e.to_string(),
    })?;
    let Value::Object(map) = value else {
        return Err(shape(line, "event record must be a JSON object"));
    };

    let mut ev = Event::default();
    let mut saw_weight = false;
    for (key, val) in &map {
        let lk = key.to_ascii_lowercase();
        if lk == "triggers" {
            load_triggers(line, &mut ev, val)?;
        } else if lk == "weight" {
            let Some(w) = val.as_f64() else {
                return Err(shape(line, "`weight` must be a number"));
            };
            if saw_weight {
                return Err(shape(line, "duplicate `weight` key after case folding"));
            }
            saw_weight = true;
            ev.weight = w;
        } else if ext.is_met_family(key) {
            load_met(line, &mut ev, key, val, ext)?;
        } else if let Value::Array(items) = val {
            load_collection(line, &mut ev, key, items, ext)?;
        } else if let Some(n) = val.as_f64() {
            if ev.scalars.insert(lk.clone(), n).is_some() {
                return Err(shape(
                    line,
                    format!("duplicate event scalar `{lk}` after case folding"),
                ));
            }
        } else {
            return Err(shape(
                line,
                format!("key `{key}`: expected an object list, a number, or `triggers`"),
            ));
        }
    }

    validate_pt_descending(line, &ev, ext)?;
    Ok(ev)
}

fn load_triggers(line: usize, ev: &mut Event, val: &Value) -> Result<(), EventError> {
    let Value::Object(flags) = val else {
        return Err(shape(line, "`triggers` must be an object of 0/1 flags"));
    };
    for (name, fv) in flags {
        let Some(flag) = fv.as_f64() else {
            return Err(shape(
                line,
                format!("trigger flag `{name}` must be a number"),
            ));
        };
        if flag != 0.0 && flag != 1.0 {
            return Err(EventError::BadTriggerFlag {
                line,
                name: name.clone(),
                value: flag,
            });
        }
        let lk = name.to_ascii_lowercase();
        if ev.triggers.insert(lk.clone(), flag).is_some() {
            return Err(shape(
                line,
                format!("duplicate trigger flag `{lk}` after case folding"),
            ));
        }
    }
    Ok(())
}

fn load_met(
    line: usize,
    ev: &mut Event,
    key: &str,
    val: &Value,
    ext: &ExtDecls,
) -> Result<(), EventError> {
    if !ev.met.is_empty() {
        return Err(shape(line, format!("duplicate MET vector (key `{key}`)")));
    }
    match val {
        Value::Object(props) => {
            for (pk, pv) in props {
                let Some(n) = pv.as_f64() else {
                    return Err(shape(
                        line,
                        format!("MET component `{pk}` must be a number"),
                    ));
                };
                let (canon, _) = ext.prop_canon(pk);
                if ev.met.insert(canon, n).is_some() {
                    return Err(shape(
                        line,
                        format!("duplicate MET component `{pk}` after canonicalization"),
                    ));
                }
            }
        }
        _ => {
            let Some(n) = val.as_f64() else {
                return Err(shape(
                    line,
                    format!("MET (key `{key}`) must be an object or a number"),
                ));
            };
            let (pt_key, _) = ext.prop_canon("pt");
            ev.met.insert(pt_key, n);
        }
    }
    Ok(())
}

fn load_collection(
    line: usize,
    ev: &mut Event,
    key: &str,
    items: &[Value],
    ext: &ExtDecls,
) -> Result<(), EventError> {
    let ckey = ext
        .base_collection(key)
        .map_or_else(|| key.to_ascii_lowercase(), str::to_ascii_lowercase);
    let mut objs = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let Value::Object(props) = item else {
            return Err(shape(
                line,
                format!("collection `{key}`: element {i} must be a JSON object"),
            ));
        };
        let mut obj = EventObject::default();
        for (pk, pv) in props {
            let Some(n) = pv.as_f64() else {
                return Err(shape(
                    line,
                    format!("collection `{key}`: element {i}: property `{pk}` must be a number"),
                ));
            };
            let (canon, _) = ext.prop_canon(pk);
            if obj.props.insert(canon, n).is_some() {
                return Err(shape(
                    line,
                    format!(
                        "collection `{key}`: element {i}: duplicate property `{pk}` \
                         after canonicalization"
                    ),
                ));
            }
        }
        objs.push(obj);
    }
    if ev.collections.insert(ckey.clone(), objs).is_some() {
        return Err(shape(
            line,
            format!("duplicate collection `{ckey}` after canonicalization"),
        ));
    }
    Ok(())
}

/// PHASE0: collections arrive pT-descending; assert, never re-sort.
/// Objects without a `pt` property are exempt (and break the chain).
fn validate_pt_descending(line: usize, ev: &Event, ext: &ExtDecls) -> Result<(), EventError> {
    let (pt_key, _) = ext.prop_canon("pt");
    for (name, objs) in &ev.collections {
        let mut prev: Option<f64> = None;
        for (i, obj) in objs.iter().enumerate() {
            let Some(pt) = obj.get(&pt_key) else {
                prev = None;
                continue;
            };
            if let Some(p) = prev
                && pt > p
            {
                return Err(EventError::NotPtDescending {
                    line,
                    collection: name.clone(),
                    index: i,
                });
            }
            prev = Some(pt);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use adl_sema::ExtDecls;

    fn parse(text: &str) -> Result<Event, EventError> {
        parse_event(text, &ExtDecls::legacy())
    }

    #[test]
    fn weight_defaults_to_one() {
        let ev = parse(r#"{"Jet": []}"#).unwrap();
        assert_eq!(ev.weight, 1.0);
    }

    #[test]
    fn weight_key_is_read_case_insensitively_and_not_a_scalar() {
        let ev = parse(r#"{"Weight": -1.5, "HT": 10.0}"#).unwrap();
        assert_eq!(ev.weight, -1.5);
        assert!(!ev.scalars.contains_key("weight"));
        assert_eq!(ev.scalars.get("ht"), Some(&10.0));
    }

    #[test]
    fn non_numeric_weight_is_a_shape_error() {
        let err = parse(r#"{"weight": "heavy"}"#).unwrap_err();
        assert!(matches!(err, EventError::Shape { .. }), "{err}");
    }

    #[test]
    fn duplicate_weight_after_case_folding_errors() {
        let err = parse(r#"{"weight": 1.0, "Weight": 2.0}"#).unwrap_err();
        assert!(matches!(err, EventError::Shape { .. }), "{err}");
    }
}
