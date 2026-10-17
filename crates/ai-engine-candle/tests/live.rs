//! The real model, through the real provider and the shared runner checks.
//!
//! Needs Meta's checkpoint, so it skips (and says so) unless `CRAFT_SAM3_WEIGHTS` names the
//! `model.safetensors` that sits beside `vocab.json` and `merges.txt`. Run it in release mode:
//! `CRAFT_SAM3_WEIGHTS=<path> cargo test -p craft-ai-engine-candle --release --test live`.

use std::path::PathBuf;
use std::sync::Arc;

use craft_ai_core::{CapabilityId, ErrorKind, InputRole, Payload, Request};
use std::time::Duration;

use craft_ai_engine::testing::check_runner_paced;
use craft_ai_engine_candle::Sam3Runner;

/// A small picture with a bright disc on a dark ground.
fn picture() -> Vec<u8> {
    let (w, h) = (192u32, 128u32);
    let mut rgb = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let inside = (x as i32 - 96).pow(2) + (y as i32 - 64).pow(2) < 40 * 40;
            let v = if inside { 230 } else { 30 };
            rgb.extend_from_slice(&[v, v, v]);
        }
    }
    let mut out = Vec::new();
    let mut e = png::Encoder::new(&mut out, w, h);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    let mut writer = e.write_header().unwrap();
    writer.write_image_data(&rgb).unwrap();
    writer.finish().unwrap();
    out
}

#[test]
fn the_real_model_passes_the_shared_runner_checks() {
    let Some(weights) = std::env::var_os("CRAFT_SAM3_WEIGHTS").map(PathBuf::from) else {
        eprintln!("skipped: set CRAFT_SAM3_WEIGHTS to run the real model");
        return;
    };
    let capability = CapabilityId::parse("image.segment").unwrap();
    let source = Payload::inline("image/png", picture()).unwrap();
    let good = Request::new(capability.clone())
        .with_input(InputRole::Source, source.clone())
        .with_input(InputRole::Prompt, Payload::text("circle"));
    // Words and no picture: refused when the job is submitted, before any work.
    let bad = Request::new(capability).with_input(InputRole::Prompt, Payload::text("circle"));
    // A real job takes about half a minute on the processor; allow it 100 s of polls.
    check_runner_paced(
        Arc::new(Sam3Runner),
        &weights,
        &good,
        &bad,
        ErrorKind::InvalidRequest,
        Duration::from_millis(100),
    )
    .unwrap();
}
