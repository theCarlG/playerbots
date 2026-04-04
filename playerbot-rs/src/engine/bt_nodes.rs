/// Behavior Tree node types.
///
/// All nodes are allocated once at bot init and reused every tick.
/// Zero heap allocation during the tick loop.
///
/// Design:
/// - Sequence: all children must succeed (left-to-right)
/// - Selector: first child that succeeds wins
/// - UtilitySelector: highest-scoring child that succeeds wins (replaces priority queue)
/// - CooldownGate: spell cooldown check before running child
/// - Condition: pure predicate on TickContext
/// - ActionLeaf: issues a command, returns Success/Failure
/// - PhaseSelector: routes to a subtree based on current encounter phase
use crate::engine::context::TickContext;
use crate::ffi::SpellId;

/// Result of a single BT node tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BtResult {
    /// Node completed successfully. Parent can continue.
    Success,
    /// Node could not execute or its condition was false. Parent should try next.
    Failure,
    /// Node started a long-running action (e.g. movement) and is still in progress.
    /// Parent should not try other children this tick.
    Running,
}

/// A single node in a behavior tree.
pub trait BtNode: Send {
    fn tick(&self, ctx: &mut TickContext<'_>) -> BtResult;
}

// ── Composite nodes ──────────────────────────────────────────────────────

/// All children must succeed in order. Fails immediately if any child fails.
pub struct Sequence {
    pub children: Vec<Box<dyn BtNode>>,
}

impl BtNode for Sequence {
    fn tick(&self, ctx: &mut TickContext<'_>) -> BtResult {
        for child in &self.children {
            match child.tick(ctx) {
                BtResult::Success => continue,
                other => return other,
            }
        }
        BtResult::Success
    }
}

/// First child that returns Success wins. Returns Failure if all children fail.
pub struct Selector {
    pub children: Vec<Box<dyn BtNode>>,
}

impl BtNode for Selector {
    fn tick(&self, ctx: &mut TickContext<'_>) -> BtResult {
        for child in &self.children {
            match child.tick(ctx) {
                BtResult::Failure => continue,
                other => return other,
            }
        }
        BtResult::Failure
    }
}

/// Evaluates all children, executes the highest-scoring one that returns Success.
/// Replaces the current C++ priority queue + all-triggers-polled loop.
///
/// Children are (base_score, node) pairs. Scores are not multiplied here —
/// individual nodes can adjust by returning Failure to be skipped.
/// The list is iterated in order; if two have equal score, the earlier one wins.
pub struct UtilitySelector {
    /// Sorted descending by score at construction time.
    children: Vec<(f32, Box<dyn BtNode>)>,
}

impl UtilitySelector {
    pub fn new(mut children: Vec<(f32, Box<dyn BtNode>)>) -> Self {
        children.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Self { children }
    }
}

impl BtNode for UtilitySelector {
    fn tick(&self, ctx: &mut TickContext<'_>) -> BtResult {
        for (_, child) in &self.children {
            match child.tick(ctx) {
                BtResult::Failure => continue,
                other => return other,
            }
        }
        BtResult::Failure
    }
}

// ── Decorator nodes ──────────────────────────────────────────────────────

/// Guards a child behind a spell cooldown check.
/// Returns Failure if the spell is still on cooldown (don't try this action).
/// Returns the child's result if the spell is ready.
pub struct CooldownGate {
    pub spell_id: SpellId,
    pub child: Box<dyn BtNode>,
}

impl BtNode for CooldownGate {
    fn tick(&self, ctx: &mut TickContext<'_>) -> BtResult {
        if ctx
            .timers
            .spell_on_cooldown(self.spell_id, ctx.server_time_ms)
        {
            return BtResult::Failure;
        }
        self.child.tick(ctx)
    }
}

/// Guards a child behind a GCD check.
pub struct GcdGate {
    pub child: Box<dyn BtNode>,
}

impl BtNode for GcdGate {
    fn tick(&self, ctx: &mut TickContext<'_>) -> BtResult {
        if ctx.timers.gcd_active(ctx.server_time_ms) {
            return BtResult::Failure;
        }
        self.child.tick(ctx)
    }
}

/// Inverts Success↔Failure (Running is unchanged).
pub struct Inverter {
    pub child: Box<dyn BtNode>,
}

impl BtNode for Inverter {
    fn tick(&self, ctx: &mut TickContext<'_>) -> BtResult {
        match self.child.tick(ctx) {
            BtResult::Success => BtResult::Failure,
            BtResult::Failure => BtResult::Success,
            r => r,
        }
    }
}

/// Runs child only once per `interval_ms` milliseconds.
/// Returns Failure (not Running) when throttled, so the parent can try alternatives.
pub struct ThrottleGate {
    pub interval_ms: u64,
    pub child: Box<dyn BtNode>,
    last_run_ms: std::cell::Cell<u64>,
}

impl ThrottleGate {
    pub fn new(interval_ms: u64, child: Box<dyn BtNode>) -> Self {
        Self {
            interval_ms,
            child,
            last_run_ms: std::cell::Cell::new(0),
        }
    }
}

impl BtNode for ThrottleGate {
    fn tick(&self, ctx: &mut TickContext<'_>) -> BtResult {
        let now = ctx.server_time_ms;
        if now.saturating_sub(self.last_run_ms.get()) < self.interval_ms {
            return BtResult::Failure;
        }
        let result = self.child.tick(ctx);
        if result != BtResult::Failure {
            self.last_run_ms.set(now);
        }
        result
    }
}

// ── Leaf nodes ───────────────────────────────────────────────────────────

/// Pure condition — returns Success if the predicate is true, Failure otherwise.
/// Never issues commands.
pub struct Condition {
    pub check: Box<dyn Fn(&TickContext<'_>) -> bool + Send>,
}

impl BtNode for Condition {
    fn tick(&self, ctx: &mut TickContext<'_>) -> BtResult {
        if (self.check)(ctx) {
            BtResult::Success
        } else {
            BtResult::Failure
        }
    }
}

/// Command leaf — issues one game command via the interface.
/// Returns Success if the command succeeded, Failure otherwise.
pub struct ActionLeaf {
    pub execute: Box<dyn Fn(&mut TickContext<'_>) -> BtResult + Send>,
}

impl BtNode for ActionLeaf {
    fn tick(&self, ctx: &mut TickContext<'_>) -> BtResult {
        (self.execute)(ctx)
    }
}

// ── Convenience constructors ─────────────────────────────────────────────

/// Build a condition node.
pub fn cond(check: impl Fn(&TickContext<'_>) -> bool + Send + 'static) -> Box<dyn BtNode> {
    Box::new(Condition {
        check: Box::new(check),
    })
}

/// Build an action leaf.
pub fn action(
    execute: impl Fn(&mut TickContext<'_>) -> BtResult + Send + 'static,
) -> Box<dyn BtNode> {
    Box::new(ActionLeaf {
        execute: Box::new(execute),
    })
}

/// Sequence of boxed nodes.
pub fn seq(children: Vec<Box<dyn BtNode>>) -> Box<dyn BtNode> {
    Box::new(Sequence { children })
}

/// Selector of boxed nodes.
pub fn sel(children: Vec<Box<dyn BtNode>>) -> Box<dyn BtNode> {
    Box::new(Selector { children })
}

/// UtilitySelector with (score, node) pairs.
pub fn util(children: Vec<(f32, Box<dyn BtNode>)>) -> Box<dyn BtNode> {
    Box::new(UtilitySelector::new(children))
}

/// CooldownGate wrapping a child.
pub fn cd_gate(spell_id: SpellId, child: Box<dyn BtNode>) -> Box<dyn BtNode> {
    Box::new(CooldownGate { spell_id, child })
}

/// GcdGate wrapping a child.
pub fn gcd_gate(child: Box<dyn BtNode>) -> Box<dyn BtNode> {
    Box::new(GcdGate { child })
}

/// Inverter wrapping a child.
pub fn not(child: Box<dyn BtNode>) -> Box<dyn BtNode> {
    Box::new(Inverter { child })
}

/// ThrottleGate — run at most once per interval.
pub fn throttle(interval_ms: u64, child: Box<dyn BtNode>) -> Box<dyn BtNode> {
    Box::new(ThrottleGate::new(interval_ms, child))
}

/// Cast a spell at a unit target: GCD gate + cooldown gate + cast action.
pub fn cast_on_target(
    spell_id: SpellId,
    target_fn: impl Fn(&TickContext<'_>) -> Option<u64> + Send + 'static,
) -> Box<dyn BtNode> {
    gcd_gate(cd_gate(
        spell_id,
        action(move |ctx| {
            if let Some(target) = target_fn(ctx) {
                if ctx.interface.cast_spell(spell_id, target) {
                    ctx.timers.on_spell_cast(spell_id, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            } else {
                BtResult::Failure
            }
        }),
    ))
}

/// Cast a spell at the current target.
pub fn cast_on_current_target(spell_id: SpellId) -> Box<dyn BtNode> {
    cast_on_target(spell_id, |ctx| {
        let h = ctx.snap.self_.current_target;
        if h == 0 { None } else { Some(h) }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::context::tests::make_test_ctx;

    #[test]
    fn sequence_succeeds_when_all_children_succeed() {
        let tree = seq(vec![cond(|_| true), cond(|_| true)]);
        let mut owned = make_test_ctx();
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Success);
    }

    #[test]
    fn sequence_fails_on_first_failure() {
        let tree = seq(vec![
            cond(|_| true),
            cond(|_| false),
            cond(|_| true), // must not be reached
        ]);
        let mut owned = make_test_ctx();
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Failure);
    }

    #[test]
    fn selector_returns_first_success() {
        let tree = sel(vec![
            cond(|_| false),
            cond(|_| true),
            cond(|_| true), // should not be reached
        ]);
        let mut owned = make_test_ctx();
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Success);
    }

    #[test]
    fn selector_fails_when_all_children_fail() {
        let tree = sel(vec![cond(|_| false), cond(|_| false)]);
        let mut owned = make_test_ctx();
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Failure);
    }

    #[test]
    fn utility_selector_picks_highest_scoring_success() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU8, Ordering};
        let called = Arc::new(AtomicU8::new(0));
        let called2 = called.clone();

        let tree = util(vec![
            (10.0, cond(|_| false)), // highest score but fails
            (
                8.0,
                action(move |_| {
                    called2.fetch_add(1, Ordering::SeqCst);
                    BtResult::Success
                }),
            ),
            (5.0, cond(|_| true)), // lower score, should not be reached
        ]);
        let mut owned = make_test_ctx();
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Success);
        assert_eq!(called.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cooldown_gate_blocks_when_spell_on_cd() {
        let tree = cd_gate(SpellId(1234), cond(|_| true));
        let mut owned = make_test_ctx();
        let now = owned.time_ms;
        owned.timers.set_cooldown(SpellId(1234), 5000, now);
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Failure);
    }

    #[test]
    fn cooldown_gate_passes_when_spell_ready() {
        let tree = cd_gate(SpellId(1234), cond(|_| true));
        let mut owned = make_test_ctx();
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Success);
    }
}
