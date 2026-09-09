//! Typed host inputs for the existing expression engine, not a new full-file parser.
use std::collections::{BTreeMap, BTreeSet};
use crate::{CompiledExpression, Diagnostic, Span, Type, Value, MAX_INT};

pub const MAX_INPUTS: usize = 256;
pub const MAX_INPUT_TEXT_UNITS: usize = 262_144;
pub type InputBindings = BTreeMap<String, Value>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputType { Int, PositiveInt, NonNegativeInt, Bool, String }
impl InputType {
    pub fn name(self) -> &'static str {
        match self {
            Self::Int => "Int", Self::PositiveInt => "PositiveInt", Self::NonNegativeInt => "NonNegativeInt",
            Self::Bool => "Bool", Self::String => "String",
        }
    }
    pub fn base_type(self) -> Type {
        match self { Self::Bool => Type::Bool, Self::String => Type::String, _ => Type::Int }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        match name { "Int" => Some(Self::Int), "PositiveInt" => Some(Self::PositiveInt),
            "NonNegativeInt" => Some(Self::NonNegativeInt), "Bool" => Some(Self::Bool),
            "String" => Some(Self::String), _ => None }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputSpec { pub id: String, pub name: String, pub input_type: InputType }
impl InputSpec {
    pub fn new(id: impl Into<String>, name: impl Into<String>, input_type: InputType) -> Self {
        Self { id: id.into(), name: name.into(), input_type }
    }
}

/// An input reference is a real syntax node; its name never substitutes source text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputReference { pub id: String, pub name: String, pub span: Span }

pub(crate) fn boundary_span() -> Span { Span { start: 0, end: 0, line: 1, column: 1 } }
pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && !id.chars().any(|c| c.is_control() || c.is_whitespace())
}
pub(crate) fn check_schema(specs: &[InputSpec]) -> Result<(), Diagnostic> {
    let error = |message| Diagnostic::new("INPUT_SCHEMA", message, boundary_span());
    if specs.len() > MAX_INPUTS { return Err(error("At most 256 inputs are supported.".to_owned())); }
    let mut ids = BTreeSet::new(); let mut names = BTreeSet::new();
    for input in specs {
        let mut chars = input.name.chars();
        let name_ok = input.name.len() <= 128 && chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !matches!(input.name.as_str(), "if" | "then" | "else" | "true" | "false" | "and" | "or" | "not");
        if !valid_id(&input.id) || !name_ok { return Err(error(format!("Invalid input ID or name: {}.", input.id))); }
        if !ids.insert(input.id.as_str()) || !names.insert(input.name.as_str()) {
            return Err(error(format!("Duplicate input ID or name: {}.", input.id)));
        }
    }
    Ok(())
}

/// Validate all arguments, including unused inputs, before any expression executes.
/// Values remain borrowed; execution clones only values actually read by a node.
pub(crate) fn resolve<'a>(program: &CompiledExpression, bindings: &'a InputBindings) -> Result<Vec<&'a Value>, Diagnostic> {
    let error = |code, message| Diagnostic::new(code, message, boundary_span());
    if bindings.len() > MAX_INPUTS { return Err(error("INPUT_LIMIT", "Too many input values.".to_owned())); }
    let declared: BTreeSet<_> = program.inputs().iter().map(|s| s.id.as_str()).collect();
    for id in bindings.keys() {
        if !declared.contains(id.as_str()) { return Err(error("UNKNOWN_INPUT", format!("Unknown input ID {id}."))); }
    }
    let mut text_units = 0usize;
    let mut values = Vec::with_capacity(program.inputs().len());
    for input in program.inputs() {
        let value = bindings.get(&input.id).ok_or_else(|| error("MISSING_INPUT", format!("Missing input {}.", input.id)))?;
        if value.value_type() != input.input_type.base_type() {
            return Err(error("TYPE_ERROR", format!("Input {} requires {}.", input.id, input.input_type.name())));
        }
        match value {
            Value::Int(n) => {
                if !(-MAX_INT..=MAX_INT).contains(n) { return Err(error("INTEGER_RANGE", format!("Input {} is outside the safe integer range.", input.id))); }
                if (input.input_type == InputType::PositiveInt && *n <= 0)
                    || (input.input_type == InputType::NonNegativeInt && *n < 0) {
                    return Err(error("REFINEMENT_VIOLATION", format!("Input {} does not satisfy {}.", input.id, input.input_type.name())));
                }
            }
            Value::Text(text) => {
                text_units = text_units.checked_add(text.len()).filter(|n| *n <= MAX_INPUT_TEXT_UNITS)
                    .ok_or_else(|| error("INPUT_LIMIT", "Input text exceeds the cumulative UTF-16 limit.".to_owned()))?;
            }
            Value::Bool(_) => {}
        }
        values.push(value);
    }
    Ok(values)
}
