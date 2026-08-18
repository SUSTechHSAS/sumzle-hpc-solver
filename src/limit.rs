//! Cooperative work limit for searches that cannot be run to completion.
//!
//! # Why this exists
//!
//! The Sumzle search space grows exponentially with the puzzle length. The RHS
//! value index removes the redundant work, but it cannot change the exponent:
//! at length 10 an exhaustive search still visits ~1.7 billion expressions, and
//! a few characters beyond that no amount of constant-factor tuning finishes in
//! a human timescale.
//!
//! For top-N that is a solvable problem, because the answer is a fixed-size
//! ranking rather than the whole solution set. A partial search still yields a
//! sensible ranking — just one drawn from part of the space instead of all of
//! it. This type is the budget that makes such a search stop on time, and the
//! flag that records that it did, so the result can be reported honestly as
//! approximate rather than passed off as exhaustive.
//!
//! # How it is checked
//!
//! Polling a clock per node would cost more than the search itself, so the
//! deadline is only consulted once every [`CHECK_INTERVAL`] expressions. In
//! between, threads read a single relaxed atomic. Once the budget is spent the
//! flag latches and every recursive frame returns immediately, so the search
//! unwinds promptly across all threads.
//!
//! The result is *deterministic in shape but not in content*: which solutions a
//! truncated search happened to reach depends on thread timing. That is
//! inherent to "stop when time runs out" and is why the flag exists — callers
//! that require reproducibility must run without a budget.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Expressions between deadline checks. A power of two so the test is a mask.
///
/// Large enough that the clock read is amortized to nothing, small enough that
/// the search reacts within milliseconds.
pub const CHECK_INTERVAL: u64 = 1 << 16;

/// A budget for a single search: a wall-clock deadline, a cap on the number of
/// expressions examined, or both.
///
/// Shared across the worker threads of one solve. Cloning is not meaningful —
/// pass it by reference (the parallel solver holds an `Arc`).
#[derive(Debug)]
pub struct SearchLimit {
    /// When the search must stop. `None` = no time limit.
    deadline: Option<Instant>,
    /// Cap on total expressions examined. `u64::MAX` = no cap.
    max_searched: u64,
    /// Latches once the budget is spent; read by every recursive frame.
    exceeded: AtomicBool,
    /// Latches when this budget, or a child folded in via
    /// [`absorb`](Self::absorb), ran out. Distinct from `exceeded`: a fresh
    /// per-pass child must be allowed to run even though an earlier pass was
    /// truncated, but the *result* still has to be reported as approximate.
    stopped_early: AtomicBool,
    /// Set by [`cancel`](Self::cancel). Propagates to child budgets, which a
    /// spent allowance deliberately does not.
    cancelled: AtomicBool,
    /// Expressions counted across all threads, updated in `CHECK_INTERVAL`
    /// batches so the shared line is not touched per leaf.
    searched: AtomicU64,
}

impl SearchLimit {
    /// An unlimited budget: the search runs to completion and its results are
    /// exhaustive.
    pub fn unlimited() -> Self {
        Self {
            deadline: None,
            max_searched: u64::MAX,
            exceeded: AtomicBool::new(false),
            stopped_early: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            searched: AtomicU64::new(0),
        }
    }

    /// Stop after `d` of wall-clock time.
    pub fn with_timeout(d: Duration) -> Self {
        Self {
            deadline: Some(Instant::now() + d),
            ..Self::unlimited()
        }
    }

    /// Stop after examining `n` expressions.
    ///
    /// Unlike a timeout this is reproducible on a given input, but it is only
    /// checked in `CHECK_INTERVAL` batches, so the true stopping point may
    /// overshoot slightly and varies with thread interleaving.
    pub fn with_max_searched(n: u64) -> Self {
        Self {
            max_searched: n,
            ..Self::unlimited()
        }
    }

    /// Whether this budget can ever stop a search. A search under an unbounded
    /// budget skips the bookkeeping entirely.
    #[inline]
    pub fn is_bounded(&self) -> bool {
        self.deadline.is_some() || self.max_searched != u64::MAX
    }

    /// Whether the budget has been spent. Cheap enough to call per node.
    #[inline]
    pub fn is_exceeded(&self) -> bool {
        self.exceeded.load(Ordering::Relaxed)
    }

    /// Force the search to stop (used when a client disconnects).
    ///
    /// Unlike running out of budget, cancellation propagates through
    /// [`split`](Self::split): there is no point starting a later pass for a
    /// caller who has gone away.
    #[inline]
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
        self.exceeded.store(true, Ordering::Relaxed);
    }

    /// Split this budget into a fresh one covering `fraction` of the remaining
    /// allowance.
    ///
    /// The top-N solve makes two passes over the same space — pass 1 gathers
    /// the character statistics the score depends on, pass 2 applies them and
    /// ranks. Sharing one budget between them starves the second: pass 1 spends
    /// everything, pass 2 stops immediately and the solve returns *no
    /// solutions at all*, which is worse than a rough answer. Giving each pass
    /// its own slice guarantees pass 2 has room to produce a ranking.
    ///
    /// The returned limit starts with a fresh counter and, for a deadline,
    /// `fraction` of the time still left.
    pub fn split(&self, fraction: f64) -> Self {
        let fraction = fraction.clamp(0.0, 1.0);

        let deadline = self.deadline.map(|d| {
            let remaining = d.saturating_duration_since(Instant::now());
            Instant::now() + remaining.mul_f64(fraction)
        });

        let spent = self.searched.load(Ordering::Relaxed);
        let max_searched = if self.max_searched == u64::MAX {
            u64::MAX
        } else {
            let remaining = self.max_searched.saturating_sub(spent);
            let share = ((remaining as f64) * fraction) as u64;
            // Never hand out a zero allowance. Budgets are only consulted
            // every CHECK_INTERVAL expressions, so an earlier pass routinely
            // overshoots its share and can leave nothing behind — which would
            // make the next pass stop before ranking anything and return an
            // empty result. One check interval is the smallest amount of work
            // that is actually observable, so it is the floor.
            share.max(CHECK_INTERVAL)
        };

        Self {
            deadline,
            max_searched,
            // An explicitly cancelled search stays cancelled: the child must
            // not resurrect work the caller has abandoned. A budget that
            // merely ran out, though, does not block the next pass — that is
            // the whole point of splitting — it only marks the result
            // approximate, which `stopped_early` carries.
            exceeded: AtomicBool::new(self.cancelled.load(Ordering::Relaxed)),
            stopped_early: AtomicBool::new(self.stopped_early()),
            cancelled: AtomicBool::new(self.cancelled.load(Ordering::Relaxed)),
            searched: AtomicU64::new(0),
        }
    }

    /// Fold a child budget's spending back into this one and record whether it
    /// ran out.
    ///
    /// The "did the search finish?" answer is the parent's to give, but only
    /// the children ever actually hit a wall — so a child that stopped early
    /// must mark the parent too, otherwise a truncated solve would be reported
    /// as exhaustive. `stopped_early` is tracked separately from `exceeded` so
    /// that recording it does not, by itself, prevent a later pass from
    /// running (see [`split`](Self::split)).
    pub fn absorb(&self, child: &SearchLimit) {
        self.searched
            .fetch_add(child.searched.load(Ordering::Relaxed), Ordering::Relaxed);
        if child.is_exceeded() {
            self.stopped_early.store(true, Ordering::Relaxed);
        }
    }

    /// Whether this budget, or any child budget folded into it, ran out.
    ///
    /// This is the flag callers should report to users: it stays true once a
    /// pass has been truncated, even though later passes get a fresh
    /// allowance.
    #[inline]
    pub fn stopped_early(&self) -> bool {
        self.stopped_early.load(Ordering::Relaxed) || self.is_exceeded()
    }

    /// Report `delta` newly examined expressions and re-check the budget.
    ///
    /// Called once per `CHECK_INTERVAL` expressions by each worker, so the
    /// clock read and the shared atomic are amortized across that batch.
    /// Returns `true` if the search should stop.
    #[inline]
    pub fn charge(&self, delta: u64) -> bool {
        if self.exceeded.load(Ordering::Relaxed) {
            return true;
        }
        let total = self.searched.fetch_add(delta, Ordering::Relaxed) + delta;
        if total >= self.max_searched {
            self.exceeded.store(true, Ordering::Relaxed);
            return true;
        }
        if let Some(deadline) = self.deadline {
            if Instant::now() >= deadline {
                self.exceeded.store(true, Ordering::Relaxed);
                return true;
            }
        }
        false
    }
}

impl Default for SearchLimit {
    fn default() -> Self {
        Self::unlimited()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlimited_never_stops() {
        let l = SearchLimit::unlimited();
        assert!(!l.is_bounded());
        assert!(!l.is_exceeded());
        assert!(!l.charge(1_000_000_000));
        assert!(!l.is_exceeded());
    }

    #[test]
    fn max_searched_stops_and_latches() {
        let l = SearchLimit::with_max_searched(100);
        assert!(l.is_bounded());
        assert!(!l.charge(60), "under the cap");
        assert!(l.charge(60), "crossing the cap stops the search");
        assert!(l.is_exceeded(), "the flag latches");
        assert!(l.charge(0), "and stays latched");
    }

    #[test]
    fn timeout_stops_once_elapsed() {
        let l = SearchLimit::with_timeout(Duration::from_millis(1));
        assert!(l.is_bounded());
        std::thread::sleep(Duration::from_millis(5));
        assert!(l.charge(1), "past the deadline the search stops");
        assert!(l.is_exceeded());
    }

    #[test]
    fn cancel_stops_an_unlimited_search() {
        let l = SearchLimit::unlimited();
        l.cancel();
        assert!(l.is_exceeded());
        assert!(l.charge(1));
    }
}
