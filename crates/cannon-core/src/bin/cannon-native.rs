use std::io::{self, Read};
use std::process::ExitCode;
use cannon_core::{compile_expression, evaluate, report, Limits, Outcome, MAX_SOURCE_UNITS};

const HELP: &str = "Cannon native expression slice 0.1.0\n\nUsage:\n  cannon-native eval '<expression>' [--json] [--max-steps N] [--max-depth N] [--max-trace N]\n  cannon-native eval --stdin [--json]\n  cannon-native demo\n\nExpressions only: Int, Bool, JSON String, operators, and if/then/else.\nNo module/record/rule/transition parser yet. The existing cannon command is unchanged.\n";

fn run() -> Result<u8, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args == ["--help"] || args == ["-h"] { print!("{HELP}"); return Ok(0); }
    if args == ["demo"] {
        for source in ["8 + 2 <= 10", "8 + 2 < 10", "if 8 + 2 <= 10 then 0 else 3000", "false and (9007199254740991 + 1 == 0)"] {
            let compiled = compile_expression(source).map_err(|e| e.message)?;
            let result = evaluate(&compiled, Limits::default());
            println!("{source}\n{}", report::render(source, "demo.expression", Limits::default(), "execution", &result));
            if result.result.is_err() { return Ok(1); }
        }
        return Ok(0);
    }
    if args[0] != "eval" { return Err("Unknown command. Use --help.".into()); }
    let mut source = None;
    let mut stdin = false;
    let mut json = false;
    let mut limits = Limits::default();
    let mut seen = std::collections::BTreeSet::new();
    let mut index = 1;
    while index < args.len() {
        let arg = &args[index];
        if arg.starts_with("--") {
            if !seen.insert(arg.as_str()) { return Err(format!("Duplicate option {arg}.")); }
            match arg.as_str() {
                "--stdin" => stdin = true,
                "--json" => json = true,
                "--max-steps" | "--max-depth" | "--max-trace" => {
                    index += 1;
                    let raw = args.get(index).ok_or_else(|| format!("{arg} requires a positive integer."))?;
                    if raw.is_empty() || !raw.bytes().all(|c| c.is_ascii_digit()) { return Err(format!("Invalid value for {arg}.")); }
                    let value: usize = raw.parse().map_err(|_| format!("Invalid value for {arg}."))?;
                    let max = if arg == "--max-depth" { 128 } else { 1_000_000 };
                    if value == 0 || value > max { return Err(format!("{arg} must be within 1..={max}.")); }
                    match arg.as_str() { "--max-steps" => limits.max_steps = value, "--max-depth" => limits.max_depth = value, _ => limits.max_trace = value }
                }
                _ => return Err(format!("Unknown option {arg}.")),
            }
        } else if source.replace(arg.clone()).is_some() { return Err("Provide exactly one expression, or --stdin.".into()); }
        index += 1;
    }
    if stdin && source.is_some() { return Err("Do not combine --stdin with an expression argument.".into()); }
    if stdin {
        // Bounded read before parsing; UTF-8 may take at most 3 bytes per UTF-16 unit.
        let max = MAX_SOURCE_UNITS * 3;
        let mut bytes = Vec::new();
        io::stdin().lock().take((max + 1) as u64).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if bytes.len() > max { return Err("Input exceeds source byte limit.".into()); }
        source = Some(String::from_utf8(bytes).map_err(|_| "Source input must be valid UTF-8.")?);
    }
    let source = source.ok_or("Missing expression. Use --help.")?;
    let (phase, outcome) = match compile_expression(&source) {
        Ok(program) => ("execution", evaluate(&program, limits)),
        Err(error) => ("compilation", Outcome { result: Err(error), trace: Vec::new(), steps: 0 }),
    };
    if json { println!("{}", report::render(&source, if stdin { "stdin.expression" } else { "argument.expression" }, limits, phase, &outcome)); }
    else {
        for step in &outcome.trace {
            println!("{}:{} {} {}{}", step.span.line, step.span.column, step.kind, step.label,
                step.value.as_ref().map(|v| format!(" = {}", report::value_json(v))).unwrap_or_default());
        }
        match &outcome.result {
            Ok(value) => println!("Result: {}", report::value_json(value)),
            Err(error) => eprintln!("{} at {}:{}: {}", error.code, error.span.line, error.span.column, error.message),
        }
    }
    Ok(if outcome.result.is_ok() { 0 } else if phase == "compilation" { 2 } else { 1 })
}
fn main() -> ExitCode {
    match run() { Ok(code) => ExitCode::from(code), Err(error) => { eprintln!("{error}"); ExitCode::from(2) } }
}
