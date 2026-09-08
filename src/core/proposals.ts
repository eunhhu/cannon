import type { Compilation } from './model.js';
import { compile } from './compile.js';
import { compare, type ChangeReport } from './changes.js';
import { digest, freeze } from './util.js';
export interface Proposal {
    schemaVersion: 1;
    id: string;
    baseSnapshotId: string;
    candidateSource: string;
    rationale: string;
    author: 'human' | 'assistant';
}
export interface Preview {
    id: string;
    proposalId: string;
    candidate: Compilation;
    report?: ChangeReport;
    applicable: boolean;
}
export function createProposal(base: Compilation, candidateSource: string, rationale: string, author: Proposal['author'] = 'human'): Proposal {
    const body = { schemaVersion: 1 as const, baseSnapshotId: base.snapshot.id, candidateSource, rationale, author };
    return freeze({ id: digest(body), ...body });
}
export function readProposal(value: unknown): Proposal {
    if (!value || typeof value !== 'object' || Array.isArray(value))
        throw new Error('Malformed proposal.');
    const p = value as Partial<Proposal>;
    if (p.schemaVersion !== 1 || typeof p.id !== 'string' || typeof p.baseSnapshotId !== 'string' || typeof p.candidateSource !== 'string' || typeof p.rationale !== 'string' || !['human', 'assistant'].includes(p.author ?? ''))
        throw new Error('Malformed proposal fields.');
    const { id, ...body } = p;
    if (id !== digest(body))
        throw new Error('Proposal checksum mismatch.');
    return freeze(p as Proposal);
}
export function previewChange(current: Compilation, input: Proposal): Preview {
    const proposal = readProposal(input);
    if (current.snapshot.id !== proposal.baseSnapshotId)
        throw new Error('STALE_PROPOSAL: source or engine changed; regenerate the proposal.');
    const candidate = compile(proposal.candidateSource, current.snapshot.uri);
    const report = current.valid && candidate.valid ? compare(current, candidate) : undefined;
    const id = digest({ proposal: proposal.id, candidate: candidate.snapshot.id, report, diagnostics: candidate.diagnostics });
    const preview: Preview = { id, proposalId: proposal.id, candidate, applicable: !!report };
    if (report)
        preview.report = report;
    return freeze(preview);
}
/** Approval names the exact deterministic preview, not a generic 'yes'. Tests remain review evidence. */
export function applyChange(current: Compilation, proposal: Proposal, reviewedPreviewId: string): Compilation {
    const preview = previewChange(current, proposal);
    if (preview.id !== reviewedPreviewId)
        throw new Error('REVIEW_MISMATCH: review does not describe this exact change.');
    if (!preview.applicable)
        throw new Error('INVALID_CANDIDATE: resolve compilation errors before applying.');
    return preview.candidate;
}
