import { BUILTINS, type Dependency, type Diagnostic, type Expr, type Program, type Definition, type Span, type Field, type Parameter } from './model.js';
const numeric = (type: string): boolean => ['Int', 'NonNegativeInt', 'PositiveInt'].includes(type);
const compatible = (actual: string, expected: string): boolean => actual === '?' || expected === '?' || actual === expected || (numeric(actual) && numeric(expected));
/** Numeric refinement obligations are checked at runtime boundaries, not claimed as proofs. */
export function check(program: Program): {
    diagnostics: Diagnostic[];
    dependencies: Dependency[];
} {
    const diagnostics: Diagnostic[] = [];
    const dependencies: Dependency[] = [];
    const byName = new Map(program.definitions.filter(d => d.kind !== 'scenario').map(d => [d.name, d]));
    const error = (code: string, message: string, span: Span): void => { diagnostics.push({ code, message, span, severity: 'error' }); };
    const names = new Set<string>();
    const ids = new Set<string>();
    const register = (id: string, span: Span): void => { if (ids.has(id))
        error('DUPLICATE_ID', `Duplicate stable ID ${id}.`, span); ids.add(id); };
    for (const def of program.definitions) {
        register(def.id, def.span);
        if (def.kind !== 'scenario') {
            if (names.has(def.name) || (BUILTINS as readonly string[]).includes(def.name))
                error('DUPLICATE_NAME', `Name ${def.name} is already defined or reserved.`, def.span);
            names.add(def.name);
        }
        if (def.kind === 'record') {
            const seen = new Set<string>();
            for (const field of def.fields) {
                register(field.id, field.span);
                if (seen.has(field.name))
                    error('DUPLICATE_FIELD', `Duplicate field ${field.name}.`, field.span);
                seen.add(field.name);
                dependencies.push({ from: def.id, to: field.id, reason: 'field' });
                if (!field.explicitId)
                    diagnostics.push({ code: 'IMPLICIT_FIELD_ID', message: `Field ${def.name}.${field.name} has a name-derived ID; add @id before renaming it.`, span: field.span, severity: 'warning' });
            }
        }
    }
    function typeExists(type: string, span: Span, owner: string): void {
        if ((BUILTINS as readonly string[]).includes(type))
            return;
        const target = byName.get(type);
        if (!target || target.kind !== 'record')
            error('UNKNOWN_TYPE', `Unknown record type ${type}.`, span);
        else
            dependencies.push({ from: owner, to: target.id, reason: 'type' });
    }
    function ensure(actual: string, expected: string, span: Span): void {
        if (!compatible(actual, expected))
            error('TYPE_MISMATCH', `Expected ${expected}, received ${actual}.`, span);
    }
    type Env = Map<string, {
        type: string;
        fieldId?: string;
    }>;
    type Mode = 'rule' | 'transition' | 'invariant' | 'actual' | 'expected';
    function infer(expr: Expr, env: Env, owner: Definition, mode: Mode): string {
        switch (expr.kind) {
            case 'literal': return typeof expr.value === 'boolean' ? 'Bool' : typeof expr.value === 'string' ? 'String' : expr.value > 0 ? 'PositiveInt' : expr.value === 0 ? 'NonNegativeInt' : 'Int';
            case 'ref': {
                const binding = env.get(expr.name);
                if (!binding) {
                    error('UNKNOWN_NAME', `Unknown name ${expr.name}.`, expr.span);
                    return '?';
                }
                if (binding.fieldId) {
                    expr.fieldId = binding.fieldId;
                    dependencies.push({ from: owner.id, to: binding.fieldId, reason: 'field' });
                }
                return binding.type;
            }
            case 'field': {
                const objectType = infer(expr.object, env, owner, mode);
                const record = byName.get(objectType);
                const field = record?.kind === 'record' ? record.fields.find(f => f.name === expr.name) : undefined;
                if (!field) {
                    error('UNKNOWN_FIELD', `${objectType} has no field ${expr.name}.`, expr.span);
                    return '?';
                }
                expr.fieldId = field.id;
                dependencies.push({ from: owner.id, to: field.id, reason: 'field' });
                return field.type;
            }
            case 'unary': {
                const value = infer(expr.operand, env, owner, mode);
                ensure(value, expr.op === '-' ? 'Int' : 'Bool', expr.span);
                return expr.op === '-' ? 'Int' : 'Bool';
            }
            case 'binary': {
                const left = infer(expr.left, env, owner, mode);
                const right = infer(expr.right, env, owner, mode);
                if (['+', '-', '*'].includes(expr.op)) {
                    ensure(left, 'Int', expr.left.span);
                    ensure(right, 'Int', expr.right.span);
                    return 'Int';
                }
                if (['and', 'or'].includes(expr.op)) {
                    ensure(left, 'Bool', expr.left.span);
                    ensure(right, 'Bool', expr.right.span);
                    return 'Bool';
                }
                if (['<', '<=', '>', '>='].includes(expr.op)) {
                    ensure(left, 'Int', expr.left.span);
                    ensure(right, 'Int', expr.right.span);
                    return 'Bool';
                }
                ensure(left, right, expr.span);
                return 'Bool';
            }
            case 'if': {
                ensure(infer(expr.condition, env, owner, mode), 'Bool', expr.condition.span);
                const then = infer(expr.then, env, owner, mode);
                const otherwise = infer(expr.otherwise, env, owner, mode);
                ensure(then, otherwise, expr.span);
                return numeric(then) && numeric(otherwise) ? 'Int' : then;
            }
            case 'call': {
                const target = byName.get(expr.name);
                const argTypes = expr.args.map(arg => infer(arg, env, owner, mode));
                if (!target || (target.kind !== 'rule' && target.kind !== 'transition')) {
                    error('NOT_CALLABLE', `${expr.name} is not a rule or transition.`, expr.span);
                    return '?';
                }
                expr.targetId = target.id;
                dependencies.push({ from: owner.id, to: target.id, reason: 'call' });
                if (mode === 'invariant' || mode === 'expected')
                    error('CALL_NOT_ALLOWED', `${mode} expressions may not call rules or transitions.`, expr.span);
                if (target.kind === 'transition' && mode !== 'actual')
                    error('TRANSITION_CALL_NOT_ALLOWED', 'Transitions may only be invoked by a scenario or the host API, not composed inside rules.', expr.span);
                if (argTypes.length !== target.params.length)
                    error('ARGUMENT_COUNT', `${target.name} expects ${target.params.length} arguments; received ${argTypes.length}.`, expr.span);
                target.params.forEach((param, index) => { if (argTypes[index])
                    ensure(argTypes[index]!, param.type, expr.args[index]!.span); });
                return target.returns;
            }
            case 'record': {
                const target = byName.get(expr.name);
                if (!target || target.kind !== 'record') {
                    error('UNKNOWN_RECORD', `Unknown record ${expr.name}.`, expr.span);
                    expr.fields.forEach(f => infer(f.value, env, owner, mode));
                    return '?';
                }
                expr.targetId = target.id;
                dependencies.push({ from: owner.id, to: target.id, reason: 'construct' });
                if (mode === 'invariant')
                    error('CONSTRUCTION_NOT_ALLOWED', 'Invariants may inspect data but not construct records.', expr.span);
                const seen = new Set<string>();
                for (const field of expr.fields) {
                    if (seen.has(field.name))
                        error('DUPLICATE_ARGUMENT', `Duplicate field value ${field.name}.`, field.span);
                    seen.add(field.name);
                    const definition = target.fields.find(f => f.name === field.name);
                    const actual = infer(field.value, env, owner, mode);
                    if (!definition)
                        error('UNKNOWN_FIELD', `${target.name} has no field ${field.name}.`, field.span);
                    else {
                        field.fieldId = definition.id;
                        ensure(actual, definition.type, field.span);
                    }
                }
                for (const field of target.fields)
                    if (!seen.has(field.name))
                        error('MISSING_FIELD', `Missing field ${target.name}.${field.name}.`, expr.span);
                return target.name;
            }
        }
    }
    function environment(params: (Parameter | Field)[], owner: Definition): Env {
        const env: Env = new Map();
        for (const param of params) {
            if (env.has(param.name))
                error('DUPLICATE_PARAMETER', `Duplicate binding ${param.name}.`, param.span);
            typeExists(param.type, param.span, owner.id);
            env.set(param.name, 'id' in param ? { type: param.type, fieldId: param.id } : { type: param.type });
        }
        return env;
    }
    for (const def of program.definitions) {
        if (def.kind === 'record') {
            const env = environment(def.fields, def);
            for (const invariant of def.invariants)
                ensure(infer(invariant, env, def, 'invariant'), 'Bool', invariant.span);
        }
        else if (def.kind === 'scenario') {
            const actual = infer(def.actual, new Map(), def, 'actual');
            const expected = infer(def.expected, new Map(), def, 'expected');
            ensure(actual, expected, def.span);
        }
        else {
            const env = environment(def.params, def);
            typeExists(def.returns, def.span, def.id);
            if (def.kind === 'rule')
                ensure(infer(def.body, env, def, 'rule'), def.returns, def.body.span);
            else {
                for (const guard of def.guards)
                    ensure(infer(guard.condition, env, def, 'transition'), 'Bool', guard.condition.span);
                ensure(infer(def.next, env, def, 'transition'), def.returns, def.next.span);
            }
        }
    }
    // Reject recursive record types and recursive call graphs in this deliberately bounded v0 language.
    const active = new Set<string>();
    const complete = new Set<string>();
    const byId = new Map(program.definitions.map(d => [d.id, d]));
    function cycle(id: string): void {
        if (complete.has(id))
            return;
        if (active.has(id)) {
            error('RECURSION_NOT_SUPPORTED', `Recursive dependency involving ${byId.get(id)?.name ?? id}.`, byId.get(id)?.span ?? program.span);
            return;
        }
        active.add(id);
        for (const dependency of dependencies)
            if (dependency.from === id && ['call', 'type'].includes(dependency.reason) && byId.has(dependency.to))
                cycle(dependency.to);
        active.delete(id);
        complete.add(id);
    }
    for (const def of program.definitions)
        cycle(def.id);
    const unique = [...new Map(dependencies.map(d => [`${d.from}:${d.to}:${d.reason}`, d])).values()];
    return { diagnostics, dependencies: unique };
}
