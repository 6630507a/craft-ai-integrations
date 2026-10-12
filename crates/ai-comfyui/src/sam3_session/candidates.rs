//! Point-prompt candidates through `SAM3Segmentation`, which offers several alternative masks
//! for an ambiguous click. Unlike hover this recomputes image features on every call, so it is
//! for a deliberate action, not for pointer movement. Scores travel as stamped `PreviewAny`
//! text for the reason given in `instances.rs`.
use super::instances::{stamp, unstamped};
use super::{Sam3Session, sam3};
use crate::sam3_candidates::{Sam3Candidates, parse_scores};
use crate::sam3_masks::{bad, fetch};
use craft_ai_model::AiError;
use serde_json::{Value, json};

/// More points than any person places, and a bound on the graph text we send.
const MAX_POINTS: usize = 32;

fn coordinates(points: &[(f32, f32)]) -> String {
    Value::Array(
        points
            .iter()
            .map(|(x, y)| json!({"x": x, "y": y}))
            .collect(),
    )
    .to_string()
}

fn candidates_graph(filename: &str, prompt: &sam3::Prompt, stamp: &str) -> Value {
    json!({
        "1": {"class_type": "LoadImage", "inputs": {"image": filename}},
        "2": {"class_type": "LoadSAM3Model", "inputs": {"precision": "auto", "compile": false}},
        "10": {"class_type": "SAM3PointCollector", "inputs": {
            "image": ["1", 0], "points_store": "{}",
            "coordinates": coordinates(&prompt.positive_points),
            "neg_coordinates": coordinates(&prompt.negative_points)}},
        "3": {"class_type": "SAM3Segmentation", "inputs": {
            "sam3_model_config": ["2", 0], "image": ["1", 0],
            "positive_points": ["10", 0], "negative_points": ["10", 1],
            "refinement_iterations": 0, "use_multimask": true, "output_best_mask": false}},
        "4": {"class_type": "MaskToImage", "inputs": {"mask": ["3", 0]}},
        "5": {"class_type": "SaveImage", "inputs": {"images": ["4", 0], "filename_prefix": "craft-sam3-candidates"}},
        "6": {"class_type": "StringConcatenate", "inputs": {
            "string_a": stamp, "string_b": ["3", 4], "delimiter": "\n"}},
        "8": {"class_type": "PreviewAny", "inputs": {"source": ["6", 0]}},
    })
}

impl Sam3Session {
    /// Every alternative mask for a point prompt, in server order, never merged or chosen for
    /// the person. Boxes are not supported on this route yet. A missing node, an inconsistent
    /// or oversized result, or no object at all is an error, never a partial list.
    pub fn candidates(&self, prompt: &sam3::Prompt) -> Result<Sam3Candidates, AiError> {
        if prompt.box_.is_some() {
            return Err(AiError::unsupported(
                "SAM 3 candidates support points only for now",
            ));
        }
        let prepared = self.prompt_bounds(prompt)?;
        if prompt.positive_points.len() + prompt.negative_points.len() > MAX_POINTS {
            return Err(AiError::invalid("SAM 3 prompt has too many points"));
        }
        if !prepared.segmentation {
            return Err(AiError::unsupported(
                "ComfyUI is missing the SAM3Segmentation or SAM3PointCollector node",
            ));
        }
        if !prepared.preview_text {
            return Err(AiError::unsupported(
                "ComfyUI is missing the PreviewAny or StringConcatenate node, so SAM 3 scores are unavailable",
            ));
        }
        let stamp = stamp();
        let record = self.queue(candidates_graph(&prepared.filename, prompt, &stamp))?;
        let text = record
            .pointer("/outputs/8/text/0")
            .and_then(Value::as_str)
            .ok_or_else(|| bad("SAM 3 returned no candidate scores"))?;
        let scores = parse_scores(unstamped(text, &stamp)?)?;
        if scores.is_empty() {
            return Err(bad("SAM 3 returned no candidates"));
        }
        let node = record
            .pointer("/outputs/5")
            .ok_or_else(|| bad("SAM 3 returned no candidate masks"))?;
        let (width, height, masks) =
            fetch(&self.http, node, prepared.width, prepared.height)?.into_parts();
        Sam3Candidates::assemble(width, height, masks, scores)
    }
}
