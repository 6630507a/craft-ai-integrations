//! Models and what each one can do.
//!
//! A provider offers models. A model advertises [`CapabilitySpec`]s: for each capability, which
//! inputs it accepts and how many of each (so "takes a mask" and "takes at most one reference"
//! are data, not guesses), how many outputs it can return, and whether it can stream, call tools
//! or return structured output. Requests are validated against the spec before anything is sent.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::capability::CapabilityId;
use crate::ids::{ModelId, open_enum};

open_enum!(
    /// What an input is for. Edits need typed inputs: a mask is not just another image.
    InputRole {
        Prompt => "prompt",
        System => "system",
        Source => "source",
        Mask => "mask",
        Reference => "reference",
        Context => "context",
    }
);

/// Optional abilities beyond the basic request and result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Features {
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub tools: bool,
    #[serde(default)]
    pub structured_output: bool,
}

/// What one model can do for one capability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySpec {
    pub capability: CapabilityId,
    /// Inputs the model accepts, with the most of each. An input role not listed is unsupported.
    pub accepts: BTreeMap<InputRole, u32>,
    /// Inputs that must be present.
    #[serde(default)]
    pub requires: Vec<InputRole>,
    /// Most outputs one request can return (variations count here).
    #[serde(default = "one")]
    pub max_outputs: u32,
    #[serde(default)]
    pub features: Features,
}

fn one() -> u32 {
    1
}

impl CapabilitySpec {
    /// A spec that accepts the given inputs (one of each) and requires none.
    pub fn new(capability: CapabilityId) -> Self {
        Self {
            capability,
            accepts: BTreeMap::new(),
            requires: Vec::new(),
            max_outputs: 1,
            features: Features::default(),
        }
    }

    pub fn accepting(mut self, role: InputRole, max: u32) -> Self {
        self.accepts.insert(role, max);
        self
    }

    pub fn requiring(mut self, role: InputRole) -> Self {
        if !self.requires.contains(&role) {
            self.requires.push(role);
        }
        self
    }

    pub fn max_outputs(mut self, n: u32) -> Self {
        self.max_outputs = n;
        self
    }

    pub fn with_features(mut self, features: Features) -> Self {
        self.features = features;
        self
    }
}

/// One model as offered by one connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: ModelId,
    pub label: String,
    #[serde(default)]
    pub capabilities: Vec<CapabilitySpec>,
}

impl ModelInfo {
    pub fn new(id: ModelId, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            capabilities: Vec::new(),
        }
    }

    pub fn with(mut self, spec: CapabilitySpec) -> Self {
        self.capabilities.push(spec);
        self
    }

    pub fn spec_for(&self, capability: &CapabilityId) -> Option<&CapabilitySpec> {
        self.capabilities
            .iter()
            .find(|s| &s.capability == capability)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap(s: &str) -> CapabilityId {
        CapabilityId::parse(s).unwrap()
    }

    #[test]
    fn input_roles_round_trip_including_custom() {
        for text in ["mask", "reference", "depth-map"] {
            let r = InputRole::parse(text).unwrap();
            assert_eq!(r.as_str(), text);
            assert_eq!(
                serde_json::from_str::<InputRole>(&serde_json::to_string(&r).unwrap()).unwrap(),
                r
            );
        }
        assert!(InputRole::parse("bad role").is_err());
    }

    #[test]
    fn spec_round_trips_with_map_keys() {
        let spec = CapabilitySpec::new(cap("image.inpaint"))
            .accepting(InputRole::Source, 1)
            .accepting(InputRole::Mask, 1)
            .accepting(InputRole::Reference, 3)
            .requiring(InputRole::Source)
            .max_outputs(4);
        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("\"reference\":3"));
        assert_eq!(serde_json::from_str::<CapabilitySpec>(&json).unwrap(), spec);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let spec: CapabilitySpec =
            serde_json::from_str(r#"{"capability":"text.complete","accepts":{"prompt":1}}"#)
                .unwrap();
        assert_eq!(spec.max_outputs, 1);
        assert!(!spec.features.streaming);
        assert!(spec.requires.is_empty());
    }

    #[test]
    fn model_finds_its_spec() {
        let m = ModelInfo::new(ModelId::new("m1").unwrap(), "M1")
            .with(CapabilitySpec::new(cap("image.generate")));
        assert!(m.spec_for(&cap("image.generate")).is_some());
        assert!(m.spec_for(&cap("image.inpaint")).is_none());
    }
}
