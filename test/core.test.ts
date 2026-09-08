import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { compile, invoke, runScenarios, verify, compare, createProposal, previewChange, applyChange, readProposal, assessEvidence, inspectDefinition, inspectProject, MemoryStore, type Compilation, type RecordValue } from '../src/index.js';
const source = readFileSync('examples/inventory.intent', 'utf8');
const changedSource = readFileSync('examples/inventory.changed.intent', 'utf8');
const base = compile(source, 'inventory.intent');
const changed = compile(changedSource, 'inventory.intent');
const stock = (onHand: number, reserved: number): RecordValue => ({ $record: 'stock', fields: { 'stock.onHand': onHand, 'stock.reserved': reserved } });
function valid(text: string): Compilation { const c = compile(text, 'test.intent'); assert.equal(c.valid, true, JSON.stringify(c.diagnostics)); return c; }
function expr(expression: string, type = 'Int'): Compilation { return valid(`module Test @id("result") rule result() -> ${type} = ${expression}`); }
function value(c: Compilation, id = 'result', args: Parameters<typeof invoke>[2] = []): unknown { const out = invoke(c, id, args); assert.equal(out.ok, true, JSON.stringify(out)); return out.ok ? out.value : undefined; }
function invalid(text: string, code: string): void { const c = compile(text); assert.equal(c.valid, false); assert.ok(c.diagnostics.some(d => d.code === code), JSON.stringify(c.diagnostics)); }
test('baseline compiles with explicit stable field IDs and no diagnostics', () => { assert.equal(base.valid, true); assert.deepEqual(base.diagnostics, []); });
test('all four human-owned scenarios pass', () => { assert.equal(runScenarios(base).length, 4); assert.ok(runScenarios(base).every(s => s.status === 'passed')); });
test('arithmetic precedence is multiplication before addition', () => assert.equal(value(expr('2 + 3 * 4')), 14));
test('parentheses change grouping', () => assert.equal(value(expr('(2 + 3) * 4')), 20));
test('subtraction associates to the left', () => assert.equal(value(expr('10 - 3 - 2')), 5));
test('negative integer and negation semantics are explicit', () => { assert.equal(value(expr('-2 * 3')), -6); assert.equal(value(expr('not false', 'Bool')), true); });
test('boolean and has higher precedence than or', () => assert.equal(value(expr('true or false and false', 'Bool')), true));
test('strings are JSON-decoded without evaluating source', () => assert.equal(value(expr('"hello\\nworld"', 'String')), 'hello\nworld'));
test('if evaluates only the selected branch', () => assert.equal(value(expr('if false then 9007199254740991 + 1 else 2')), 2));
test('and short-circuits and records the skipped operand', () => { const c = expr('false and (9007199254740991 + 1 > 0)', 'Bool'); const result = invoke(c, 'result', []); assert.equal(result.ok && result.value, false); assert.ok(result.trace.some(s => s.kind === 'short-circuit')); });
test('or short-circuits', () => assert.equal(value(expr('true or (9007199254740991 + 1 > 0)', 'Bool')), true));
test('rule arguments are evaluated and nominal records validated', () => assert.equal(value(base, 'canReserve', [stock(10, 8), 2]), true));
test('a transition returns a new validated state without modifying inputs', () => { const input = stock(10, 2); const result = value(base, 'reserve', [input, 3]); assert.deepEqual(JSON.parse(JSON.stringify(result)), stock(10, 5)); assert.equal(input.fields['stock.reserved'], 2); assert.equal(Object.isFrozen(input), false); });
test('transition guard preserves the business rejection reason', () => { const r = invoke(base, 'reserve', [stock(10, 8), 3]); assert.equal(r.ok, false); if (!r.ok) {
    assert.equal(r.error.code, 'GUARD_REJECTED');
    assert.equal(r.error.message, 'INSUFFICIENT_STOCK');
} assert.ok(r.trace.some(s => s.kind === 'guard' && s.value === false)); });
test('runtime rejects zero for PositiveInt', () => { const r = invoke(base, 'canReserve', [stock(10, 0), 0]); assert.ok(!r.ok && r.error.code === 'REFINEMENT_VIOLATION'); });
test('runtime rejects negative fields', () => { const r = invoke(base, 'canReserve', [stock(-1, 0), 1]); assert.ok(!r.ok && r.error.code === 'REFINEMENT_VIOLATION'); });
test('record invariant rejects oversubscribed stock', () => { const r = invoke(base, 'canReserve', [stock(10, 11), 1]); assert.ok(!r.ok && r.error.code === 'INVARIANT_VIOLATION'); });
test('runtime rejects extra record fields', () => { const s = stock(10, 0); s.fields['unexpected'] = 0; const r = invoke(base, 'canReserve', [s, 1]); assert.ok(!r.ok && r.error.code === 'RECORD_SHAPE'); });
test('runtime rejects missing record fields', () => { const s = stock(10, 0); delete s.fields['stock.reserved']; const r = invoke(base, 'canReserve', [s, 1]); assert.ok(!r.ok && r.error.code === 'RECORD_SHAPE'); });
test('nominal record identity is enforced', () => { const s = stock(10, 0); s.$record = 'not-stock'; const r = invoke(base, 'canReserve', [s, 1]); assert.ok(!r.ok && r.error.code === 'TYPE_ERROR'); });
test('integer overflow fails instead of rounding', () => { const r = invoke(expr('9007199254740991 + 1'), 'result', []); assert.ok(!r.ok && r.error.code === 'INTEGER_RANGE'); });
test('number inputs cannot be NaN, Infinity, fractional, or unsafe', () => { for (const n of [NaN, Infinity, 1.5, 9007199254740992]) {
    const r = invoke(base, 'canReserve', [stock(10, 0), n]);
    assert.ok(!r.ok && r.error.code === 'INTEGER_RANGE');
} });
test('runtime refinement on a rule return is checked', () => { const r = invoke(expr('-1', 'PositiveInt'), 'result', []); assert.ok(!r.ok && r.error.code === 'REFINEMENT_VIOLATION'); });
test('execution step exhaustion is not presented as a successful result', () => { const r = invoke(base, 'canReserve', [stock(10, 2), 1], { maxSteps: 1 }); assert.ok(!r.ok && r.error.code === 'STEP_LIMIT'); });
test('trace exhaustion is explicit, not silently truncated', () => { const r = invoke(base, 'canReserve', [stock(10, 2), 1], { maxTrace: 1 }); assert.ok(!r.ok && r.error.code === 'TRACE_LIMIT'); });
test('depth exhaustion is explicit', () => { const r = invoke(expr('1 + 2'), 'result', [], { maxDepth: 1 }); assert.ok(!r.ok && r.error.code === 'DEPTH_LIMIT'); });
test('invalid execution limits are rejected', () => { for (const limit of [0, -1, 1.5, Infinity])
    assert.throws(() => invoke(base, 'canReserve', [stock(10, 0), 1], { maxSteps: limit })); });
test('every trace step retains source identity and valid source offsets', () => { const r = invoke(base, 'canReserve', [stock(10, 8), 2]); for (const s of r.trace) {
    assert.equal(s.span.sourceId, base.snapshot.sourceId);
    assert.ok(s.span.start >= 0 && s.span.end <= source.length);
} assert.ok(r.trace.some(s => s.kind === 'binary' && s.label === '+' && s.value === 10)); });
test('compile snapshots and IR are immutable', () => { assert.ok(Object.isFrozen(base)); assert.ok(Object.isFrozen(base.program?.definitions)); assert.throws(() => { base.program!.module = 'Mutated'; }, TypeError); });
test('unknown identifiers are static errors', () => invalid('module T @id("r") rule f() -> Int = missing', 'UNKNOWN_NAME'));
test('unknown record type is a static error', () => invalid('module T @id("r") rule f(x: Missing) -> Bool = true', 'UNKNOWN_TYPE'));
test('unknown fields are static errors', () => invalid(source.replace('stock.reserved + quantity', 'stock.missing + quantity'), 'UNKNOWN_FIELD'));
test('boolean/numeric coercion is rejected', () => invalid('module T @id("r") rule f() -> Bool = true == 1', 'TYPE_MISMATCH'));
test('string arithmetic is rejected', () => invalid('module T @id("r") rule f() -> String = "a" + "b"', 'TYPE_MISMATCH'));
test('wrong argument arity is a static error', () => invalid(source.replace('}, 2) == true', '}) == true'), 'ARGUMENT_COUNT'));
test('duplicate stable IDs are rejected', () => invalid(source.replace('@id("reserve")', '@id("canReserve")'), 'DUPLICATE_ID'));
test('duplicate names are rejected', () => invalid('module T @id("a") rule f() -> Int = 1 @id("b") rule f() -> Int = 2', 'DUPLICATE_NAME'));
test('duplicate parameters are rejected', () => invalid('module T @id("a") rule f(x: Int, x: Int) -> Int = x', 'DUPLICATE_PARAMETER'));
test('duplicate constructor fields are rejected', () => invalid(source.replace('Stock { onHand: 10, reserved: 8 }', 'Stock { onHand: 10, reserved: 8, reserved: 8 }'), 'DUPLICATE_ARGUMENT'));
test('missing constructor fields are rejected', () => invalid(source.replace('Stock { onHand: 10, reserved: 8 }', 'Stock { onHand: 10 }'), 'MISSING_FIELD'));
test('direct recursion is rejected', () => invalid('module T @id("a") rule f() -> Int = f()', 'RECURSION_NOT_SUPPORTED'));
test('mutual recursion is rejected', () => invalid('module T @id("a") rule f() -> Int = g() @id("b") rule g() -> Int = f()', 'RECURSION_NOT_SUPPORTED'));
test('recursive record types are rejected', () => invalid('module T @id("node") record Node { child: Node }', 'RECURSION_NOT_SUPPORTED'));
test('expected answers cannot call the implementation under test', () => invalid(source.replace('}, 2) == true', '}, 2) == canReserve(Stock { onHand: 10, reserved: 8 }, 2)'), 'CALL_NOT_ALLOWED'));
test('record invariants cannot call a rule that recursively validates the record', () => invalid(source.replace('invariant reserved <= onHand', 'invariant helper(onHand)') + '\n@id("helper") rule helper(n: Int) -> Bool = true', 'CALL_NOT_ALLOWED'));
test('a pure rule cannot hide transition invocation', () => invalid(source + '\n@id("hidden") rule hidden(s: Stock) -> Stock = reserve(s, 1)', 'TRANSITION_CALL_NOT_ALLOWED'));
test('unsafe integer literals are parse errors', () => invalid('module T @id("a") rule f() -> Int = 9007199254740992', 'PARSE_ERROR'));
test('malformed strings and unexpected characters produce located diagnostics', () => { for (const s of ['"unterminated', '1 / 2', '[]'])
    invalid(`module T @id("a") rule f() -> Int = ${s}`, 'PARSE_ERROR'); });
test('expression nesting is bounded before type checking', () => invalid(`module T @id("a") rule f() -> Int = ${'('.repeat(200)}1${')'.repeat(200)}`, 'PARSE_ERROR'));
test('source length limit is enforced', () => invalid('//'.padEnd(262145, 'x'), 'PARSE_ERROR'));
test('implicit field identities are visible as warnings', () => { const c = valid('module T @id("r") record R { count: Int }'); assert.ok(c.diagnostics.some(d => d.code === 'IMPLICIT_FIELD_ID' && d.severity === 'warning')); });
test('invalid compilations cannot execute', () => assert.throws(() => runScenarios(compile('not a program')), /valid compilation/));
test('inspection returns actual structure and honest deployment status', () => { const project = inspectProject(base) as {
    status: {
        deployment: string;
    };
}; assert.equal(project.status.deployment, 'not-connected'); const d = inspectDefinition(base, 'canReserve') as {
    dependencies: unknown[];
}; assert.ok(d.dependencies.length); assert.throws(() => inspectDefinition(base, 'missing')); });
test('whitespace and comments do not become semantic changes', () => { const spaced = compile('// comment\n' + source.replaceAll('\n', '\n\n'), 'inventory.intent'); const d = compare(base, spaced); assert.equal(d.changes.length, 0); assert.ok(d.baselineChecks.every(c => !c.actualChanged)); assert.notEqual(base.snapshot.id, spaced.snapshot.id); });
test('rule rename preserves identity, bound calls, and historical replay', () => { const renamed = compile(source.replaceAll('canReserve', 'reservable').replace('@id("reservable")', '@id("canReserve")'), 'inventory.intent'); assert.equal(renamed.valid, true); const report = compare(base, renamed); assert.equal(report.changes.length, 1); assert.equal(report.changes[0]!.kind, 'renamed'); assert.ok(report.baselineChecks.every(c => c.candidateStatus === 'passed')); });
test('field rename with explicit ID preserves historical constructor inputs', () => {
    // Rename identifier tokens only; preserve stable ID strings.
    const renamed = compile(source.replace(/"(?:[^"\\]|\\.)*"|\breserved\b/g, match => match === 'reserved' ? 'held' : match), 'inventory.intent');
    assert.equal(renamed.valid, true, JSON.stringify(renamed.diagnostics));
    const d = compare(base, renamed);
    assert.ok(d.changes.some(c => c.kind === 'field-renamed'));
    assert.ok(d.baselineChecks.every(c => c.candidateStatus === 'passed'));
});
test('policy change and expectation change remain separate facts', () => { const d = compare(base, changed); assert.ok(d.changes.some(c => c.category === 'policy')); assert.ok(d.changes.some(c => c.category === 'expectation')); });
test('changing the expected answer does not conceal the old failed expectation', () => { const d = compare(base, changed); assert.ok(d.candidateScenarios.every(s => s.status === 'passed')); assert.equal(d.baselineChecks.find(s => s.id === 'case.exact')!.candidateStatus, 'failed'); });
test('changed dependencies identify the transition and its scenarios as candidates', () => { const d = compare(base, changed); assert.ok(d.impacted.some(i => i.id === 'reserve')); assert.ok(d.impacted.some(i => i.id === 'case.partial')); });
test('deleting a test cannot delete baseline replay', () => { const without = compile(changedSource.replace(/@id\("case.exact"\)[\s\S]*?(?=@id\("case.excess"\))/, ''), 'inventory.intent'); const d = compare(base, without); assert.ok(d.changes.some(c => c.targetId === 'case.exact' && c.kind === 'definition-removed')); assert.equal(d.baselineChecks.find(c => c.id === 'case.exact')!.candidateStatus, 'failed'); });
test('editing scenario inputs is classified separately from expected outputs', () => { const c = compile(source.replace('}, 2) == true', '}, 1) == true'), 'inventory.intent'); const d = compare(base, c); assert.ok(d.changes.some(c => c.kind === 'scenario-input-changed')); assert.ok(!d.changes.some(c => c.kind === 'expectation-changed')); });
test('cross-version traces distinguish historical call sites from new definitions', () => { const d = compare(base, changed); const trace = d.baselineChecks[0]!.candidateActual.trace; assert.ok(trace.some(s => s.span.sourceId === base.snapshot.sourceId)); assert.ok(trace.some(s => s.span.sourceId === changed.snapshot.sourceId)); });
test('semantic comparison refuses invalid candidates', () => assert.throws(() => compare(base, compile('broken')), /valid compilations/));
test('verification is tied to exact source, build, runtime, and options', () => { const e = verify(base); assert.equal(e.passed, true); assert.equal(e.claims.formalProof, false); assert.equal(assessEvidence(base, e).current, true); assert.equal(assessEvidence(changed, e).current, false); assert.equal(assessEvidence(base, e, { maxSteps: 9999 }).current, false); });
test('corrupted verification output does not remain current', () => { const e = structuredClone(verify(base)); e.passed = false; assert.equal(assessEvidence(base, e).current, false); });
test('malformed evidence is rejected without claiming freshness', () => { for (const e of [null, [], {}, { schemaVersion: 100 }])
    assert.equal(assessEvidence(base, e).current, false); });
test('no scenarios is not presented as verification success', () => assert.equal(verify(expr('1')).passed, false));
test('human and assistant proposals go through the same deterministic preview', () => { for (const author of ['human', 'assistant'] as const) {
    const p = createProposal(base, changedSource, 'Keep one item.', author);
    const a = previewChange(base, p);
    const b = previewChange(base, p);
    assert.equal(a.id, b.id);
    assert.ok(a.applicable);
    assert.equal(applyChange(base, p, a.id).snapshot.source, changedSource);
} });
test('stale proposals are rejected even after a comment-only edit', () => { const p = createProposal(base, changedSource, 'policy'); assert.throws(() => previewChange(compile('// changed\n' + source, 'inventory.intent'), p), /STALE_PROPOSAL/); });
test('approval cannot be reused for a different preview', () => { const p = createProposal(base, changedSource, 'policy'); assert.throws(() => applyChange(base, p, 'not-the-reviewed-digest'), /REVIEW_MISMATCH/); });
test('invalid candidates can be previewed but not applied', () => { const p = createProposal(base, 'broken', 'proposal'); const preview = previewChange(base, p); assert.equal(preview.applicable, false); assert.throws(() => applyChange(base, p, preview.id), /INVALID_CANDIDATE/); });
test('proposal payload tampering is rejected', () => { const p = structuredClone(createProposal(base, changedSource, 'policy')); p.rationale = 'different'; assert.throws(() => readProposal(p), /checksum/); });
test('memory store rejects a stale write without modifying stored data', () => { const store = new MemoryStore(); store.create('one', stock(10, 0)); const a = store.read('one'); const b = store.read('one'); assert.deepEqual(store.compareAndSet('one', a.version, stock(10, 2)), { ok: true, version: 1 }); assert.deepEqual(store.compareAndSet('one', b.version, stock(10, 3)), { ok: false, currentVersion: 1 }); assert.equal(store.read('one').value.fields['stock.reserved'], 2); });
test('memory store reads do not expose mutable internal references', () => { const store = new MemoryStore(); store.create('one', stock(10, 0)); const read = store.read('one'); read.value.fields['stock.reserved'] = 99; assert.equal(store.read('one').value.fields['stock.reserved'], 0); assert.throws(() => store.create('one', stock(10, 0))); });
test('exhaustive inventory checks across 3,060 valid input combinations', () => {
    let count = 0;
    for (let onHand = 0; onHand <= 16; onHand++)
        for (let reserved = 0; reserved <= onHand; reserved++)
            for (let quantity = 1; quantity <= 20; quantity++) {
                const input = stock(onHand, reserved);
                const possible = quantity <= onHand - reserved;
                assert.equal(value(base, 'canReserve', [input, quantity]), possible);
                const result = invoke(base, 'reserve', [input, quantity]);
                if (possible) {
                    assert.equal(result.ok, true);
                    if (result.ok)
                        assert.deepEqual(JSON.parse(JSON.stringify(result.value)), stock(onHand, reserved + quantity));
                }
                else
                    assert.ok(!result.ok && result.error.code === 'GUARD_REJECTED');
                count++;
            }
    assert.equal(count, 3060);
});
test('Unicode strings do not shift subsequent UTF-16 diagnostic columns', () => {
    const text = 'module T @id("s") scenario "😀" { expect missing == true }';
    const compilation = compile(text, 'unicode.intent');
    const diagnostic = compilation.diagnostics.find(d => d.code === 'UNKNOWN_NAME');
    assert.ok(diagnostic);
    assert.equal(diagnostic.span.start, text.indexOf('missing'));
    assert.equal(diagnostic.span.column, text.indexOf('missing') + 1);
});
