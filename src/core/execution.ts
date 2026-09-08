import type { Compilation, Expr, Outcome, Program, RecordDef, RecordValue, RunOptions, ScenarioDef, ScenarioResult, Span, TraceStep, Value } from './model.js';
import { equal, freeze } from './util.js';
export const DEFAULT_RUN_OPTIONS = Object.freeze({ maxSteps: 10000, maxDepth: 128, maxTrace: 10000 });
export function runOptions(options: RunOptions = {}): Required<RunOptions> {
    const resolved = { ...DEFAULT_RUN_OPTIONS, ...options };
    for (const [name, value] of Object.entries(resolved))
        if (!Number.isSafeInteger(value) || value < 1 || value > 1000000)
            throw new Error(`Invalid execution limit ${name}.`);
    if (resolved.maxDepth > 128)
        throw new Error('maxDepth may not exceed 128.');
    return resolved;
}
class ExecutionError extends Error {
    constructor(public readonly code: string, message: string, public readonly span: Span) { super(message); }
}
class Executor {
    readonly trace: TraceStep[] = [];
    steps = 0;
    private depth = 0;
    constructor(private readonly program: Program, private readonly options: Required<RunOptions>) { }
    private fail(code: string, message: string, span: Span): never { throw new ExecutionError(code, message, span); }
    private tick(span: Span): void { this.steps++; if (this.steps > this.options.maxSteps)
        this.fail('STEP_LIMIT', 'Execution step budget exceeded.', span); }
    private note(kind: string, label: string, span: Span, value?: Value, detail?: string): void {
        if (this.trace.length >= this.options.maxTrace)
            this.fail('TRACE_LIMIT', 'Trace budget exceeded; this run is incomplete.', span);
        const entry: TraceStep = { index: this.trace.length, kind, label, span };
        if (value !== undefined)
            entry.value = value;
        if (detail !== undefined)
            entry.detail = detail;
        this.trace.push(entry);
    }
    private integer(value: Value, span: Span): number {
        if (typeof value !== 'number' || !Number.isSafeInteger(value))
            this.fail('INTEGER_RANGE', 'Expected a safe integer; overflow and coercion are not permitted.', span);
        return value;
    }
    private boolean(value: Value, span: Span): boolean { if (typeof value !== 'boolean')
        this.fail('TYPE_ERROR', 'Expected Bool.', span); return value; }
    private record(id: string | undefined, span: Span): RecordDef {
        const def = this.program.definitions.find(d => d.id === id);
        if (!def || def.kind !== 'record')
            this.fail('UNKNOWN_RECORD', `Record ${id ?? '<unbound>'} is absent in this snapshot.`, span);
        return def;
    }
    private validate(value: Value, type: string, span: Span): void {
        this.tick(span);
        if (['Int', 'PositiveInt', 'NonNegativeInt'].includes(type)) {
            const n = this.integer(value, span);
            if ((type === 'PositiveInt' && n <= 0) || (type === 'NonNegativeInt' && n < 0))
                this.fail('REFINEMENT_VIOLATION', `Value ${n} does not satisfy ${type}.`, span);
            return;
        }
        if (type === 'Bool') {
            this.boolean(value, span);
            return;
        }
        if (type === 'String') {
            if (typeof value !== 'string')
                this.fail('TYPE_ERROR', 'Expected String.', span);
            return;
        }
        const record = this.program.definitions.find(d => d.name === type && d.kind === 'record');
        if (!record || record.kind !== 'record')
            this.fail('UNKNOWN_TYPE', `Type ${type} is absent in this snapshot.`, span);
        this.validateRecord(value, record, span);
    }
    private validateRecord(value: Value, record: RecordDef, span: Span): void {
        this.tick(span);
        this.depth++;
        try {
            if (this.depth > this.options.maxDepth)
                this.fail('DEPTH_LIMIT', 'Execution depth budget exceeded.', span);
            if (!value || typeof value !== 'object' || Array.isArray(value) || value.$record !== record.id || !value.fields || typeof value.fields !== 'object' || Array.isArray(value.fields))
                this.fail('TYPE_ERROR', `Expected nominal record ${record.name}.`, span);
            const keys = Object.keys(value.fields);
            if (keys.length !== record.fields.length || keys.some(key => !record.fields.some(field => field.id === key)))
                this.fail('RECORD_SHAPE', `Record ${record.name} has missing or unknown fields.`, span);
            const env = new Map<string, Value>();
            for (const field of record.fields) {
                const fieldValue = value.fields[field.id];
                if (fieldValue === undefined)
                    this.fail('RECORD_SHAPE', `Missing field ${field.name}.`, span);
                this.validate(fieldValue, field.type, span);
                env.set(field.id, fieldValue);
            }
            for (const invariant of record.invariants) {
                const satisfied = this.boolean(this.eval(invariant, env), invariant.span);
                this.note('invariant', `${record.name} invariant`, invariant.span, satisfied);
                if (!satisfied)
                    this.fail('INVARIANT_VIOLATION', `Invariant of ${record.name} is false.`, invariant.span);
            }
        }
        finally {
            this.depth--;
        }
    }
    invoke(targetId: string, args: Value[], span: Span): Value {
        this.tick(span);
        const def = this.program.definitions.find(d => d.id === targetId);
        if (!def || (def.kind !== 'rule' && def.kind !== 'transition'))
            this.fail('UNKNOWN_DEFINITION', `Callable ${targetId} is absent in this snapshot.`, span);
        if (args.length !== def.params.length)
            this.fail('ARGUMENT_COUNT', `${def.name} expects ${def.params.length} arguments.`, span);
        const env = new Map<string, Value>();
        def.params.forEach((param, i) => { this.validate(args[i]!, param.type, span); env.set(param.name, args[i]!); });
        if (def.kind === 'transition')
            for (const guard of def.guards) {
                const passed = this.boolean(this.eval(guard.condition, env), guard.span);
                this.note('guard', guard.error, guard.span, passed);
                if (!passed)
                    this.fail('GUARD_REJECTED', guard.error, guard.span);
            }
        const value = this.eval(def.kind === 'rule' ? def.body : def.next, env);
        this.validate(value, def.returns, def.span);
        this.note('return', `${def.kind} ${def.name}`, def.span, value);
        return value;
    }
    eval(expr: Expr, env: Map<string, Value> = new Map()): Value {
        this.tick(expr.span);
        this.depth++;
        try {
            if (this.depth > this.options.maxDepth)
                this.fail('DEPTH_LIMIT', 'Execution depth budget exceeded.', expr.span);
            let value: Value;
            switch (expr.kind) {
                case 'literal':
                    value = expr.value;
                    break;
                case 'ref': {
                    const result = env.get(expr.fieldId ?? expr.name);
                    if (result === undefined)
                        this.fail('UNKNOWN_BINDING', `Binding ${expr.name} is missing.`, expr.span);
                    value = result;
                    break;
                }
                case 'field': {
                    const object = this.eval(expr.object, env);
                    if (!object || typeof object !== 'object' || !expr.fieldId || !Object.hasOwn(object.fields, expr.fieldId))
                        this.fail('UNKNOWN_FIELD', `Field ${expr.name} is missing.`, expr.span);
                    value = object.fields[expr.fieldId]!;
                    break;
                }
                case 'unary': {
                    const operand = this.eval(expr.operand, env);
                    value = expr.op === '-' ? this.integer(-this.integer(operand, expr.span), expr.span) : !this.boolean(operand, expr.span);
                    break;
                }
                case 'binary': {
                    const left = this.eval(expr.left, env);
                    if (expr.op === 'and' && !this.boolean(left, expr.left.span)) {
                        value = false;
                        this.note('short-circuit', 'and: right operand not evaluated', expr.span);
                        break;
                    }
                    if (expr.op === 'or' && this.boolean(left, expr.left.span)) {
                        value = true;
                        this.note('short-circuit', 'or: right operand not evaluated', expr.span);
                        break;
                    }
                    const right = this.eval(expr.right, env);
                    if (expr.op === '==')
                        value = equal(left, right);
                    else if (expr.op === '!=')
                        value = !equal(left, right);
                    else if (expr.op === 'and' || expr.op === 'or')
                        value = this.boolean(right, expr.right.span);
                    else {
                        const l = this.integer(left, expr.left.span);
                        const r = this.integer(right, expr.right.span);
                        switch (expr.op) {
                            case '+':
                                value = this.integer(l + r, expr.span);
                                break;
                            case '-':
                                value = this.integer(l - r, expr.span);
                                break;
                            case '*':
                                value = this.integer(l * r, expr.span);
                                break;
                            case '<':
                                value = l < r;
                                break;
                            case '<=':
                                value = l <= r;
                                break;
                            case '>':
                                value = l > r;
                                break;
                            case '>=':
                                value = l >= r;
                                break;
                            default: this.fail('UNKNOWN_OPERATOR', expr.op, expr.span);
                        }
                    }
                    break;
                }
                case 'if': {
                    const condition = this.boolean(this.eval(expr.condition, env), expr.condition.span);
                    this.note('branch', condition ? 'then branch selected' : 'else branch selected', expr.span, condition);
                    value = this.eval(condition ? expr.then : expr.otherwise, env);
                    break;
                }
                case 'call': {
                    if (!expr.targetId)
                        this.fail('UNBOUND_CALL', expr.name, expr.span);
                    value = this.invoke(expr.targetId, expr.args.map(arg => this.eval(arg, env)), expr.span);
                    break;
                }
                case 'record': {
                    const definition = this.record(expr.targetId, expr.span);
                    const fields: Record<string, Value> = Object.create(null) as Record<string, Value>;
                    for (const field of expr.fields) {
                        if (!field.fieldId)
                            this.fail('UNBOUND_FIELD', field.name, field.span);
                        fields[field.fieldId] = this.eval(field.value, env);
                    }
                    value = { $record: definition.id, fields };
                    this.validateRecord(value, definition, expr.span);
                    break;
                }
            }
            this.note(expr.kind, expr.kind === 'binary' || expr.kind === 'unary' ? expr.op : 'name' in expr ? expr.name : expr.kind, expr.span, value);
            return value;
        }
        finally {
            this.depth--;
        }
    }
    capture(operation: () => Value): Outcome {
        try {
            return freeze({ ok: true, value: operation(), trace: this.trace, steps: this.steps });
        }
        catch (error) {
            if (!(error instanceof ExecutionError))
                throw error;
            return freeze({ ok: false, error: { code: error.code, message: error.message, span: error.span }, trace: this.trace, steps: this.steps });
        }
    }
}
function program(compilation: Compilation): Program {
    if (!compilation.valid || !compilation.program)
        throw new Error('Execution requires a valid compilation.');
    return compilation.program;
}
/** Internal expression replay retains each expression's original source ID, even across revisions. */
export function evaluate(compilation: Compilation, expr: Expr, options: RunOptions = {}): Outcome {
    const executor = new Executor(program(compilation), runOptions(options));
    return executor.capture(() => executor.eval(expr));
}
export function invoke(compilation: Compilation, id: string, args: Value[], options: RunOptions = {}): Outcome {
    const p = program(compilation);
    const executor = new Executor(p, runOptions(options));
    return executor.capture(() => executor.invoke(id, structuredClone(args), p.span));
}
export function runScenario(compilation: Compilation, scenario: ScenarioDef, options: RunOptions = {}): ScenarioResult {
    const actual = evaluate(compilation, scenario.actual, options);
    const expected = evaluate(compilation, scenario.expected, options);
    return freeze({ id: scenario.id, name: scenario.name, status: !actual.ok || !expected.ok ? 'error' : equal(actual.value, expected.value) ? 'passed' : 'failed', actual, expected });
}
export function runScenarios(compilation: Compilation, options: RunOptions = {}): ScenarioResult[] {
    return program(compilation).definitions.filter((d): d is ScenarioDef => d.kind === 'scenario').map(scenario => runScenario(compilation, scenario, options));
}
export function outcomeSignature(outcome: Outcome): unknown { return outcome.ok ? { value: outcome.value } : { error: outcome.error.code, message: outcome.error.message }; }
/** Human-readable names are a view; canonical values retain stable record/field IDs. */
export function displayValue(compilation: Compilation, value: Value): unknown {
    if (typeof value !== 'object')
        return value;
    const def = compilation.program?.definitions.find(d => d.id === value.$record);
    return { type: def?.name ?? value.$record, fields: Object.fromEntries(Object.entries(value.fields).map(([id, v]) => [def?.kind === 'record' ? def.fields.find(f => f.id === id)?.name ?? id : id, displayValue(compilation, v)])) };
}
