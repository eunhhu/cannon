// Independent expectations first; differential comparison is additional evidence.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { compile } from '../dist/src/core/compile.js';
import { invoke } from '../dist/src/core/execution.js';
const referenceOnly = process.argv.slice(2).join(' ') === '--reference-only';
if (process.argv.length > 2 && !referenceOnly) throw new Error('Unknown options.');
const fixtures = JSON.parse(readFileSync(new URL('../conformance/bound-policies.json', import.meta.url), 'utf8'));
const exe = resolve('target/debug/cannon-policy' + (process.platform === 'win32' ? '.exe' : ''));
const ids = new Set(); let traceChecks = 0;
const normal = x => JSON.parse(JSON.stringify(x));
for (const f of fixtures) {
  assert.ok(!ids.has(f.id), `Duplicate fixture ${f.id}`); ids.add(f.id);
  const prefix = `module Bound\n@id("bound.rule") rule policy(${f.inputs.map(x => `${x.name}: ${x.type}`).join(', ')}) -> ${f.returns} =\n`;
  const compilation = compile(prefix + f.source + '\n', `fixture:${f.id}`);
  let expectedResult;
  if (f.phase === 'compilation') {
    assert.equal(compilation.valid, false, f.id);
    assert.ok(compilation.diagnostics.some(d => d.code === f.expected.error), f.id);
  } else {
    assert.equal(compilation.valid, true, JSON.stringify(compilation.diagnostics));
    expectedResult = invoke(compilation, 'bound.rule', f.inputs.map(x => x.value));
    if (Object.hasOwn(f.expected, 'value')) {
      assert.equal(expectedResult.ok, true, f.id);
      assert.deepEqual(normal(expectedResult.value), f.expected.value, f.id);
    } else {
      assert.equal(expectedResult.ok, false, f.id);
      assert.equal(expectedResult.error.code, f.expected.error, f.id);
    }
  }
  if (referenceOnly) continue;
  const args = ['eval', f.source, ...f.inputs.flatMap(x => ['--input', `${x.id}:${x.name}:${x.type}=${JSON.stringify(x.value)}`]), '--json'];
  const processResult = spawnSync(exe, args, { encoding: 'utf8', timeout: 10000, maxBuffer: 4 * 1024 * 1024 });
  assert.ifError(processResult.error);
  const report = JSON.parse(processResult.stdout); const actual = report.execution;
  assert.equal(report.schema, 'cannon.native.bound-expression/1', f.id);
  assert.equal(actual.source.text, f.source, f.id); assert.equal(actual.phase, f.phase, f.id);
  assert.deepEqual(report.inputs, f.inputs.map(({id,name,type}) => ({id,name,type})), f.id);
  assert.deepEqual(report.bindings, Object.fromEntries(f.inputs.map(x => [x.id, x.value])), f.id);
  if (Object.hasOwn(f.expected, 'value')) {
    assert.equal(processResult.status, 0, f.id); assert.equal(actual.ok, true, f.id);
    assert.deepEqual(actual.value, f.expected.value, f.id);
  } else {
    assert.equal(processResult.status, f.phase === 'compilation' ? 2 : 1, f.id);
    assert.equal(actual.ok, false, f.id); assert.equal(actual.error.code, f.expected.error, f.id);
  }
  // Input-boundary accounting is a separate API contract. For evaluated bodies,
  // normalize only the known reference call wrapper and ref/input terminology.
  if (expectedResult && (expectedResult.ok || f.expected.error === 'INTEGER_RANGE')) {
    const trace = [...expectedResult.trace];
    if (expectedResult.ok) { assert.equal(trace.at(-1).kind, 'return'); trace.pop(); }
    const lines = (prefix.match(/\n/g) ?? []).length;
    const adjusted = trace.map((step, index) => ({ index,
      kind: step.kind === 'ref' ? 'input' : step.kind,
      label: step.kind === 'ref' ? 'input' : step.label,
      span: {start: step.span.start - prefix.length, end: step.span.end - prefix.length,
        line: step.span.line - lines, column: step.span.column},
      ...(Object.hasOwn(step, 'value') ? {value: step.value} : {}) }));
    assert.deepEqual(actual.trace, normal(adjusted), `${f.id}: complete body trace`);
    const wrapperSteps = 1 + f.inputs.length + (expectedResult.ok ? 1 : 0);
    assert.equal(actual.steps, expectedResult.steps - wrapperSteps, `${f.id}: body steps`);
    for (const ref of report.inputReferences) {
      assert.ok(f.inputs.some(x => x.id === ref.id && x.name === ref.name), f.id);
      assert.equal(f.source.slice(ref.span.start, ref.span.end), ref.name, f.id);
    }
    traceChecks++;
  }
}
if (!referenceOnly) {
  const result = spawnSync(exe, ['demo', '--json'], {encoding: 'utf8', timeout: 10000});
  assert.ifError(result.error); assert.equal(result.status, 0);
  const demo = JSON.parse(result.stdout);
  assert.equal(demo.candidateStatus, 'passed'); assert.equal(demo.historicalStatus, 'failed');
  assert.equal(demo.passed, false); assert.deepEqual(demo.regressions, ['case.exact']);
  assert.equal(demo.candidateResults[0].expected, false); assert.equal(demo.historicalResults[0].expected, true);
  assert.equal(demo.historicalResults[0].execution.source.text, demo.candidate.source);
}
console.log(`${fixtures.length} independent bound-policy fixtures passed against ${referenceOnly ? 'TypeScript only (native NOT checked)' : `TypeScript and Rust; ${traceChecks} complete body-trace comparisons; native historical-review demo checked`}.`);
