use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use openvoice_desktop::core::events::{AppEventSink, AppShellEvent, ProcessingStage};
use openvoice_desktop::core::state::RecordingState;
use openvoice_desktop::core::status::StatusSnapshot;

fn snapshot() -> Arc<StatusSnapshot> {
    Arc::new(StatusSnapshot::new())
}

#[test]
fn controller_events_move_the_snapshot_through_every_state() {
    let snapshot = snapshot();
    assert_eq!(snapshot.state(), RecordingState::Idle);

    snapshot.emit(AppShellEvent::Recording);
    assert_eq!(snapshot.state(), RecordingState::Recording);

    snapshot.emit(AppShellEvent::Processing {
        stage: ProcessingStage::WritingAudio,
    });
    assert_eq!(snapshot.state(), RecordingState::WritingAudio);

    snapshot.emit(AppShellEvent::Processing {
        stage: ProcessingStage::Uploading,
    });
    assert_eq!(snapshot.state(), RecordingState::Transcribing);

    snapshot.emit(AppShellEvent::Processing {
        stage: ProcessingStage::InsertingText,
    });
    assert_eq!(snapshot.state(), RecordingState::InsertingText);

    snapshot.emit(AppShellEvent::Idle);
    assert_eq!(snapshot.state(), RecordingState::Idle);
}

#[test]
fn the_overlay_stops_reporting_work_as_soon_as_the_text_is_inserted() {
    let snapshot = snapshot();
    snapshot.emit(AppShellEvent::Processing {
        stage: ProcessingStage::InsertingText,
    });
    // The controller still has to restore the clipboard after this event; the
    // user can already see the text, so nothing may keep the overlay up.
    snapshot.emit(AppShellEvent::Inserted);
    assert_eq!(snapshot.state(), RecordingState::Idle);
}

#[test]
fn cancel_reaches_the_snapshot_while_the_upload_is_still_in_flight() {
    let snapshot = snapshot();
    snapshot.emit(AppShellEvent::Processing {
        stage: ProcessingStage::Uploading,
    });
    snapshot.emit(AppShellEvent::Canceled);
    assert_eq!(snapshot.state(), RecordingState::Idle);
}

#[test]
fn transient_events_never_change_the_reported_state() {
    let snapshot = snapshot();
    snapshot.emit(AppShellEvent::Recording);
    snapshot.emit(AppShellEvent::MicLevel { bucket: 7 });
    snapshot.emit(AppShellEvent::Busy {
        state: RecordingState::Recording,
    });
    snapshot.emit(AppShellEvent::Error {
        message: "endpoint unreachable".to_string(),
    });
    assert_eq!(snapshot.state(), RecordingState::Recording);
}

#[test]
fn the_waveform_follows_the_live_recorder_only_while_recording() {
    let snapshot = snapshot();
    let level = Arc::new(AtomicU8::new(0));
    snapshot.attach_level(Arc::clone(&level));

    snapshot.emit(AppShellEvent::Recording);
    level.store(6, Ordering::Relaxed);
    assert_eq!(snapshot.level_bucket(), 6);

    // A stopped recorder keeps its last value; reporting it would leave the
    // overlay waving at a microphone nobody is talking into.
    snapshot.emit(AppShellEvent::Processing {
        stage: ProcessingStage::Uploading,
    });
    assert_eq!(snapshot.level_bucket(), 0);
}

#[test]
fn a_reload_repoints_the_waveform_at_the_new_recorder() {
    let snapshot = snapshot();
    let first = Arc::new(AtomicU8::new(4));
    snapshot.attach_level(Arc::clone(&first));
    snapshot.emit(AppShellEvent::Recording);
    assert_eq!(snapshot.level_bucket(), 4);

    let second = Arc::new(AtomicU8::new(9));
    snapshot.attach_level(Arc::clone(&second));
    first.store(1, Ordering::Relaxed);
    assert_eq!(snapshot.level_bucket(), 9);
}

#[test]
fn reported_levels_stay_inside_the_bucket_range() {
    let snapshot = snapshot();
    snapshot.attach_level(Arc::new(AtomicU8::new(u8::MAX)));
    snapshot.emit(AppShellEvent::Recording);
    assert_eq!(snapshot.level_bucket(), 10);
}

#[test]
fn language_survives_until_it_is_cycled_again() {
    let snapshot = snapshot();
    assert_eq!(snapshot.language(), None);
    snapshot.set_language(Some("es".to_string()));
    assert_eq!(snapshot.language(), Some("es".to_string()));
    snapshot.set_language(None);
    assert_eq!(snapshot.language(), None);
}

#[test]
fn paused_snapshot_stays_active_but_silences_the_meter() {
    let snapshot = Arc::new(StatusSnapshot::new());
    let level = Arc::new(std::sync::atomic::AtomicU8::new(9));
    snapshot.attach_level(level);
    snapshot.emit(AppShellEvent::Recording);
    assert_eq!(snapshot.level_bucket(), 9);
    snapshot.emit(AppShellEvent::Paused);
    assert_eq!(snapshot.state(), RecordingState::Paused);
    assert_eq!(snapshot.level_bucket(), 0);
    snapshot.emit(AppShellEvent::Recording);
    assert_eq!(snapshot.level_bucket(), 9);
}
