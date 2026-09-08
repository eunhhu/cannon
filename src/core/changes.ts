import type { Compilation, Definition, Outcome, ScenarioDef, ScenarioResult, Snapshot } from './model.js';
import { evaluate, outcomeSignature, runScenario, runScenarios } from './execution.js';
import { digest, equal, freeze, semantic } from './util.js';
export interface Change {
    id: string;
    targetId: string;
    label: string;
    kind: string;
    category: 'architecture' | 'model' | 'policy' | 'interface' | 'scenario-input' | 'expectation' | 'identity';
    before?: unknown;
    after?: unknown;
    evidence: 'structural-comparison';
}
export interface BaselineCheck {
    id: string;
    name: string;
    before: ScenarioResult;
    candidateActual: Outcome;
    candidateStatus: 'passed' | 'failed' | 'error';
    actualChanged: boolean;
}
export interface ChangeReport {
    schemaVersion: 1;
    before: Snapshot;
    after: Snapshot;
    changes: Change[];
    impacted: {
        id: string;
        name: string;
        kind: string;
    }[];
    baselineChecks: BaselineCheck[];
    candidateScenarios: ScenarioResult[];
    limitations: string[];
}
/** Structural changes and observed behavior are separate facts, never an equivalence proof. */
export function compare(before: Compilation, after: Compilation): ChangeReport {
    if (!before.valid || !after.valid || !before.program || !after.program)
        throw new Error('Compare requires two valid compilations. Inspect diagnostics first.');
    const changes: Change[] = [];
    const add = (targetId: string, label: string, kind: string, category: Change['category'], old?: unknown, next?: unknown): void => {
        const base = { targetId, label, kind, category, before: semantic(old), after: semantic(next), evidence: 'structural-comparison' as const };
        changes.push({ id: digest(base), ...base });
    };
    const difference = (id: string, name: string, kind: string, category: Change['category'], old: unknown, next: unknown): void => {
        if (!equal(semantic(old), semantic(next)))
            add(id, name, kind, category, old, next);
    };
    if (before.program.module !== after.program.module)
        add('@module', after.program.module, 'module-renamed', 'architecture', before.program.module, after.program.module);
    const oldDefs = new Map(before.program.definitions.map(d => [d.id, d]));
    const newDefs = new Map(after.program.definitions.map(d => [d.id, d]));
    for (const id of new Set([...oldDefs.keys(), ...newDefs.keys()])) {
        const old = oldDefs.get(id);
        const next = newDefs.get(id);
        if (!old || !next) {
            const d = (old ?? next)!;
            add(id, d.name, old ? 'definition-removed' : 'definition-added', d.kind === 'scenario' ? 'expectation' : d.kind === 'record' ? 'model' : 'policy', old, next);
            continue;
        }
        if (old.kind !== next.kind) {
            add(id, next.name, 'definition-kind-changed', 'interface', old, next);
            continue;
        }
        difference(id, next.name, 'renamed', 'identity', old.name, next.name);
        if (old.kind === 'record' && next.kind === 'record') {
            const a = new Map(old.fields.map(f => [f.id, f]));
            const b = new Map(next.fields.map(f => [f.id, f]));
            for (const fid of new Set([...a.keys(), ...b.keys()])) {
                const f = a.get(fid);
                const g = b.get(fid);
                const label = `${next.name}.${(f ?? g)!.name}`;
                if (!f || !g)
                    add(fid, label, f ? 'field-removed' : 'field-added', 'model', f, g);
                else {
                    difference(fid, label, 'field-renamed', 'identity', f.name, g.name);
                    difference(fid, label, 'field-type-changed', 'model', f.type, g.type);
                }
            }
            difference(id, next.name, 'invariant-changed', 'policy', old.invariants, next.invariants);
        }
        else if (old.kind === 'rule' && next.kind === 'rule') {
            difference(id, next.name, 'signature-changed', 'interface', { params: old.params, returns: old.returns }, { params: next.params, returns: next.returns });
            difference(id, next.name, 'rule-body-changed', 'policy', old.body, next.body);
        }
        else if (old.kind === 'transition' && next.kind === 'transition') {
            difference(id, next.name, 'signature-changed', 'interface', { params: old.params, returns: old.returns }, { params: next.params, returns: next.returns });
            difference(id, next.name, 'guards-changed', 'policy', old.guards, next.guards);
            difference(id, next.name, 'next-state-changed', 'policy', old.next, next.next);
        }
        else if (old.kind === 'scenario' && next.kind === 'scenario') {
            difference(id, next.name, 'scenario-input-changed', 'scenario-input', old.actual, next.actual);
            difference(id, next.name, 'expectation-changed', 'expectation', old.expected, next.expected);
        }
    }
    const affected = new Set(changes.map(c => c.targetId));
    const queue = [...affected];
    const dependencies = [...before.dependencies, ...after.dependencies];
    while (queue.length) {
        const id = queue.shift()!;
        for (const edge of dependencies)
            if (edge.to === id && !affected.has(edge.from)) {
                affected.add(edge.from);
                queue.push(edge.from);
            }
    }
    const definitions = new Map([...oldDefs, ...newDefs]);
    const impacted = [...affected].map(id => definitions.get(id)).filter((d): d is Definition => d !== undefined).map(d => ({ id: d.id, name: d.name, kind: d.kind }));
    // Replay the OLD actual expression against new definitions by stable IDs.
    // Keep the OLD expected result from the OLD snapshot, even if new tests were edited/deleted.
    const baselineChecks = before.program.definitions.filter((d): d is ScenarioDef => d.kind === 'scenario').map(scenario => {
        const historical = runScenario(before, scenario);
        const candidateActual = evaluate(after, scenario.actual);
        const candidateStatus = !candidateActual.ok || !historical.expected.ok ? 'error' as const : equal(candidateActual.value, historical.expected.value) ? 'passed' as const : 'failed' as const;
        return { id: scenario.id, name: scenario.name, before: historical, candidateActual, candidateStatus, actualChanged: !equal(outcomeSignature(historical.actual), outcomeSignature(candidateActual)) };
    });
    return freeze({ schemaVersion: 1 as const, before: before.snapshot, after: after.snapshot, changes, impacted, baselineChecks, candidateScenarios: runScenarios(after), limitations: [
            'Impact is a dependency-based candidate set, not a whole-program behavioral proof.',
            'Baseline replay covers historical scenarios only; untested inputs remain unknown.',
            'Execution is local and pure. Database, network, multi-module architecture and deployment are not connected.',
            'Historical scenario spans retain their historical sourceId; called definitions use the candidate sourceId.'
        ] });
}
