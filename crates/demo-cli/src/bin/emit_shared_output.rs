use common::{EngineConfig, EngineMode};
use session_core::EngineSession;

fn main() {
    let mut session = EngineSession::new(EngineConfig {
        mode: EngineMode::Bypass,
        ..EngineConfig::default()
    });
    session
        .enable_shared_output(960, 1, 48_000)
        .expect("enable shared output");
    session.start();

    let frames = 480usize;
    let samples: Vec<f32> = (0..frames)
        .map(|index| if index % 32 < 16 { 0.10 } else { -0.10 })
        .collect();

    session
        .push_input_pcm(&samples, frames, 1, 48_000, 123_456_789)
        .expect("push PCM");

    let shared_path = session
        .shared_output_path()
        .expect("shared output path");

    println!("shared_path={shared_path}");
    println!("frames_written={frames}");
}
