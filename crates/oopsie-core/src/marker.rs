//! Thread-local trace markers.
//!
//! A marker records the raw stack — `(ip, symbol_address)` per physical
//! frame, never symbolicated — at a chosen point. Captures on the same thread
//! embed a snapshot of it, and rendering hides the trace's common suffix with
//! the marker: an exact bottom boundary instead of symbol-name heuristics.

use std::cell::Cell;
use std::sync::Arc;

/// Whether the frame that set the marker is itself hidden.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MarkerBoundary {
    /// Hide the setter's own frame too.
    Inclusive,
    /// Keep the caller's frame visible.
    Exclusive,
}

/// A recorded stack: `(ip, symbol_address)` per physical frame, top → bottom.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct TraceMarker {
    pub(crate) frames: Arc<[MarkerFrame]>,
    pub(crate) boundary: MarkerBoundary,
}

#[derive(Debug)]
pub struct MarkerFrame {
    ip: usize,
    symbol_address: usize,
}

impl<F: FrameLike> From<&F> for MarkerFrame {
    #[inline]
    fn from(frame: &F) -> Self {
        Self {
            ip: frame.ip(),
            symbol_address: frame.symbol_address(),
        }
    }
}

pub trait FrameLike {
    fn ip(&self) -> usize;
    fn symbol_address(&self) -> usize;
}

impl FrameLike for backtrace::BacktraceFrame {
    #[inline]
    fn ip(&self) -> usize {
        self.ip() as usize
    }

    #[inline]
    fn symbol_address(&self) -> usize {
        self.symbol_address() as usize
    }
}

impl FrameLike for backtrace::Frame {
    #[inline]
    fn ip(&self) -> usize {
        self.ip() as usize
    }

    #[inline]
    fn symbol_address(&self) -> usize {
        self.symbol_address() as usize
    }
}

impl FrameLike for MarkerFrame {
    #[inline]
    fn ip(&self) -> usize {
        self.ip
    }

    #[inline]
    fn symbol_address(&self) -> usize {
        self.symbol_address
    }
}

impl TraceMarker {
    pub(crate) fn frames(&self) -> &[MarkerFrame] {
        &self.frames
    }

    pub(crate) fn is_inclusive(&self) -> bool {
        self.boundary == MarkerBoundary::Inclusive
    }

    /// Number of trailing `trace` frames hidden by this marker: the longest
    /// common `ip` suffix, extended by one frame for inclusive markers when
    /// the divergent frames share a `symbol_address` (same function,
    /// different call sites within it).
    pub(crate) fn cut_len(&self, trace: &[impl FrameLike]) -> usize {
        let m = &self.frames;
        let mut k = 0;
        while k < trace.len()
            && k < m.len()
            && trace[trace.len() - 1 - k].ip() == m[m.len() - 1 - k].ip
        {
            k += 1;
        }
        if self.is_inclusive()
            && k < trace.len()
            && k < m.len()
            && trace[trace.len() - 1 - k].symbol_address() == m[m.len() - 1 - k].symbol_address
        {
            k += 1;
        }
        k
    }
}

thread_local! {
    static MARKER: Cell<Option<TraceMarker>> = const { Cell::new(None) };
}

fn capture_marker(boundary: MarkerBoundary) -> TraceMarker {
    let mut frames = Vec::with_capacity(32);
    backtrace::trace(|frame| {
        frames.push(MarkerFrame::from(frame));
        true
    });
    TraceMarker {
        frames: frames.into(),
        boundary,
    }
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

#[doc(hidden)]
pub fn set_start_marker() {
    let data = capture_marker(MarkerBoundary::Exclusive);
    let _ = MARKER.try_with(|slot| {
        slot.set(Some(data));
    });
}

/// Install an inclusive marker anchored at the caller, returning the previous
/// marker for [`restore_marker`].
#[doc(hidden)]
#[must_use]
pub fn set_inclusive_marker() -> Option<TraceMarker> {
    let data = capture_marker(MarkerBoundary::Inclusive);
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
    () => {
        $crate::__private::set_start_marker()
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker_frames(pairs: &[(usize, usize)]) -> Arc<[MarkerFrame]> {
        pairs
            .iter()
            .map(|&(ip, symbol_address)| MarkerFrame { ip, symbol_address })
            .collect()
    }

    #[test]
    fn capture_records_frames() {
        let marker = capture_marker(MarkerBoundary::Exclusive);
        assert!(!marker.frames().is_empty());
        assert!(!marker.is_inclusive());
    }

    #[test]
    fn set_start_marker_is_latest_wins() {
        set_start_marker();
        let first = current().expect("marker set");
        set_start_marker();
        let second = current().expect("marker set");
        assert!(!Arc::ptr_eq(&first.frames, &second.frames));
    }

    #[test]
    fn current_returns_clone_and_leaves_slot_intact() {
        set_start_marker();
        let a = current().expect("marker set");
        let b = current().expect("still set");
        assert!(Arc::ptr_eq(&a.frames, &b.frames));
    }

    #[test]
    fn set_inclusive_marker_returns_previous_and_restore_reinstates_it() {
        set_start_marker();
        let user_marker = current().expect("marker set");
        let prev = set_inclusive_marker();
        let inclusive = current().expect("inclusive marker set");
        assert!(inclusive.is_inclusive());
        assert!(!Arc::ptr_eq(&user_marker.frames, &inclusive.frames));
        restore_marker(prev);
        let restored = current().expect("previous marker restored");
        assert!(Arc::ptr_eq(&user_marker.frames, &restored.frames));
    }

    #[test]
    fn cut_len_exact_suffix() {
        let marker = TraceMarker {
            frames: marker_frames(&[(900, 90), (10, 1), (20, 2), (30, 3)]),
            boundary: MarkerBoundary::Exclusive,
        };
        let trace = marker_frames(&[(800, 80), (11, 1), (20, 2), (30, 3)]);
        assert_eq!(marker.cut_len(&trace), 2);
    }

    #[test]
    fn cut_len_inclusive_extends_one_frame_on_symbol_match() {
        let marker = TraceMarker {
            frames: marker_frames(&[(900, 90), (10, 1), (20, 2), (30, 3)]),
            boundary: MarkerBoundary::Inclusive,
        };
        let trace = marker_frames(&[(800, 80), (11, 1), (20, 2), (30, 3)]);
        assert_eq!(marker.cut_len(&trace), 3);
    }

    #[test]
    fn cut_len_inclusive_no_symbol_match_stays_exact() {
        let marker = TraceMarker {
            frames: marker_frames(&[(10, 1), (20, 2)]),
            boundary: MarkerBoundary::Inclusive,
        };
        let trace = marker_frames(&[(11, 7), (20, 2)]);
        assert_eq!(marker.cut_len(&trace), 1);
    }

    #[test]
    fn cut_len_disjoint_stacks_is_zero() {
        let marker = TraceMarker {
            frames: marker_frames(&[(10, 1), (20, 2)]),
            boundary: MarkerBoundary::Exclusive,
        };
        assert_eq!(marker.cut_len(&marker_frames(&[(77, 7), (88, 8)])), 0);
    }

    #[test]
    fn cut_len_empty_marker_is_zero() {
        let marker = TraceMarker {
            frames: marker_frames(&[]),
            boundary: MarkerBoundary::Inclusive,
        };
        assert_eq!(marker.cut_len(&marker_frames(&[(1, 1)])), 0);
    }

    #[test]
    fn cut_len_empty_trace_is_zero() {
        let marker = TraceMarker {
            frames: marker_frames(&[(10, 1), (20, 2)]),
            boundary: MarkerBoundary::Inclusive,
        };
        assert_eq!(marker.cut_len(&marker_frames(&[])), 0);
    }

    #[test]
    fn cut_len_trace_fully_contained_is_capped() {
        let marker = TraceMarker {
            frames: marker_frames(&[(10, 1), (20, 2), (30, 3)]),
            boundary: MarkerBoundary::Exclusive,
        };
        let trace = marker_frames(&[(20, 2), (30, 3)]);
        assert_eq!(marker.cut_len(&trace), 2);
    }
}
