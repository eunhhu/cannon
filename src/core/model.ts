/** One semantic model is shared by execution, review, CLI, and assistant integrations. */
export interface Span {
    sourceId: string;
    uri: string;
    start: number;
    end: number;
    line: number;
    column: number;
}
export type Value = number | boolean | string | RecordValue;
export interface RecordValue {
    $record: string;
    fields: {
        [fieldId: string]: Value;
    };
}
export interface Parameter {
    name: string;
    type: string;
    span: Span;
}
export interface Field extends Parameter {
    id: string;
    explicitId: boolean;
}
export type Expr = {
    kind: 'literal';
    value: number | boolean | string;
    span: Span;
} | {
    kind: 'ref';
    name: string;
    fieldId?: string;
    span: Span;
} | {
    kind: 'field';
    object: Expr;
    name: string;
    fieldId?: string;
    span: Span;
} | {
    kind: 'unary';
    op: '-' | 'not';
    operand: Expr;
    span: Span;
} | {
    kind: 'binary';
    op: string;
    left: Expr;
    right: Expr;
    span: Span;
} | {
    kind: 'if';
    condition: Expr;
    then: Expr;
    otherwise: Expr;
    span: Span;
} | {
    kind: 'call';
    name: string;
    targetId?: string;
    args: Expr[];
    span: Span;
} | {
    kind: 'record';
    name: string;
    targetId?: string;
    fields: {
        name: string;
        fieldId?: string;
        value: Expr;
        span: Span;
    }[];
    span: Span;
};
interface Base {
    id: string;
    name: string;
    span: Span;
}
export interface RecordDef extends Base {
    kind: 'record';
    fields: Field[];
    invariants: Expr[];
}
export interface RuleDef extends Base {
    kind: 'rule';
    params: Parameter[];
    returns: string;
    body: Expr;
}
export interface TransitionDef extends Base {
    kind: 'transition';
    params: Parameter[];
    returns: string;
    guards: {
        condition: Expr;
        error: string;
        span: Span;
    }[];
    next: Expr;
}
export interface ScenarioDef extends Base {
    kind: 'scenario';
    actual: Expr;
    expected: Expr;
}
export type Definition = RecordDef | RuleDef | TransitionDef | ScenarioDef;
export interface Program {
    module: string;
    definitions: Definition[];
    span: Span;
}
export interface Diagnostic {
    code: string;
    message: string;
    span: Span;
    severity: 'error' | 'warning';
}
export interface Dependency {
    from: string;
    to: string;
    reason: 'type' | 'call' | 'field' | 'construct' | 'invariant';
}
export interface Snapshot {
    id: string;
    sourceId: string;
    uri: string;
    source: string;
    engineDigest: string;
    engineVersion: string;
}
export interface Compilation {
    snapshot: Snapshot;
    program?: Program;
    diagnostics: Diagnostic[];
    dependencies: Dependency[];
    valid: boolean;
}
export interface TraceStep {
    index: number;
    kind: string;
    label: string;
    span: Span;
    value?: Value;
    detail?: string;
}
export type Outcome = {
    ok: true;
    value: Value;
    trace: TraceStep[];
    steps: number;
} | {
    ok: false;
    error: {
        code: string;
        message: string;
        span: Span;
    };
    trace: TraceStep[];
    steps: number;
};
export interface RunOptions {
    maxSteps?: number;
    maxDepth?: number;
    maxTrace?: number;
}
export interface ScenarioResult {
    id: string;
    name: string;
    status: 'passed' | 'failed' | 'error';
    actual: Outcome;
    expected: Outcome;
}
export const BUILTINS = ['Int', 'NonNegativeInt', 'PositiveInt', 'Bool', 'String'] as const;
export function children(expr: Expr): Expr[] {
    switch (expr.kind) {
        case 'field': return [expr.object];
        case 'unary': return [expr.operand];
        case 'binary': return [expr.left, expr.right];
        case 'if': return [expr.condition, expr.then, expr.otherwise];
        case 'call': return expr.args;
        case 'record': return expr.fields.map(f => f.value);
        default: return [];
    }
}
export function expressions(def: Definition): Expr[] {
    switch (def.kind) {
        case 'record': return def.invariants;
        case 'rule': return [def.body];
        case 'transition': return [...def.guards.map(g => g.condition), def.next];
        case 'scenario': return [def.actual, def.expected];
    }
}
export function walk(expr: Expr, visit: (e: Expr) => void): void {
    const stack = [expr];
    while (stack.length) {
        const e = stack.pop()!;
        visit(e);
        stack.push(...children(e));
    }
}
export function isRecord(value: Value): value is RecordValue { return typeof value === 'object'; }
