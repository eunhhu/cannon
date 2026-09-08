//! A small, versioned JSON report for native expressions; NOT a full evidence certificate.
use std::fmt::Write;
use crate::{Diagnostic, Limits, Outcome, Span, Value};

pub fn quote_units(units: &[u16]) -> String {
    let mut text = String::from("\"");
    for &unit in units {
        match unit {
            34 => text.push_str("\\\""), 92 => text.push_str("\\\\"),
            0x20..=0x7e => text.push(char::from_u32(unit as u32).expect("ASCII")),
            _ => { write!(text, "\\u{unit:04x}").expect("writing to String"); }
        }
    }
    text.push('"'); text
}
pub fn quote(text: &str) -> String { quote_units(&text.encode_utf16().collect::<Vec<_>>()) }
pub fn value_json(value: &Value) -> String {
    match value { Value::Int(n) => n.to_string(), Value::Bool(v) => v.to_string(), Value::Text(units) => quote_units(units) }
}
fn span_json(span: Span) -> String {
    format!("{{\"start\":{},\"end\":{},\"line\":{},\"column\":{}}}", span.start, span.end, span.line, span.column)
}
fn error_json(error: &Diagnostic) -> String {
    format!("{{\"code\":{},\"message\":{},\"span\":{}}}", quote(error.code), quote(&error.message), span_json(error.span))
}
pub fn render(source: &str, uri: &str, limits: Limits, phase: &str, outcome: &Outcome) -> String {
    let result = match &outcome.result {
        Ok(value) => format!("\"ok\":true,\"value\":{},\"type\":{}", value_json(value), quote(value.value_type().name())),
        Err(error) => format!("\"ok\":false,\"error\":{}", error_json(error)),
    };
    let trace = outcome.trace.iter().enumerate().map(|(index, entry)| {
        let value = entry.value.as_ref().map(|v| format!(",\"value\":{}", value_json(v))).unwrap_or_default();
        format!("{{\"index\":{index},\"kind\":{},\"label\":{},\"span\":{}{value}}}", quote(entry.kind), quote(entry.label), span_json(entry.span))
    }).collect::<Vec<_>>().join(",");
    format!("{{\"schema\":\"cannon.native.expression/1\",\"engine\":{{\"name\":\"cannon-core\",\"version\":{},\"implementation\":\"rust\"}},\"source\":{{\"uri\":{},\"text\":{}}},\"limits\":{{\"maxSteps\":{},\"maxDepth\":{},\"maxTrace\":{}}},\"phase\":{},\"steps\":{},\"trace\":[{}],{}}}", quote(env!("CARGO_PKG_VERSION")), quote(uri), quote(source), limits.max_steps, limits.max_depth, limits.max_trace, quote(phase), outcome.steps, trace, result)
}
