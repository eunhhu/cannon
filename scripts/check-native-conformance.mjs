// Neither implementation authors expected results. The shared corpus is reviewed source.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { compile, evaluate } from '../dist/src/index.js';

const root = fileURLToPath(new URL('../', import.meta.url));
const args = process.argv.slice(2);
const referenceOnly = args.includes('--reference-only');
assert.ok(args.every(arg => arg === '--reference-only'), 'Unknown conformance option');
const native = resolve(root, 'target/debug', process.platform === 'win32' ? 'cannon-native.exe' : 'cannon-native');
const lines = readFileSync(resolve(root, 'conformance/expressions.tsv'), 'utf8').split('\n').filter(line => line && !line.startsWith('#'));
assert.ok(lines.length > 0, 'An empty conformance suite is not success');
const seen = new Set();
let compared = 0;
for (const line of lines) {
  const columns = line.split('\t');
  assert.equal(columns.length, 6, 'Malformed fixture');
  const [id, type, encodedSource, expectedKind, encodedExpected, encodedLimits] = columns;
  assert.ok(!seen.has(id), `Duplicate fixture ${id}`); seen.add(id);
  const source = JSON.parse(encodedSource);
  const expected = JSON.parse(encodedExpected);
  const limits = JSON.parse(encodedLimits);
  assert.ok(['Int', 'Bool', 'String'].includes(type));
  assert.ok(['value', 'compile-error', 'runtime-error'].includes(expectedKind));
  assert.equal(typeof source, 'string');
  const prefix = `module Conformance\n@id("fixture") rule fixture() -> ${type} = `;
  const compilation = compile(prefix + source + '\n', `conformance:${id}`);
  let reference;
  if (!compilation.valid) {
    assert.equal(expectedKind, 'compile-error', `${id}: unexpected reference compile error`);
    assert.ok(compilation.diagnostics.some(d => d.code === expected), `${id}: expected ${expected}`);
    reference = { kind: 'compile-error', code: expected };
  } else {
    const rule = compilation.program.definitions[0];
    assert.equal(rule.kind, 'rule');
    const run = evaluate(compilation, rule.body, limits);
    reference = { kind: run.ok ? 'value' : 'runtime-error', run };
    assert.equal(reference.kind, expectedKind, `${id}: reference phase`);
    if (run.ok) assert.deepEqual(JSON.parse(JSON.stringify(run.value)), expected, `${id}: independent expected value`);
    else assert.equal(run.error.code, expected, `${id}: independent expected error`);
  }
  if (referenceOnly) continue;
  const nativeArgs = ['eval', '--stdin', '--json'];
  for (const [key, value] of Object.entries(limits)) {
    const flag = { maxSteps: '--max-steps', maxDepth: '--max-depth', maxTrace: '--max-trace' }[key];
    assert.ok(flag); nativeArgs.push(flag, String(value));
  }
  const processResult = spawnSync(native, nativeArgs, { input: source, encoding: 'utf8', timeout: 10000, maxBuffer: 4 * 1024 * 1024 });
  assert.ifError(processResult.error);
  assert.equal(processResult.signal, null, `${id}: native terminated by signal`);
  assert.equal(processResult.status, expectedKind === 'value' ? 0 : expectedKind === 'compile-error' ? 2 : 1, `${id}: native exit code: ${processResult.stderr}`);
  const actual = JSON.parse(processResult.stdout);
  assert.equal(actual.schema, 'cannon.native.expression/1');
  assert.equal(actual.engine.implementation, 'rust');
  assert.deepEqual(actual.source, { uri: 'stdin.expression', text: source });
  assert.deepEqual(actual.limits, { maxSteps: 10000, maxDepth: 128, maxTrace: 10000, ...limits });
  assert.equal(actual.ok, expectedKind === 'value', `${id}: native outcome`);
  assert.equal(actual.phase, expectedKind === 'compile-error' ? 'compilation' : 'execution', `${id}: native phase`);
  if (actual.ok) { assert.deepEqual(actual.value, expected, `${id}: native independent value`); assert.equal(actual.type, type); }
  else assert.equal(actual.error.code, expected, `${id}: native independent error`);
  if (reference.kind === 'compile-error') { assert.deepEqual(actual.trace, []); assert.equal(actual.steps, 0); continue; }
  const prefixLines = prefix.split('\n');
  const span = original => ({
    start: original.start - prefix.length, end: original.end - prefix.length,
    line: original.line - prefixLines.length + 1,
    column: original.line === prefixLines.length ? original.column - prefixLines.at(-1).length : original.column,
  });
  const trace = reference.run.trace.map(step => ({
    index: step.index, kind: step.kind, label: step.label, span: span(step.span),
    ...(Object.hasOwn(step, 'value') ? { value: step.value } : {}),
  }));
  // JSON normalization is explicit: JS -0 is 0 on the wire. No branch/value/step is dropped.
  assert.deepEqual(actual.trace, JSON.parse(JSON.stringify(trace)), `${id}: trace and UTF-16 origin parity`);
  assert.equal(actual.steps, reference.run.steps, `${id}: executed step parity`);
  if (!actual.ok) assert.deepEqual(actual.error.span, span(reference.run.error.span), `${id}: error origin parity`);
  compared++;
}
console.log(`${lines.length} independent expression fixtures passed against ${referenceOnly ? 'TypeScript reference' : 'TypeScript and Rust'}; ${compared} executed trace comparisons.`);
