//! A native scalar-policy client; the existing cannon-native CLI is unchanged.
use std::process::ExitCode;
use cannon_core::{bound_report, compile_with_inputs, evaluate_bound, parse_input_literal,
    InputBindings, InputSpec, InputType, Limits, Outcome, Value, MAX_INPUTS};
use cannon_core::policy_review::{review_policies, PolicyCase, ReviewLimits};

const HELP: &str = "Cannon native typed policies\n\nUsage:\n  cannon-policy eval '<expression>' --input ID:NAME:TYPE=LITERAL [--input ...] [--json]\n  cannon-policy demo [--json]\n\nTypes: Int, PositiveInt, NonNegativeInt, Bool, String. Values bind by ID, never by display name.\nThis is a scalar-policy client, not the full .intent file runner. No files are modified.\n";

fn demo(json: bool) -> Result<u8, String> {
    let inputs = vec![InputSpec::new("stock.total", "onHand", InputType::NonNegativeInt),
        InputSpec::new("stock.held", "reserved", InputType::NonNegativeInt),
        InputSpec::new("request.quantity", "quantity", InputType::PositiveInt)];
    let baseline = compile_with_inputs("reserved + quantity <= onHand", &inputs).map_err(|e| e.message)?;
    let candidate = compile_with_inputs("reserved + quantity < onHand", &inputs).map_err(|e| e.message)?;
    let bindings = InputBindings::from([("stock.total".into(), Value::Int(10)),
        ("stock.held".into(), Value::Int(8)), ("request.quantity".into(), Value::Int(2))]);
    let old = PolicyCase { id: "case.exact".into(), name: "Reserve all remaining stock".into(), inputs: bindings, expected: Value::Bool(true) };
    let mut changed = old.clone(); changed.expected = Value::Bool(false);
    let review = review_policies(&baseline, &candidate, &[old], &[changed], ReviewLimits::default()).map_err(|e| e.message)?;
    if json { println!("{}", bound_report::render_review(&review)); }
    else {
        println!("Baseline: {}\nCandidate: {}", baseline.source(), candidate.source());
        println!("Candidate examples: {:?}", cannon_core::policy_review::PolicyReview::status(&review.candidate_results));
        for change in &review.case_changes { println!("Case {}: {}", change.id, change.kind); }
        for case in &review.historical_results {
            println!("Historical {}: {}; expected {:?}, got {:?}", case.case.id,
                if case.passed { "PASSED" } else { "FAILED" }, case.case.expected, case.outcome.result);
        }
        println!("Review gate passed: {}. This demo intentionally exposes a changed requirement.", review.passed());
    }
    if review.regressions() != ["case.exact"] || review.passed() { return Err("Demo did not expose the expected regression.".into()); }
    Ok(0)
}
fn run() -> Result<u8, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args == ["--help"] || args == ["-h"] { print!("{HELP}"); return Ok(0); }
    if args == ["demo"] || args == ["demo", "--json"] { return demo(args.len() == 2); }
    if args.first().map(String::as_str) != Some("eval") { return Err("Unknown command; use --help.".into()); }
    let mut source = None; let mut json = false; let mut specs = Vec::new(); let mut bindings = InputBindings::new();
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--json" if !json => json = true,
            "--input" => {
                index += 1;
                let raw = args.get(index).ok_or("--input requires ID:NAME:TYPE=LITERAL.")?;
                let (schema, literal) = raw.split_once('=').ok_or("Input is missing =LITERAL.")?;
                let parts: Vec<_> = schema.split(':').collect();
                if parts.len() != 3 { return Err("Use ID:NAME:TYPE=LITERAL; CLI IDs cannot contain a colon.".into()); }
                if specs.len() >= MAX_INPUTS { return Err("Too many inputs.".into()); }
                let ty = InputType::from_name(parts[2]).ok_or("Unknown input type.")?;
                let value = parse_input_literal(literal).map_err(|e| format!("{}: {}", e.code, e.message))?;
                if bindings.insert(parts[0].to_owned(), value).is_some() { return Err("Duplicate input ID.".into()); }
                specs.push(InputSpec::new(parts[0], parts[1], ty));
            }
            option if option.starts_with("--") => return Err(format!("Unknown or duplicate option {option}.")),
            expression => { if source.replace(expression.to_owned()).is_some() { return Err("Provide exactly one expression.".into()); } }
        }
        index += 1;
    }
    let source = source.ok_or("Missing expression.")?;
    let limits = Limits::default();
    let compiled = compile_with_inputs(&source, &specs);
    let (phase, outcome) = match &compiled {
        Ok(program) => ("execution", evaluate_bound(program, &bindings, limits)),
        Err(error) => ("compilation", Outcome { result: Err(error.clone()), trace: Vec::new(), steps: 0 }),
    };
    if json { println!("{}", bound_report::render(&source, &specs, &bindings, compiled.as_ref().ok(), limits, phase, &outcome)); }
    else {
        for step in &outcome.trace { println!("{}:{} {} {} {:?}", step.span.line, step.span.column, step.kind, step.label, step.value); }
        match &outcome.result { Ok(v) => println!("Result: {}", cannon_core::report::value_json(v)),
            Err(e) => eprintln!("{} at {}:{}: {}", e.code, e.span.line, e.span.column, e.message) }
    }
    Ok(if outcome.result.is_ok() { 0 } else if phase == "compilation" { 2 } else { 1 })
}
fn main() -> ExitCode {
    match run() { Ok(code) => ExitCode::from(code), Err(error) => { eprintln!("{error}"); ExitCode::from(2) } }
}
