use crate::inputs::{resolve, InputBindings};
use crate::syntax::{NodeKind, Op};
use crate::{CompiledExpression, Diagnostic, Span, Value, MAX_DEPTH, MAX_INT};
use std::ops::ControlFlow;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};

#[derive(Clone, Copy, Debug)]
pub struct Limits { pub max_steps: usize, pub max_depth: usize, pub max_trace: usize }
impl Default for Limits {
    fn default() -> Self { Self { max_steps: 10_000, max_depth: MAX_DEPTH, max_trace: 10_000 } }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceStep { pub kind: &'static str, pub label: &'static str, pub span: Span, pub value: Option<Value> }
#[derive(Clone, Debug)]
pub struct Outcome { pub result: Result<Value, Diagnostic>, pub trace: Vec<TraceStep>, pub steps: usize }

/// A one-way cooperative cancellation signal shared with an editor/host thread.
/// Cancellation is not a deadline, thread termination, or retroactive invalidation.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    pub fn new() -> Self { Self::default() }
    pub fn cancel(&self) { self.0.store(true, Ordering::Relaxed); }
    pub fn is_cancelled(&self) -> bool { self.0.load(Ordering::Relaxed) }
}

#[derive(Clone, Copy, Debug)]
pub struct ObservationOptions {
    /// False delivers events to the observer without retaining their values.
    pub retain_trace: bool,
    /// Sum of UTF-16 units in text values delivered as trace events, including
    /// repeated observations. Not a bound on total memory or encoded JSON bytes.
    pub max_trace_text_units: usize,
}
impl Default for ObservationOptions {
    fn default() -> Self { Self { retain_trace: true, max_trace_text_units: usize::MAX } }
}

/// Streaming metadata stays separate from the legacy Outcome: an empty retained
/// trace must not be mistaken for proof that no operations were observed.
#[derive(Clone, Debug)]
pub struct ObservedOutcome {
    pub outcome: Outcome,
    pub options: ObservationOptions,
    pub emitted_events: usize,
    pub observed_text_units: usize,
}

struct Executor<'a, F> {
    program: &'a CompiledExpression,
    values: Vec<&'a Value>,
    limits: Limits,
    options: ObservationOptions,
    cancellation: &'a CancellationToken,
    observer: F,
    trace: Vec<TraceStep>,
    steps: usize,
    emitted_events: usize,
    observed_text_units: usize,
}
impl<F: FnMut(&TraceStep) -> ControlFlow<()>> Executor<'_, F> {
    fn checkpoint(&self, span: Span) -> Result<(), Diagnostic> {
        if self.cancellation.is_cancelled() {
            Err(Diagnostic::new("CANCELLED", "Execution cancelled; no completed result is available.", span))
        } else { Ok(()) }
    }
    fn note(&mut self, kind: &'static str, label: &'static str, span: Span, value: Option<Value>) -> Result<(), Diagnostic> {
        self.checkpoint(span)?;
        // Count emitted events, not retained events: streaming cannot bypass limits.
        if self.emitted_events >= self.limits.max_trace {
            return Err(Diagnostic::new("TRACE_LIMIT", "Trace budget exceeded; this run is incomplete.", span));
        }
        let units = match &value { Some(Value::Text(text)) => text.len(), _ => 0 };
        let total = self.observed_text_units.checked_add(units)
            .filter(|n| *n <= self.options.max_trace_text_units)
            .ok_or_else(|| Diagnostic::new("TRACE_VALUE_LIMIT", "Observed text budget exceeded; this run is incomplete.", span))?;
        let step = TraceStep { kind, label, span, value };
        self.observed_text_units = total;
        self.emitted_events += 1;
        let decision = (self.observer)(&step);
        if self.options.retain_trace { self.trace.push(step); }
        if decision.is_break() {
            return Err(Diagnostic::new("CANCELLED", "Observer cancelled execution; no completed result is available.", span));
        }
        self.checkpoint(span)
    }
    fn integer(value: Value, span: Span) -> Result<i64, Diagnostic> {
        match value { Value::Int(n) => Ok(n), _ => Err(Diagnostic::new("TYPE_ERROR", "Expected Int.", span)) }
    }
    fn boolean(value: Value, span: Span) -> Result<bool, Diagnostic> {
        match value { Value::Bool(v) => Ok(v), _ => Err(Diagnostic::new("TYPE_ERROR", "Expected Bool.", span)) }
    }
    fn checked(value: Option<i64>, span: Span) -> Result<Value, Diagnostic> {
        value.filter(|n| (-MAX_INT..=MAX_INT).contains(n)).map(Value::Int)
            .ok_or_else(|| Diagnostic::new("INTEGER_RANGE", "Expected a safe integer; overflow and coercion are not permitted.", span))
    }
    fn eval(&mut self, id: usize, depth: usize) -> Result<Value, Diagnostic> {
        let node = &self.program.nodes[id];
        let span = node.span;
        self.checkpoint(span)?;
        self.steps += 1;
        if self.steps > self.limits.max_steps { return Err(Diagnostic::new("STEP_LIMIT", "Execution step budget exceeded.", span)); }
        if depth > self.limits.max_depth { return Err(Diagnostic::new("DEPTH_LIMIT", "Execution depth budget exceeded.", span)); }
        let kind = node.kind.clone();
        let (value, trace_kind, label) = match kind {
            NodeKind::Input(index) => ((*self.values[index]).clone(), "input", "input"),
            NodeKind::Literal(value) => (value, "literal", "literal"),
            NodeKind::Unary(op, child) => {
                let v = self.eval(child, depth + 1)?;
                let result = if op == Op::Neg { Self::checked(Self::integer(v, span)?.checked_neg(), span)? }
                    else { Value::Bool(!Self::boolean(v, span)?) };
                (result, "unary", op.label())
            }
            NodeKind::Binary(op, left, right) => {
                let l = self.eval(left, depth + 1)?;
                if (op == Op::And && l == Value::Bool(false)) || (op == Op::Or && l == Value::Bool(true)) {
                    let label = if op == Op::And { "and: right operand not evaluated" } else { "or: right operand not evaluated" };
                    self.note("short-circuit", label, span, None)?;
                    (l, "binary", op.label())
                } else {
                    let r = self.eval(right, depth + 1)?;
                    let result = match op {
                        Op::Eq => Value::Bool(l == r), Op::Ne => Value::Bool(l != r),
                        Op::And | Op::Or => Value::Bool(Self::boolean(r, self.program.nodes[right].span)?),
                        _ => {
                            let l = Self::integer(l, self.program.nodes[left].span)?;
                            let r = Self::integer(r, self.program.nodes[right].span)?;
                            match op {
                                Op::Add => Self::checked(l.checked_add(r), span)?,
                                Op::Sub => Self::checked(l.checked_sub(r), span)?,
                                Op::Mul => Self::checked(l.checked_mul(r), span)?,
                                Op::Lt => Value::Bool(l < r), Op::Le => Value::Bool(l <= r),
                                Op::Gt => Value::Bool(l > r), Op::Ge => Value::Bool(l >= r),
                                _ => return Err(Diagnostic::new("UNKNOWN_OPERATOR", "Invalid binary operator.", span)),
                            }
                        }
                    };
                    (result, "binary", op.label())
                }
            }
            NodeKind::If(condition, yes, no) => {
                let decision = Self::boolean(self.eval(condition, depth + 1)?, self.program.nodes[condition].span)?;
                self.note("branch", if decision { "then branch selected" } else { "else branch selected" }, span, Some(Value::Bool(decision)))?;
                (self.eval(if decision { yes } else { no }, depth + 1)?, "if", "if")
            }
        };
        self.note(trace_kind, label, span, Some(value.clone()))?;
        Ok(value)
    }
}

/// Evaluate through the same interpreter used by `evaluate`, observing events as
/// they occur. The callback is synchronous and trusted host code. It may cancel,
/// but the engine cannot impose deadlines or catch panics inside host callbacks.
pub fn evaluate_observed<F: FnMut(&TraceStep) -> ControlFlow<()>>(
    program: &CompiledExpression,
    limits: Limits,
    options: ObservationOptions,
    cancellation: &CancellationToken,
    observer: F,
) -> ObservedOutcome {
    evaluate_bound_observed(program, &InputBindings::new(), limits, options, cancellation, observer)
}

pub fn evaluate_bound_observed<F: FnMut(&TraceStep) -> ControlFlow<()>>(
    program: &CompiledExpression,
    bindings: &InputBindings,
    limits: Limits,
    options: ObservationOptions,
    cancellation: &CancellationToken,
    observer: F,
) -> ObservedOutcome {
    if limits.max_steps == 0 || limits.max_steps > 1_000_000 || limits.max_depth == 0 || limits.max_depth > MAX_DEPTH || limits.max_trace == 0 || limits.max_trace > 1_000_000 {
        return ObservedOutcome {
            outcome: Outcome { result: Err(Diagnostic::new("INVALID_LIMIT", "Limits must be positive; depth <= 128, steps/trace <= 1000000.", program.span())), trace: Vec::new(), steps: 0 },
            options, emitted_events: 0, observed_text_units: 0,
        };
    }
    let values = match resolve(program, bindings) {
        Ok(values) => values,
        Err(error) => return ObservedOutcome {
            outcome: Outcome { result: Err(error), trace: Vec::new(), steps: 0 },
            options, emitted_events: 0, observed_text_units: 0,
        },
    };
    let mut executor = Executor {
        program, values, limits, options, cancellation, observer,
        trace: Vec::new(), steps: 0, emitted_events: 0, observed_text_units: 0,
    };
    let result = executor.eval(program.root, 1);
    ObservedOutcome {
        outcome: Outcome { result, trace: executor.trace, steps: executor.steps },
        options, emitted_events: executor.emitted_events, observed_text_units: executor.observed_text_units,
    }
}

pub fn evaluate(program: &CompiledExpression, limits: Limits) -> Outcome {
    evaluate_observed(program, limits, ObservationOptions::default(), &CancellationToken::new(), |_| ControlFlow::Continue(())).outcome
}

/// Bind values by stable ID; all declarations are validated before evaluation.
pub fn evaluate_bound(program: &CompiledExpression, bindings: &InputBindings, limits: Limits) -> Outcome {
    evaluate_bound_observed(program, bindings, limits, ObservationOptions::default(), &CancellationToken::new(), |_| ControlFlow::Continue(())).outcome
}
