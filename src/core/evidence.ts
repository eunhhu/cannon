import type { Compilation, RunOptions, ScenarioResult, Snapshot } from './model.js';
import { runOptions, runScenarios } from './execution.js';
import { digest, equal, freeze } from './util.js';
export interface Evidence {
    schemaVersion: 1;
    id: string;
    createdAt: string;
    snapshot: Snapshot;
    runtime: string;
    options: Required<RunOptions>;
    results: ScenarioResult[];
    passed: boolean;
    claims: {
        kind: 'executed-scenarios';
        formalProof: false;
        deployment: 'not-connected';
    };
}
export function verify(compilation: Compilation, options: RunOptions = {}): Evidence {
    const normalized = runOptions(options);
    const results = runScenarios(compilation, normalized);
    const content = { schemaVersion: 1 as const, createdAt: new Date().toISOString(), snapshot: compilation.snapshot,
        runtime: process.version, options: normalized, results, passed: results.length > 0 && results.every(r => r.status === 'passed'),
        claims: { kind: 'executed-scenarios' as const, formalProof: false as const, deployment: 'not-connected' as const } };
    return freeze({ id: digest(content), ...content });
}
/** Hashes detect accidental mutation, not forged reports. This is freshness checking, not attestation. */
export function assessEvidence(compilation: Compilation, candidate: unknown, options: RunOptions = {}): {
    current: boolean;
    reasons: string[];
} {
    if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate))
        return { current: false, reasons: ['Malformed evidence.'] };
    const report = candidate as Partial<Evidence>;
    const reasons: string[] = [];
    if (report.schemaVersion !== 1 || !report.snapshot || !Array.isArray(report.results))
        return { current: false, reasons: ['Unsupported or malformed evidence.'] };
    const { id, ...content } = report;
    if (id !== digest(content))
        reasons.push('Report content checksum differs.');
    if (report.snapshot.id !== compilation.snapshot.id)
        reasons.push('Source snapshot or engine changed.');
    if (report.snapshot.source !== compilation.snapshot.source || report.snapshot.uri !== compilation.snapshot.uri)
        reasons.push('Embedded source differs.');
    if (report.snapshot.engineDigest !== compilation.snapshot.engineDigest)
        reasons.push('Engine build changed.');
    if (report.runtime !== process.version)
        reasons.push('Node runtime changed.');
    if (!equal(report.options, runOptions(options)))
        reasons.push('Execution options changed.');
    if (!compilation.valid)
        reasons.push('Current source is invalid.');
    return { current: reasons.length === 0, reasons };
}
