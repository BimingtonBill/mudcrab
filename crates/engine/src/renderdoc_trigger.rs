//! A RenderDoc capture of a chosen frame, for settled scenes (feature `renderdoc`, development only).
//!
//! `renderdoccmd capture` can only start a capture by hand (F12); a scripted run needs the
//! application to ask. With `MUDCRAB_RENDERDOC_CAPTURE_FRAME=<n>` set, this plugin calls
//! RenderDoc's in-application API once the main frame counter reaches `n`, asking for
//! `MUDCRAB_RENDERDOC_CAPTURE_FRAMES` frames (default 1). The engine must have been launched through
//! RenderDoc (`python tools/perf/perf_capture.py run renderdoc ...` does that): the API is the
//! `renderdoc.dll` that RenderDoc injects, and a run that asks for a capture without it stops with
//! a clear error instead of running uncaptured.
//!
//! An environment variable and not a command-line flag on purpose: it is a development hook, it
//! changes no engine behaviour, and it keeps `EngineConfig` free of a field that only a profiler
//! run reads. Without the variable the plugin does nothing.

use bevy::{diagnostic::FrameCount, prelude::*};
use renderdoc::{RenderDoc, V140};

const FRAME_VARIABLE: &str = "MUDCRAB_RENDERDOC_CAPTURE_FRAME";
const FRAMES_VARIABLE: &str = "MUDCRAB_RENDERDOC_CAPTURE_FRAMES";

pub struct RenderDocTriggerPlugin;

struct Trigger {
    api: RenderDoc<V140>,
    frame: u32,
    frames: u32,
    done: bool,
}

/// A positive whole number from the environment, `None` when the variable is not set.
fn number(name: &str) -> Option<u32> {
    let value = std::env::var(name).ok()?;
    match value.trim().parse::<u32>() {
        Ok(number) if number > 0 => Some(number),
        _ => panic!("{name} must be a whole number above 0, got {value:?}"),
    }
}

impl Plugin for RenderDocTriggerPlugin {
    fn build(&self, app: &mut App) {
        let Some(frame) = number(FRAME_VARIABLE) else {
            return;
        };
        let frames = number(FRAMES_VARIABLE).unwrap_or(1);
        let api = RenderDoc::<V140>::new().unwrap_or_else(|error| {
            panic!(
                "{FRAME_VARIABLE} is set but RenderDoc's API is not available ({error}); \
                 start the engine through `renderdoccmd capture` (perf_capture.py run renderdoc)"
            )
        });
        app.insert_non_send(Trigger {
            api,
            frame,
            frames,
            done: false,
        })
        .add_systems(Update, trigger_capture);
    }
}

fn trigger_capture(frame_count: Res<FrameCount>, mut trigger: NonSendMut<Trigger>) {
    if trigger.done || frame_count.0 < trigger.frame {
        return;
    }
    trigger.done = true;
    let frames = trigger.frames;
    trigger.api.trigger_multi_frame_capture(frames);
    info!(
        frame = frame_count.0,
        frames, "RenderDoc capture requested (main frame counter)"
    );
}
