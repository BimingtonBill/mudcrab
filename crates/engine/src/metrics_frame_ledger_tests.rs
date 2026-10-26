#[test]
fn write_frame_times_creates_parents_and_preserves_independent_native_ledger_order() {
    use crate::frame_budget::{FrameOwner, ScoredFrame};

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("capture results/run.01/frames.csv");
    let budget_path = directory
        .path()
        .join("capture results/run.01/frames.budget.csv");
    let frame_ms = [4.0, 3.25, 16.6667];
    // Native frames have their own count and order, independent of virtual frame times.
    let scored = [
        ScoredFrame {
            ms: 17.1234567,
            owner: FrameOwner::RenderThread,
            wait_ms: Some(2.7654328),
        },
        ScoredFrame {
            ms: 2.5,
            owner: FrameOwner::Unknown,
            wait_ms: None,
        },
        ScoredFrame {
            ms: 11.25,
            owner: FrameOwner::Gpu,
            wait_ms: Some(0.0),
        },
        ScoredFrame {
            ms: 8.000001,
            owner: FrameOwner::MainWorld,
            wait_ms: Some(0.000001),
        },
        ScoredFrame {
            ms: 6.125,
            owner: FrameOwner::Upload,
            wait_ms: None,
        },
    ];
    assert!(!path.parent().unwrap().exists());

    write_frame_times(&path, &frame_ms, &scored).unwrap();

    let actual_frame_times = std::fs::read_to_string(&path).unwrap();
    assert_eq!(actual_frame_times, frame_times_csv(&frame_ms));
    assert_eq!(
        actual_frame_times,
        "frame,ms\n0,4.0000\n1,3.2500\n2,16.6667\n"
    );
    assert_eq!(
        std::fs::read_to_string(&budget_path).unwrap(),
        concat!(
            "scored_frame,real_ms,owner,main_thread_wait_ms\n",
            "0,17.123457,RenderThread,2.765433\n",
            "1,2.500000,Unknown,\n",
            "2,11.250000,Gpu,0.000000\n",
            "3,8.000001,MainWorld,0.000001\n",
            "4,6.125000,Upload,\n",
        )
    );
}

#[test]
fn write_frame_times_writes_header_only_for_empty_native_ledger() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("empty ledger/run.02/frames.csv");
    let budget_path = directory
        .path()
        .join("empty ledger/run.02/frames.budget.csv");
    let frame_ms = [9.5, 1.25];

    write_frame_times(&path, &frame_ms, &[]).unwrap();

    let actual_frame_times = std::fs::read_to_string(&path).unwrap();
    assert_eq!(actual_frame_times, frame_times_csv(&frame_ms));
    assert_eq!(actual_frame_times, "frame,ms\n0,9.5000\n1,1.2500\n");
    assert_eq!(
        std::fs::read_to_string(&budget_path).unwrap(),
        "scored_frame,real_ms,owner,main_thread_wait_ms\n"
    );
}
