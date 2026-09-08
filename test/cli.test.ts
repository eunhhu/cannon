import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, writeFile, rm, symlink, readdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { createProposal, previewChange } from '../src/index.js';
import { loadFile, applyFile, writeNewJson } from '../src/filesystem.js';
const cli = resolve('dist/src/cli.js');
const run = (...args: string[]) => spawnSync(process.execPath, [cli, ...args], { encoding: 'utf8', timeout: 20000 });
const file = resolve('examples/inventory.intent');
test('CLI demo completes and exposes hidden expectation changes', () => { const result = run('demo'); assert.equal(result.status, 0, result.stderr); assert.match(result.stdout, /Candidate scenarios: 4\/4 passed/); assert.match(result.stdout, /FAILED/); assert.match(result.stdout, /STALE_PROPOSAL/); });
test('CLI check is machine-readable', () => { const result = run('check', file, '--json'); assert.equal(result.status, 0); assert.equal(JSON.parse(result.stdout).valid, true); });
test('CLI run emits a full versioned report', () => { const result = run('run', file, '--json'); assert.equal(result.status, 0); const report = JSON.parse(result.stdout); assert.equal(report.results.length, 4); assert.equal(report.passed, true); });
test('CLI regression gate fails even when candidate tests pass', () => { const result = run('diff', file, 'examples/inventory.changed.intent', '--fail-on-regression'); assert.equal(result.status, 1, result.stderr); });
test('CLI failed unchanged expectations yield exit code 1', () => { const result = run('run', 'examples/inventory.regression.intent'); assert.equal(result.status, 1); });
test('CLI unknown commands and irrelevant options are not silently accepted', () => { for (const args of [['nope'], ['check', file, '--surprise'], ['check', file, '--reason', 'ignored'], ['check', file, file]])
    assert.equal(run(...args).status, 2); });
test('CLI handles missing files without a stack trace', () => { const result = run('check', '/not/a/real/intent-file'); assert.equal(result.status, 2); assert.doesNotMatch(result.stderr, /at main/); });
test('filesystem apply writes exactly the reviewed source and cleans its lock', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'intent-test-'));
    try {
        const path = join(dir, 'sample.intent');
        const source = await readFile(file, 'utf8');
        await writeFile(path, source);
        const base = await loadFile(path);
        const p = createProposal(base, source + '\n// reviewed change\n', 'test');
        const preview = previewChange(base, p);
        await applyFile(path, p, preview.id);
        assert.equal(await readFile(path, 'utf8'), p.candidateSource);
        assert.deepEqual(await readdir(dir), ['sample.intent']);
    }
    finally {
        await rm(dir, { recursive: true, force: true });
    }
});
test('filesystem stale apply does not overwrite newer edits', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'intent-test-'));
    try {
        const path = join(dir, 'sample.intent');
        const source = await readFile(file, 'utf8');
        await writeFile(path, source);
        const base = await loadFile(path);
        const p = createProposal(base, source + '\n// proposal', 'test');
        const preview = previewChange(base, p);
        await writeFile(path, source + '\n// another editor');
        await assert.rejects(applyFile(path, p, preview.id), /STALE_PROPOSAL/);
        assert.equal(await readFile(path, 'utf8'), source + '\n// another editor');
        assert.deepEqual(await readdir(dir), ['sample.intent']);
    }
    finally {
        await rm(dir, { recursive: true, force: true });
    }
});
test('filesystem lock does not get removed by a second failed writer', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'intent-test-'));
    try {
        const path = join(dir, 'sample.intent');
        await writeFile(path, await readFile(file, 'utf8'));
        const base = await loadFile(path);
        const p = createProposal(base, base.snapshot.source + '\n', 'test');
        const preview = previewChange(base, p);
        await writeFile(path + '.intent-lock', 'existing lock');
        await assert.rejects(applyFile(path, p, preview.id), /EEXIST/);
        assert.equal(await readFile(path + '.intent-lock', 'utf8'), 'existing lock');
    }
    finally {
        await rm(dir, { recursive: true, force: true });
    }
});
test('filesystem refuses source symlinks', { skip: process.platform === 'win32' }, async () => {
    const dir = await mkdtemp(join(tmpdir(), 'intent-test-'));
    try {
        const original = join(dir, 'original.intent');
        const path = join(dir, 'linked.intent');
        await writeFile(original, await readFile(file, 'utf8'));
        await symlink(original, path);
        const base = await loadFile(path);
        const p = createProposal(base, base.snapshot.source + '\n', 'test');
        const preview = previewChange(base, p);
        await assert.rejects(applyFile(path, p, preview.id), /non-symlink/);
    }
    finally {
        await rm(dir, { recursive: true, force: true });
    }
});
test('writing reports never silently overwrites existing files', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'intent-test-'));
    try {
        const path = join(dir, 'evidence.json');
        await writeNewJson(path, { one: 1 });
        await assert.rejects(writeNewJson(path, { two: 2 }), /EEXIST/);
        assert.deepEqual(JSON.parse(await readFile(path, 'utf8')), { one: 1 });
    }
    finally {
        await rm(dir, { recursive: true, force: true });
    }
});
test('CLI proposal / preview / apply completes an end-to-end review flow', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'intent-test-'));
    try {
        const current = join(dir, 'current.intent');
        const proposal = join(dir, 'proposal.json');
        await writeFile(current, await readFile(file, 'utf8'));
        const create = run('propose', current, 'examples/inventory.changed.intent', '--reason', 'Keep one item', '--author', 'assistant', '--out', proposal);
        assert.equal(create.status, 0, create.stderr);
        const preview = run('preview', current, proposal, '--json');
        assert.equal(preview.status, 0, preview.stderr);
        const report = JSON.parse(preview.stdout);
        assert.equal(report.applicable, true);
        const apply = run('apply', current, proposal, '--review', report.id);
        assert.equal(apply.status, 0, apply.stderr);
        assert.equal(await readFile(current, 'utf8'), await readFile('examples/inventory.changed.intent', 'utf8'));
        assert.equal(run('preview', current, proposal).status, 2);
    }
    finally {
        await rm(dir, { recursive: true, force: true });
    }
});

// Keep the public prototype identity separate from the .intent language format.
test('CLI help uses the Cannon prototype name', () => {
    const result = spawnSync(process.execPath, [resolve('dist/src/cli.js'), '--help'], { encoding: 'utf8' });
    assert.equal(result.status, 0);
    assert.match(result.stdout, /Cannon 0\.1\.0/);
    assert.match(result.stdout, /cannon diff/);
    assert.ok(!result.stdout.includes('Intent Workbench'));
});
