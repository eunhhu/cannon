use crate::syntax::{NodeKind, Op};
use crate::{CompiledExpression, Diagnostic, Span, Value, MAX_DEPTH, MAX_INT};

#[derive(Clone, Copy, Debug)]
pub struct Limits { pub max_steps: usize, pub max_depth: usize, pub max_trace: usize }
impl Default for Limits {
    fn default() -> Self { Self { max_steps: 10_000, max_depth: MAX_DEPTH, max_trace: 10_000 } }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceStep { pub kind: &'static str, pub label: &'static str, pub span: Span, pub value: Option<Value> }
#[derive(Clone, Debug)]
pub struct Outcome { pub result: Result<Value, Diagnostic>, pub trace: Vec<TraceStep>, pub steps: usize }
struct Executor<'a> { program: &'a CompiledExpression, limits: Limits, trace: Vec<TraceStep>, steps: usize }
impl Executor<'_> {
    fn note(&mut self, kind: &'static str, label: &'static str, span: Span, value: Option<Value>) -> Result<(), Diagnostic> {
        if self.trace.len() >= self.limits.max_trace { return Err(Diagnostic::new("TRACE_LIMIT", "Trace budget exceeded; this run is incomplete.", span)); }
        self.trace.push(TraceStep { kind, label, span, value }); Ok(())
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
        self.steps += 1;
        if self.steps > self.limits.max_steps { return Err(Diagnostic::new("STEP_LIMIT", "Execution step budget exceeded.", span)); }
        if depth > self.limits.max_depth { return Err(Diagnostic::new("DEPTH_LIMIT", "Execution depth budget exceeded.", span)); }
        // Clone one flat arena node, not the whole subtree.
        let kind = node.kind.clone();
        let (value, trace_kind, label) = match kind {
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

pub fn evaluate(program: &CompiledExpression, limits: Limits) -> Outcome {
    if limits.max_steps == 0 || limits.max_steps > 1_000_000 || limits.max_depth == 0 || limits.max_depth > MAX_DEPTH || limits.max_trace == 0 || limits.max_trace > 1_000_000 {
        return Outcome { result: Err(Diagnostic::new("INVALID_LIMIT", "Limits must be positive; depth <= 128, steps/trace <= 1000000.", program.span())), trace: Vec::new(), steps: 0 };
    }
    let mut executor = Executor { program, limits, trace: Vec::new(), steps: 0 };
    let result = executor.eval(program.root, 1);
    Outcome { result, trace: executor.trace, steps: executor.steps }
}
