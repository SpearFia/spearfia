//! Measured voiceover timing for the open project, shared by every timeline view.

use spearfia_script::Script;
use spearfia_script::timing::{self, Timeline};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

// shortcut: one process-wide map because one project is open at a time; pass it down the
// stages if the app ever shows two projects at once.
static MEASURED: LazyLock<Mutex<HashMap<usize, f32>>> = LazyLock::new(Default::default);

/// Replace the measured seconds per script line (the workspace refreshes this).
pub fn set(measured: HashMap<usize, f32>) {
    if let Ok(mut m) = MEASURED.lock() {
        *m = measured;
    }
}

/// The script's timeline, using recorded VO lengths where they exist.
pub fn timeline(script: &Script) -> Timeline {
    match MEASURED.lock() {
        Ok(m) => timing::timeline_measured(script, &m),
        Err(_) => timing::timeline(script),
    }
}

/// Seconds for the spoken text between byte offsets in the main script, and whether they
/// come from recorded VO (`true`) or the words-per-minute estimate.
pub fn passage_seconds(source: &str, range: std::ops::Range<usize>) -> (f32, bool) {
    let text = source.get(range.clone()).unwrap_or("");
    let estimate = spearfia_script::parse(text).est_seconds();
    let Ok(m) = MEASURED.lock() else {
        return (estimate, false);
    };
    // Scale the estimate by the recorded pace of the lines the passage touches.
    let first = source
        .get(..range.start)
        .unwrap_or("")
        .matches('\n')
        .count();
    let lines = first..=first + text.matches('\n').count();
    let script = spearfia_script::parse(source);
    let (mut recorded, mut estimated) = (0.0, 0.0);
    for b in &script.blocks {
        if let spearfia_script::Block::Line {
            kind,
            segments,
            line,
        } = b
            && kind.is_spoken()
            && lines.contains(line)
        {
            let Some(secs) = m.get(line) else {
                return (estimate, false);
            };
            recorded += secs;
            estimated += spearfia_script::estimate_seconds(spearfia_script::word_count(segments));
        }
    }
    if estimated > 0.0 {
        (estimate * recorded / estimated, true)
    } else {
        (estimate, false)
    }
}
