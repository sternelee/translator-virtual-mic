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
    session.start().expect("start");

    let frames = 480usize;
    let samples: Vec<f32> = (0..frames)
        .map(|index| if index % 32 < 16 { 0.10 } else { -0.10 })
        .collect();

    session
        .push_input_pcm(&samples, frames, 1, 48_000, 123_456_789)
        .expect("push PCM");

    let mut out = vec![0.0f32; frames];
    let ts = session.pull_output_pcm(&mut out, 1, 48_000).expect("pull PCM");
    let mut shared = vec![0.0f32; frames];
    let (shared_frames, shared_ts) = session
        .read_shared_output_pcm(&mut shared, 1)
        .expect("read shared output");
    let shared_path = session
        .shared_output_path()
        .expect("shared output path");

    println!("timestamp_ns={ts}");
    println!("first_samples={:?}", &out[..8]);
    println!("shared_frames={shared_frames}");
    println!("shared_timestamp_ns={shared_ts}");
    println!("shared_path={shared_path}");
    println!("shared_first_samples={:?}", &shared[..8]);
    println!("metrics={}", session.metrics_json());
}
