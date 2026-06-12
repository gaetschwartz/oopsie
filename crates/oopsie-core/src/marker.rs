//! Thread-local trace markers.
//!
//! A marker records the raw stack — one `ip` per physical frame, never
//! symbolicated — at a chosen point. Captures on the same thread embed a
//! snapshot of it, and rendering hides the trace's common suffix with the
//! marker: an exact bottom boundary instead of symbol-name heuristics.

use std::cell::Cell;
use std::sync::Arc;

/// A recorded stack: one ip per physical frame, top → bottom.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct TraceMarker {
    pub(crate) ips: Arc<[usize]>,
}

pub trait FrameLike {
    fn ip(&self) -> usize;
}

impl FrameLike for backtrace::BacktraceFrame {
    #[inline]
    fn ip(&self) -> usize {
        self.ip() as usize
    }
}

impl FrameLike for backtrace::Frame {
    #[inline]
    fn ip(&self) -> usize {
        self.ip() as usize
    }
}

impl TraceMarker {
    /// Number of trailing `trace` frames hidden by this marker: the longest
    /// common `ip` suffix.
    pub(crate) fn cut_len(&self, trace: &[impl FrameLike]) -> usize {
        let m = &self.ips;
        let mut k = 0;
        while k < trace.len()
            && k < m.len()
            && trace[trace.len() - 1 - k].ip() == m[m.len() - 1 - k]
        {
            k += 1;
        }
        k
    }
}

thread_local! {
    static MARKER: Cell<Option<TraceMarker>> = const { Cell::new(None) };
}

fn capture_marker() -> TraceMarker {
    let mut ips = Vec::with_capacity(32);
    backtrace::trace(|frame| {
        ips.push(FrameLike::ip(frame));
        true
    });
    TraceMarker { ips: ips.into() }
}

/// Snapshot the current thread's marker, if any. All slot accessors degrade
/// to a no-op once the thread's TLS is being torn down — capture inside a
/// dying thread's panic hook must never double-panic.
pub fn current() -> Option<TraceMarker> {
    MARKER
        .try_with(|slot| {
            let cur = slot.take();
            let snapshot = cur.clone();
            slot.set(cur);
            snapshot
        })
        .ok()
        .flatten()
}

/// Install a marker captured at the caller, returning the previous marker
/// for [`restore_marker`].
#[doc(hidden)]
#[must_use]
pub fn set_marker() -> Option<TraceMarker> {
    let data = capture_marker();
    MARKER
        .try_with(|slot| {
            let prev = slot.take();
            slot.set(Some(data));
            prev
        })
        .ok()
        .flatten()
}

#[doc(hidden)]
pub fn restore_marker(prev: Option<TraceMarker>) {
    let _ = MARKER.try_with(|slot| slot.set(prev));
}

/// Marks the start of meaningful backtraces on the current thread: frames
/// below this call are hidden from rendered error and panic traces. The
/// function containing the call stays visible. A later call replaces the
/// marker; other threads are unaffected.
/// `RUST_BACKTRACE=full` renders traces unfiltered, ignoring markers.
#[macro_export]
macro_rules! start_marker {
    () => {{
        let _ = $crate::__private::set_marker();
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    impl FrameLike for usize {
        fn ip(&self) -> usize {
            *self
        }
    }

    fn ips(values: &[usize]) -> Arc<[usize]> {
        values.into()
    }

    #[test]
    fn capture_records_frames() {
        let marker = capture_marker();
        assert!(!marker.ips.is_empty());
    }

    #[test]
    fn set_marker_is_latest_wins() {
        let _ = set_marker();
        let first = current().expect("marker set");
        let _ = set_marker();
        let second = current().expect("marker set");
        assert!(!Arc::ptr_eq(&first.ips, &second.ips));
    }

    #[test]
    fn current_returns_clone_and_leaves_slot_intact() {
        let _ = set_marker();
        let a = current().expect("marker set");
        let b = current().expect("still set");
        assert!(Arc::ptr_eq(&a.ips, &b.ips));
    }

    #[test]
    fn set_marker_returns_previous_and_restore_reinstates_it() {
        let _ = set_marker();
        let user_marker = current().expect("marker set");
        let prev = set_marker();
        let replacement = current().expect("replacement marker set");
        assert!(!Arc::ptr_eq(&user_marker.ips, &replacement.ips));
        restore_marker(prev);
        let restored = current().expect("previous marker restored");
        assert!(Arc::ptr_eq(&user_marker.ips, &restored.ips));
    }

    #[test]
    fn cut_len_exact_suffix() {
        let marker = TraceMarker {
            ips: ips(&[900, 10, 20, 30]),
        };
        let trace = ips(&[800, 11, 20, 30]);
        assert_eq!(marker.cut_len(&trace), 2);
    }

    #[test]
    fn cut_len_disjoint_stacks_is_zero() {
        let marker = TraceMarker {
            ips: ips(&[10, 20]),
        };
        assert_eq!(marker.cut_len(&ips(&[77, 88])), 0);
    }

    #[test]
    fn cut_len_empty_marker_is_zero() {
        let marker = TraceMarker { ips: ips(&[]) };
        assert_eq!(marker.cut_len(&ips(&[1])), 0);
    }

    #[test]
    fn cut_len_empty_trace_is_zero() {
        let marker = TraceMarker {
            ips: ips(&[10, 20]),
        };
        assert_eq!(marker.cut_len(&ips(&[])), 0);
    }

    #[test]
    fn cut_len_trace_fully_contained_is_capped() {
        let marker = TraceMarker {
            ips: ips(&[10, 20, 30]),
        };
        let trace = ips(&[20, 30]);
        assert_eq!(marker.cut_len(&trace), 2);
    }
}
