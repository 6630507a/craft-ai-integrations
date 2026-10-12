//! Prepared local SAM 3 image embedding for fast hover previews and exact selection masks.
//! The installed interactive node owns one PhotoCraft cache slot; each prepare forces new image state.
use crate::http::Http;

use base64::Engine;
use craft_ai_model::endpoint::Endpoint;

use craft_ai_model::{AiError, ErrorKind};
use serde_json::{Value, json};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod candidates;
mod instances;

const NODE: &str = "craft_ai_sam3";
const REQUIRED: &[&str] = &[
    "LoadImage",
    "LoadSAM3Model",
    "SAM3InteractiveCollector",
    "MaskToImage",
    "SaveImage",
];

/// Everything the installed SAM 3 model accepts in one geometric prompt.
pub mod sam3 {
    use serde_json::{Value, json};
    /// One geometric prompt: any mix of positive points, negative points and one box,
    /// all in pixels of the prepared image.
    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct Prompt {
        pub positive_points: Vec<(f32, f32)>,
        pub negative_points: Vec<(f32, f32)>,
        pub box_: Option<[f32; 4]>,
    }

    impl Prompt {
        pub fn point(x: f32, y: f32) -> Self {
            Self {
                positive_points: vec![(x, y)],
                ..Default::default()
            }
        }

        pub fn is_empty(&self) -> bool {
            self.positive_points.is_empty()
                && self.negative_points.is_empty()
                && self.box_.is_none()
        }

        pub(crate) fn store(&self, session: u128) -> Value {
            let points = |list: &[(f32, f32)]| {
                list.iter()
                    .map(|(x, y)| json!({"x": x, "y": y}))
                    .collect::<Vec<_>>()
            };
            json!([{
                "positive_points": points(&self.positive_points),
                "negative_points": points(&self.negative_points),
                "positive_boxes": match self.box_ {
                    Some([x1, y1, x2, y2]) => json!([{"x1": x1, "y1": y1, "x2": x2, "y2": y2}]),
                    None => json!([]),
                },
                "session": session,
            }])
        }
    }
}

struct Prepared {
    filename: String,
    width: u32,
    height: u32,
    nonce: u128,
    /// The server has the text grounding node (checked once, during `prepare`).
    grounding: bool,
    /// The server has `PreviewAny` and `StringConcatenate`, which carry stamped boxes and scores out.
    preview_text: bool,
    /// The server has `SAM3Segmentation` and `SAM3PointCollector` (point candidates).
    segmentation: bool,
    /// The server has the box helper nodes (`SAM3CreateBox`, `SAM3CombineBoxes`).
    boxes: bool,
}

/// A single local session. Calls block and must never run on the UI thread.
/// Starting a second session on the same server replaces the first session's hover cache.
pub struct Sam3Session {
    http: Http,
    prepared: Option<Prepared>,
}

impl Sam3Session {
    pub fn new(base: &str) -> Result<Self, AiError> {
        let endpoint =
            Endpoint::parse(base).map_err(|_| AiError::invalid("invalid ComfyUI address"))?;
        if !endpoint.is_loopback() || base != endpoint.fingerprint() {
            return Err(AiError::invalid(
                "SAM 3 needs a loopback ComfyUI address without a path",
            ));
        }
        Ok(Self {
            http: Http::new(
                base.to_owned(),
                Duration::from_secs(3),
                Duration::from_secs(15),
            ),
            prepared: None,
        })
    }

    /// Confirm that the loopback listener is ComfyUI rather than another local service.
    pub fn verify_server(&self) -> Result<(), AiError> {
        let stats = self.http.get_json("/system_stats")?;
        if stats
            .pointer("/system/comfyui_version")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            return Err(AiError::unsupported(
                "The local port is not serving ComfyUI",
            ));
        }
        Ok(())
    }

    /// Upload the PNG once and compute its image embedding as the tool activates.
    pub fn prepare(&mut self, image: &[u8]) -> Result<(), AiError> {
        self.prepared = None;
        if image.len() < 8
            || image.len() > 16 * 1024 * 1024
            || !image.starts_with(b"\x89PNG\r\n\x1a\n")
        {
            return Err(AiError::invalid("SAM 3 needs a PNG image under 16 MiB"));
        }
        let reader = png::Decoder::new(image)
            .read_info()
            .map_err(|_| AiError::invalid("SAM 3 needs a valid PNG image"))?;
        let (width, height) = (reader.info().width, reader.info().height);
        if width == 0
            || height == 0
            || width > 8192
            || height > 8192
            || u64::from(width) * u64::from(height) > 16_777_216
        {
            return Err(AiError::invalid(
                "SAM 3 source image dimensions are invalid",
            ));
        }
        let nodes = self.http.get_object_info()?;
        for class in REQUIRED {
            if nodes.get(*class).is_none() {
                return Err(AiError::unsupported(format!(
                    "ComfyUI is missing the {class} node"
                )));
            }
        }
        let filename = self
            .http
            .upload_image("sam3-interactive", "image/png", image)?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let prepared = Prepared {
            filename,
            width,
            height,
            nonce,
            grounding: nodes.get("SAM3Grounding").is_some(),
            boxes: nodes.get("SAM3CreateBox").is_some() && nodes.get("SAM3CombineBoxes").is_some(),
            segmentation: nodes.get("SAM3Segmentation").is_some()
                && nodes.get("SAM3PointCollector").is_some(),
            preview_text: nodes.get("PreviewAny").is_some()
                && nodes.get("StringConcatenate").is_some(),
        };
        let graph = self.graph(&prepared, json!([{"session":nonce}]), false);
        self.queue(graph)?;
        self.prepared = Some(prepared);
        Ok(())
    }

    /// Return a JPEG overlay for a pixel inside the prepared image, without queueing a graph.
    pub fn preview(&self, x: f32, y: f32) -> Result<Vec<u8>, AiError> {
        let prepared = self.point(x, y)?;
        let response = self.http.post(
            "/sam3/interactive_segment_one",
            &json!({
                "node_id": NODE, "prompt_index": 0, "prompt_name": "Object selection",
                "prompt": {"positive_points": [{"x": x, "y": y}]},
            }),
        )?;
        if response.get("num_masks").and_then(Value::as_u64) != Some(1) {
            return Err(AiError::new(
                ErrorKind::Provider,
                "SAM 3 preview returned no object",
            ));
        }
        let encoded = response
            .get("overlay")
            .and_then(Value::as_str)
            .filter(|s| s.len() <= 24 * 1024 * 1024)
            .ok_or_else(|| {
                AiError::new(ErrorKind::Provider, "SAM 3 preview is missing or too large")
            })?;
        let jpeg = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| AiError::new(ErrorKind::Provider, "SAM 3 preview is not base64"))?;
        if !jpeg.starts_with(&[0xff, 0xd8]) || !jpeg.ends_with(&[0xff, 0xd9]) {
            return Err(AiError::new(
                ErrorKind::Provider,
                "SAM 3 preview is not JPEG",
            ));
        }
        // Preserve the association with the image checked before this HTTP call.
        let _ = prepared;
        Ok(jpeg)
    }

    /// Save the exact grayscale mask through the installed graph, not the lossy hover JPEG.
    pub fn commit(&self, x: f32, y: f32) -> Result<Vec<u8>, AiError> {
        let prepared = self.point(x, y)?;
        let graph = self.graph(
            prepared,
            json!([{
                "positive_points": [{"x": x, "y": y}], "session": prepared.nonce
            }]),
            true,
        );
        let record = self.queue(graph)?;
        let node = record
            .pointer("/outputs/5")
            .ok_or_else(|| AiError::new(ErrorKind::Provider, "SAM 3 returned no selection mask"))?;
        crate::sam3_masks::fetch(&self.http, node, prepared.width, prepared.height)?.union_png()
    }

    /// Return a JPEG overlay for the full prompt, without queueing a graph.
    pub fn preview_prompt(&self, prompt: &sam3::Prompt) -> Result<Vec<u8>, AiError> {
        let prepared = self.prompt_bounds(prompt)?;
        let response = self.http.post(
            "/sam3/interactive_segment_one",
            &json!({
                "node_id": NODE, "prompt_index": 0, "prompt_name": "Object selection",
                "prompt": {
                    "positive_points": prompt.positive_points.iter().map(|(x, y)| json!({"x": x, "y": y})).collect::<Vec<_>>(),
                    "negative_points": prompt.negative_points.iter().map(|(x, y)| json!({"x": x, "y": y})).collect::<Vec<_>>(),
                    "positive_boxes": match prompt.box_ {
                        Some([x1, y1, x2, y2]) => json!([{"x1": x1, "y1": y1, "x2": x2, "y2": y2}]),
                        None => json!([]),
                    },
                },
            }),
        )?;
        if response.get("num_masks").and_then(Value::as_u64) != Some(1) {
            return Err(AiError::new(
                ErrorKind::Provider,
                "SAM 3 preview returned no object",
            ));
        }
        let encoded = response
            .get("overlay")
            .and_then(Value::as_str)
            .filter(|s| s.len() <= 24 * 1024 * 1024)
            .ok_or_else(|| {
                AiError::new(ErrorKind::Provider, "SAM 3 preview is missing or too large")
            })?;
        let jpeg = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| AiError::new(ErrorKind::Provider, "SAM 3 preview is not base64"))?;
        if !jpeg.starts_with(&[0xff, 0xd8]) || !jpeg.ends_with(&[0xff, 0xd9]) {
            return Err(AiError::new(
                ErrorKind::Provider,
                "SAM 3 preview is not JPEG",
            ));
        }
        // Preserve the association with the image checked before this HTTP call.
        let _ = prepared;
        Ok(jpeg)
    }

    /// Save the exact grayscale mask for a full geometric prompt.
    pub fn commit_prompt(&self, prompt: &sam3::Prompt) -> Result<Vec<u8>, AiError> {
        let prepared = self.prompt_bounds(prompt)?;
        let graph = self.graph(prepared, prompt.store(prepared.nonce), true);
        self.mask_from(graph)
    }

    /// Text grounding: segment everything matching the phrase, reusing the prepared image.
    pub fn ground(&self, text: &str) -> Result<Vec<u8>, AiError> {
        self.ground_masks(text)?.union_png()
    }

    /// Preserve every matching instance in server order; no partial batch is returned on failure.
    pub fn ground_masks(&self, text: &str) -> Result<crate::Sam3MaskBatch, AiError> {
        let (prepared, trimmed) = self.grounding_request(text)?;
        let masks = self.masks_from(instances::grounding_graph(
            &prepared.filename,
            trimmed,
            None,
        ))?;
        if !masks.has_pixels()? {
            return Err(AiError::new(
                ErrorKind::Provider,
                format!("Nothing matched \u{201c}{trimmed}\u{201d}"),
            ));
        }
        Ok(masks)
    }

    /// Try each phrase in order and return the first that matches something, with the phrase.
    /// The model only understands concrete nouns, so a generic "subject" needs several guesses.
    pub fn ground_first(&self, texts: &[&str]) -> Result<(String, Vec<u8>), AiError> {
        let mut last = AiError::invalid("SAM 3 needs at least one phrase");
        for text in texts {
            match self.ground(text) {
                Ok(mask) => return Ok(((*text).to_owned(), mask)),
                Err(e) if e.message.starts_with("Nothing matched") => last = e,
                Err(e) => return Err(e),
            }
        }
        Err(if texts.len() > 1 {
            AiError::new(
                ErrorKind::Provider,
                "No subject found. Type what to select, such as person or car",
            )
        } else {
            last
        })
    }

    fn mask_from(&self, graph: Value) -> Result<Vec<u8>, AiError> {
        self.masks_from(graph)?.union_png()
    }

    fn masks_from(&self, graph: Value) -> Result<crate::Sam3MaskBatch, AiError> {
        let prepared = self
            .prepared
            .as_ref()
            .ok_or_else(|| AiError::invalid("SAM 3 image not prepared"))?;
        let record = self.queue(graph)?;
        let node = record
            .pointer("/outputs/5")
            .ok_or_else(|| AiError::new(ErrorKind::Provider, "SAM 3 returned no selection mask"))?;
        crate::sam3_masks::fetch(&self.http, node, prepared.width, prepared.height)
    }

    /// Validate every point and the box against the prepared image.
    fn prompt_bounds(&self, prompt: &sam3::Prompt) -> Result<&Prepared, AiError> {
        let prepared = self
            .prepared
            .as_ref()
            .ok_or_else(|| AiError::invalid("SAM 3 image not prepared"))?;
        if prompt.is_empty() {
            return Err(AiError::invalid("SAM 3 prompt is empty"));
        }
        let inside = |x: f32, y: f32| {
            x.is_finite()
                && y.is_finite()
                && x >= 0.0
                && y >= 0.0
                && x < prepared.width as f32
                && y < prepared.height as f32
        };
        let mut all = prompt.positive_points.iter().chain(&prompt.negative_points);
        if !all.all(|(x, y)| inside(*x, *y)) {
            return Err(AiError::invalid(
                "SAM 3 prompt points must be inside the image",
            ));
        }
        if let Some([x1, y1, x2, y2]) = prompt.box_ {
            let (lo_x, hi_x) = (x1.min(x2), x1.max(x2));
            let (lo_y, hi_y) = (y1.min(y2), y1.max(y2));
            if !inside(lo_x, lo_y) || !inside(hi_x.max(lo_x), hi_y.max(lo_y)) {
                return Err(AiError::invalid(
                    "SAM 3 prompt box must be inside the image",
                ));
            }
            if hi_x - lo_x < 1.0 || hi_y - lo_y < 1.0 {
                return Err(AiError::invalid("SAM 3 prompt box is degenerate"));
            }
        }
        Ok(prepared)
    }

    fn point(&self, x: f32, y: f32) -> Result<&Prepared, AiError> {
        let p = self
            .prepared
            .as_ref()
            .ok_or_else(|| AiError::invalid("SAM 3 image not prepared"))?;
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x >= p.width as f32
            || y >= p.height as f32
        {
            return Err(AiError::invalid("SAM 3 point must be inside the image"));
        }
        Ok(p)
    }

    fn graph(&self, p: &Prepared, prompts: Value, mask: bool) -> Value {
        let mut graph = json!({
            "1":{"class_type":"LoadImage","inputs":{"image":p.filename}},
            "2":{"class_type":"LoadSAM3Model","inputs":{"precision":"auto","compile":false}},
            NODE:{"class_type":"SAM3InteractiveCollector","inputs":{
                "sam3_model_config":["2",0],"image":["1",0],
                "multi_prompts_store": prompts.to_string()
            }}
        });
        // serde_json's json! macro treats NODE as a literal key; the node ID above is stable.
        if mask {
            graph["4"] = json!({"class_type":"MaskToImage","inputs":{"mask":[NODE,0]}});
            graph["5"] = json!({"class_type":"SaveImage","inputs":{"images":["4",0],"filename_prefix":"craft-sam3-selection"}});
        }
        graph
    }

    fn queue(&self, mut graph: Value) -> Result<Value, AiError> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_OUTPUT: AtomicU64 = AtomicU64::new(0);
        let nodes = graph
            .as_object_mut()
            .ok_or_else(|| AiError::invalid("SAM 3 graph is invalid"))?;
        for node in nodes.values_mut() {
            if node.get("class_type").and_then(Value::as_str) != Some("SaveImage") {
                continue;
            }
            if let Some(prefix) = node.pointer_mut("/inputs/filename_prefix") {
                let name = prefix
                    .as_str()
                    .ok_or_else(|| AiError::invalid("SAM 3 output prefix is invalid"))?;
                let unique = format!(
                    "{name}-{}-{}",
                    std::process::id(),
                    NEXT_OUTPUT.fetch_add(1, Ordering::Relaxed)
                );
                *prefix = Value::String(unique);
            }
        }
        // A fully cached SaveImage can yield no history outputs on newer servers.
        // Change only the sink signature, keeping source/model/inference nodes cacheable.
        let response = self.http.post("/prompt", &json!({"prompt":graph}))?;
        let id = response
            .get("prompt_id")
            .and_then(Value::as_str)
            .filter(|s| {
                !s.is_empty()
                    && s.len() <= 128
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
            })
            .ok_or_else(|| {
                AiError::new(
                    ErrorKind::Provider,
                    "ComfyUI did not return a valid prompt id",
                )
            })?;
        let path = format!("/history/{id}");
        let deadline = Instant::now() + Duration::from_secs(240);
        loop {
            let history = self.http.get_json(&path)?;
            if let Some(record) = history.get(id) {
                if record.pointer("/status/status_str").and_then(Value::as_str) == Some("error") {
                    return Err(AiError::new(
                        ErrorKind::Provider,
                        "SAM 3 workflow failed in ComfyUI",
                    ));
                }
                if record.pointer("/status/completed").and_then(Value::as_bool) == Some(true) {
                    return Ok(record.clone());
                }
            }
            if Instant::now() >= deadline {
                return Err(AiError::new(
                    ErrorKind::Network,
                    "SAM 3 timed out in ComfyUI",
                ));
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    }
}
