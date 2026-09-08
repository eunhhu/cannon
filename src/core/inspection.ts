import type { Compilation } from './model.js';
export function inspectProject(compilation: Compilation): unknown {
    return {
        snapshotId: compilation.snapshot.id, sourceId: compilation.snapshot.sourceId, engineDigest: compilation.snapshot.engineDigest,
        module: compilation.program?.module ?? null, valid: compilation.valid, diagnostics: compilation.diagnostics,
        definitions: compilation.program?.definitions.map(d => ({ id: d.id, name: d.name, kind: d.kind, span: d.span,
            ...('params' in d ? { inputs: d.params.map(p => ({ name: p.name, type: p.type })), returns: d.returns } : {}),
            ...(d.kind === 'record' ? { fields: d.fields, invariantCount: d.invariants.length } : {}) })) ?? [],
        dependencies: compilation.dependencies,
        status: { specification: 'source-present-not-approved-automatically', verification: 'run-verify-to-produce-evidence', deployment: 'not-connected' },
        boundaries: ['Single source file / one module in v0.1.', 'No implicit effects or arbitrary JavaScript execution.', 'External adapters are not analyzed by this compiler.']
    };
}
export function inspectDefinition(compilation: Compilation, id: string): unknown {
    const definition = compilation.program?.definitions.find(d => d.id === id);
    if (!definition)
        throw new Error(`Definition ${id} was not found.`);
    return { snapshotId: compilation.snapshot.id, definition, dependencies: compilation.dependencies.filter(e => e.from === id), dependents: compilation.dependencies.filter(e => e.to === id) };
}
