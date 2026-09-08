#!/usr/bin/env node
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compile, inspectProject, verify, compare, createProposal, previewChange, readProposal, assessEvidence, displayValue, MemoryStore, type Compilation, type ChangeReport, type RecordValue } from './index.js';
import { loadFile, writeNewJson, applyFile } from './filesystem.js';
import { errorMessage } from './core/util.js';
const help = `Cannon 0.1.0 — executable intent and inspectable changes

  cannon check <file> [--json]
  cannon inspect <file>
  cannon run <file> [--json] [--evidence <new.json>] [--trace <scenario-id>]
  cannon diff <before> <after> [--json] [--out <new.json>] [--fail-on-regression]
  cannon evidence <file> <report.json>
  cannon propose <current> <candidate> --reason <text> --out <new.json> [--author assistant]
  cannon preview <current> <proposal.json> [--json] [--out <new.json>]
  cannon apply <current> <proposal.json> --review <preview-id>
  cannon demo

Exit codes: 0 completed/passed, 1 failed scenarios or requested regression gate, 2 invalid input/error.
No command sends source to an AI provider. 'apply' is the only command that edits source.
JSON output and evidence may contain input data; treat them as potentially sensitive.`;
function options(args: string[]): {
    positional: string[];
    flags: Map<string, string | true>;
} {
    const positional: string[] = [];
    const flags = new Map<string, string | true>();
    const booleans = new Set(['--json', '--fail-on-regression']);
    const valued = new Set(['--out', '--evidence', '--trace', '--reason', '--author', '--review']);
    for (let i = 0; i < args.length; i++) {
        const arg = args[i]!;
        if (!arg.startsWith('--')) {
            positional.push(arg);
            continue;
        }
        if (flags.has(arg))
            throw new Error(`Duplicate option ${arg}.`);
        if (booleans.has(arg))
            flags.set(arg, true);
        else if (valued.has(arg)) {
            const value = args[++i];
            if (!value || value.startsWith('--'))
                throw new Error(`Missing value for ${arg}.`);
            flags.set(arg, value);
        }
        else
            throw new Error(`Unknown option ${arg}.`);
    }
    return { positional, flags };
}
function json(value: unknown): void { console.log(JSON.stringify(value, null, 2)); }
function checked(compilation: Compilation): Compilation {
    if (!compilation.valid)
        throw new Error(compilation.diagnostics.filter(d => d.severity === 'error').map(d => `${d.span.uri}:${d.span.line}:${d.span.column} ${d.code}: ${d.message}`).join('\n'));
    return compilation;
}
function summary(report: ChangeReport): void {
    console.log(`Snapshots: ${report.before.id.slice(0, 12)} → ${report.after.id.slice(0, 12)}`);
    for (const change of report.changes)
        console.log(`  [${change.category}] ${change.kind}: ${change.label}`);
    if (!report.changes.length)
        console.log('  No structural changes (formatting and locations are ignored).');
    console.log(`Impact candidates: ${report.impacted.map(i => i.name).join(', ') || 'none'}`);
    console.log(`Candidate scenarios: ${report.candidateScenarios.filter(r => r.status === 'passed').length}/${report.candidateScenarios.length} passed`);
    console.log('Historical expectations replayed against the candidate:');
    for (const baseline of report.baselineChecks)
        console.log(`  ${baseline.candidateStatus.toUpperCase()} ${baseline.name}${baseline.actualChanged ? ' — actual outcome changed' : ''}`);
    console.log('This is scoped execution evidence, not a proof of equivalence or deployment status.');
}
async function demo(): Promise<number> {
    const baselineFile = fileURLToPath(new URL('../../examples/inventory.intent', import.meta.url));
    const candidateFile = fileURLToPath(new URL('../../examples/inventory.changed.intent', import.meta.url));
    const baseline = checked(await loadFile(baselineFile));
    const candidate = checked(compile(await readFile(candidateFile, 'utf8'), baseline.snapshot.uri));
    console.log('\n1. Execute the human-owned baseline\n');
    const evidence = verify(baseline);
    for (const result of evidence.results)
        console.log(`  ${result.status.toUpperCase()} ${result.name}`);
    console.log('\n2. Inspect actual evaluation evidence for case.exact\n');
    const exact = evidence.results.find(r => r.id === 'case.exact')!;
    for (const step of exact.actual.trace.filter(s => s.kind === 'binary' || s.kind === 'return'))
        console.log(`  ${step.label} → ${JSON.stringify(step.value)} (${step.span.uri}:${step.span.line})`);
    console.log('\n3. Compare a policy change that ALSO changes the expected answer\n');
    summary(compare(baseline, candidate));
    console.log('\n4. Reject a proposal based on an outdated snapshot\n');
    const proposal = createProposal(baseline, candidate.snapshot.source, 'Keep one item available.', 'assistant');
    const preview = previewChange(baseline, proposal);
    console.log(`  Review ID: ${preview.id}`);
    try {
        previewChange(candidate, proposal);
        throw new Error('Stale proposal was not rejected.');
    }
    catch (error) {
        if (!errorMessage(error).includes('STALE_PROPOSAL'))
            throw error;
        console.log(`  ${errorMessage(error)}`);
    }
    console.log('\n5. Demonstrate a local compare-and-set conflict\n');
    const store = new MemoryStore();
    const stock: RecordValue = { $record: 'stock', fields: { 'stock.onHand': 10, 'stock.reserved': 2 } };
    store.create('sample', stock);
    const first = store.read('sample');
    const second = store.read('sample');
    console.log(`  First write: ${JSON.stringify(store.compareAndSet('sample', first.version, stock))}`);
    console.log(`  Stale write: ${JSON.stringify(store.compareAndSet('sample', second.version, stock))}`);
    console.log('\nDemo complete. No repository, database, AI provider, or deployment was modified.\n');
    return 0;
}
export async function main(argv: string[]): Promise<number> {
    const command = argv[0];
    if (!command || command === 'help' || command === '--help') {
        console.log(help);
        return 0;
    }
    const { positional, flags } = options(argv.slice(1));
    const at = (i: number): string => { const value = positional[i]; if (!value)
        throw new Error(`Missing argument ${i + 1}.\n${help}`); return value; };
    const flag = (name: string): string | undefined => { const value = flags.get(name); return typeof value === 'string' ? value : undefined; };
    const requireFlag = (name: string): string => { const value = flag(name); if (!value)
        throw new Error(`Required option ${name}.`); return value; };
    const allowed: Record<string, {
        count: number;
        flags: string[];
    }> = {
        check: { count: 1, flags: ['--json'] }, inspect: { count: 1, flags: [] }, run: { count: 1, flags: ['--json', '--evidence', '--trace'] },
        diff: { count: 2, flags: ['--json', '--out', '--fail-on-regression'] }, evidence: { count: 2, flags: [] },
        propose: { count: 2, flags: ['--reason', '--out', '--author'] }, preview: { count: 2, flags: ['--json', '--out'] },
        apply: { count: 2, flags: ['--review'] }, demo: { count: 0, flags: [] }
    };
    const shape = allowed[command];
    if (!shape)
        throw new Error(`Unknown command ${command}.\n${help}`);
    if (positional.length !== shape.count)
        throw new Error(`${command} expects ${shape.count} positional arguments.\n${help}`);
    for (const key of flags.keys())
        if (!shape.flags.includes(key))
            throw new Error(`${command} does not accept ${key}.`);
    if (command === 'demo')
        return demo();
    const current = await loadFile(at(0));
    if (command === 'check') {
        if (flags.has('--json'))
            json({ valid: current.valid, snapshotId: current.snapshot.id, diagnostics: current.diagnostics });
        else {
            console.log(current.valid ? 'VALID' : 'INVALID');
            for (const d of current.diagnostics)
                console.log(`${d.span.line}:${d.span.column} ${d.severity} ${d.code}: ${d.message}`);
        }
        return current.valid ? 0 : 2;
    }
    if (command === 'inspect') {
        json(inspectProject(current));
        return current.valid ? 0 : 2;
    }
    if (command === 'evidence') {
        const result = assessEvidence(current, JSON.parse(await readFile(at(1), 'utf8')));
        json(result);
        return result.current ? 0 : 1;
    }
    checked(current);
    if (command === 'run') {
        const result = verify(current);
        if (flag('--evidence'))
            await writeNewJson(flag('--evidence')!, result);
        if (flags.has('--json'))
            json(result);
        else {
            console.log(`Snapshot ${result.snapshot.id}\nEngine ${result.snapshot.engineDigest}`);
            for (const scenario of result.results)
                console.log(`${scenario.status.toUpperCase()} ${scenario.id}: ${scenario.name}`);
            console.log(`${result.results.filter(r => r.status === 'passed').length}/${result.results.length} passed; deployment not connected; not a formal proof.`);
            if (flag('--trace')) {
                const scenario = result.results.find(r => r.id === flag('--trace'));
                if (!scenario)
                    throw new Error(`Unknown scenario ${flag('--trace')}.`);
                for (const step of scenario.actual.trace)
                    console.log(`${step.index} ${step.label} → ${step.value === undefined ? step.detail ?? '' : JSON.stringify(displayValue(current, step.value))} @ ${step.span.line}:${step.span.column}`);
            }
        }
        return result.passed ? 0 : 1;
    }
    if (command === 'diff') {
        const result = compare(current, checked(await loadFile(at(1))));
        if (flag('--out'))
            await writeNewJson(flag('--out')!, result);
        if (flags.has('--json'))
            json(result);
        else
            summary(result);
        return flags.has('--fail-on-regression') && result.baselineChecks.some(r => r.before.status === 'passed' && r.candidateStatus !== 'passed') ? 1 : 0;
    }
    if (command === 'propose') {
        const author = flag('--author') ?? 'human';
        if (author !== 'human' && author !== 'assistant')
            throw new Error('--author must be human or assistant.');
        const proposal = createProposal(current, await readFile(at(1), 'utf8'), requireFlag('--reason'), author);
        await writeNewJson(requireFlag('--out'), proposal);
        console.log(`Proposal ${proposal.id} written to ${flag('--out')}. No source changed.`);
        return 0;
    }
    const proposal = readProposal(JSON.parse(await readFile(at(1), 'utf8')));
    if (command === 'preview') {
        const preview = previewChange(current, proposal);
        if (flag('--out'))
            await writeNewJson(flag('--out')!, preview);
        if (flags.has('--json'))
            json(preview);
        else {
            console.log(`Review ID: ${preview.id}\nApplicable: ${preview.applicable}\nAuthor: ${proposal.author}\nRationale (unverified): ${proposal.rationale}`);
            if (preview.report)
                summary(preview.report);
            else
                json(preview.candidate.diagnostics);
        }
        return preview.applicable ? 0 : 2;
    }
    const updated = await applyFile(at(0), proposal, requireFlag('--review'));
    console.log(`Applied reviewed change. New snapshot: ${updated.snapshot.id}`);
    return 0;
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    main(process.argv.slice(2)).then(code => { process.exitCode = code; }).catch(error => { console.error(errorMessage(error)); process.exitCode = 2; });
}
