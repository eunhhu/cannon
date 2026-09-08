import { createHash } from 'node:crypto';
export function stableJson(value: unknown): string {
    if (value === null || typeof value !== 'object')
        return JSON.stringify(value) ?? 'null';
    if (Array.isArray(value))
        return `[${value.map(stableJson).join(',')}]`;
    const object = value as Record<string, unknown>;
    return `{${Object.keys(object).filter(k => object[k] !== undefined).sort().map(k => `${JSON.stringify(k)}:${stableJson(object[k])}`).join(',')}}`;
}
export function digest(value: unknown): string { return createHash('sha256').update(stableJson(value)).digest('hex'); }
export function equal(a: unknown, b: unknown): boolean { return stableJson(a) === stableJson(b); }
export function freeze<T>(value: T): T {
    if (value && typeof value === 'object' && !Object.isFrozen(value)) {
        for (const child of Object.values(value))
            freeze(child);
        Object.freeze(value);
    }
    return value;
}
/** Compare syntax/semantics without letting whitespace or source locations create false changes. */
export function semantic(value: unknown): unknown {
    if (Array.isArray(value))
        return value.map(semantic);
    if (!value || typeof value !== 'object')
        return value;
    const object = value as Record<string, unknown>;
    return Object.fromEntries(Object.entries(object)
        .filter(([key]) => !['span', 'explicitId'].includes(key) && !(key === 'name' && (object['targetId'] || object['fieldId'])))
        .map(([key, val]) => [key, semantic(val)]));
}
export function errorMessage(error: unknown): string { return error instanceof Error ? error.message : String(error); }
