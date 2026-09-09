//! Native scalar policies: one typed expression engine for execution and inspection.
//! No filesystem, database, editor, or AI dependency belongs in this library.
mod execution;
mod syntax;
mod inputs;
pub mod policy_review;
pub mod report;
pub mod bound_report;
pub mod session;
pub mod project;
pub mod project_review;

pub use execution::{evaluate, evaluate_bound, evaluate_bound_observed, evaluate_observed, CancellationToken, Limits, ObservationOptions, ObservedOutcome, Outcome, TraceStep};
pub use syntax::{compile_expression, compile_with_inputs, parse_input_literal, CompiledExpression};
pub use inputs::{InputBindings, InputReference, InputSpec, InputType, MAX_INPUTS, MAX_INPUT_TEXT_UNITS};

pub const MAX_INT: i64 = 9_007_199_254_740_991;
pub const MAX_SOURCE_UNITS: usize = 262_144;
pub const MAX_TOKENS: usize = 20_000;
pub const MAX_DEPTH: usize = 128;

/// UTF-16 offsets, end-exclusive, and one-based line/column, relative to exact input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}
impl Diagnostic {
    pub(crate) fn new(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self { code, message: message.into(), span }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type { Int, Bool, String }
impl Type {
    pub fn name(self) -> &'static str {
        match self { Self::Int => "Int", Self::Bool => "Bool", Self::String => "String" }
    }
}

/// UTF-16 code units deliberately preserve JSON's unpaired surrogate escapes.
/// Rust String would silently narrow the reference engine's String domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value { Int(i64), Bool(bool), Text(Vec<u16>) }
impl Value {
    pub fn value_type(&self) -> Type {
        match self { Self::Int(_) => Type::Int, Self::Bool(_) => Type::Bool, Self::Text(_) => Type::String }
    }
}
