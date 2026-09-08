import { ENGINE_DIGEST, ENGINE_VERSION } from '../generated/build-info.js';
import { parse, ParseError } from '../language/parser.js';
import { check } from './checking.js';
import { digest, freeze } from './util.js';
import type { Compilation, Snapshot } from './model.js';
export function compile(source: string, uri = 'memory.intent'): Compilation {
    const sourceId = digest({ uri, source });
    const snapshot: Snapshot = { id: digest({ sourceId, engine: ENGINE_DIGEST }), sourceId, uri, source, engineDigest: ENGINE_DIGEST, engineVersion: ENGINE_VERSION };
    try {
        const program = parse(source, uri, sourceId);
        const { diagnostics, dependencies } = check(program);
        return freeze({ snapshot, program, diagnostics, dependencies, valid: !diagnostics.some(d => d.severity === 'error') });
    }
    catch (error) {
        if (!(error instanceof ParseError))
            throw error;
        return freeze({ snapshot, diagnostics: [{ code: 'PARSE_ERROR', message: error.message, span: error.span, severity: 'error' as const }], dependencies: [], valid: false });
    }
}
