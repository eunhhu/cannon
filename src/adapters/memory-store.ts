import type { RecordValue } from '../core/model.js';
/** Demonstration only: compare-and-set is atomic within this synchronous in-memory instance. */
export class MemoryStore {
    private readonly rows = new Map<string, {
        version: number;
        value: RecordValue;
    }>();
    create(key: string, value: RecordValue): void {
        if (this.rows.has(key))
            throw new Error(`Row ${key} already exists.`);
        this.rows.set(key, { version: 0, value: structuredClone(value) });
    }
    read(key: string): {
        version: number;
        value: RecordValue;
    } {
        const row = this.rows.get(key);
        if (!row)
            throw new Error(`Unknown row ${key}.`);
        return structuredClone(row);
    }
    compareAndSet(key: string, expectedVersion: number, value: RecordValue): {
        ok: true;
        version: number;
    } | {
        ok: false;
        currentVersion: number;
    } {
        const current = this.rows.get(key);
        if (!current)
            throw new Error(`Unknown row ${key}.`);
        if (current.version !== expectedVersion)
            return { ok: false, currentVersion: current.version };
        const version = current.version + 1;
        if (!Number.isSafeInteger(version))
            throw new Error('Version overflow.');
        this.rows.set(key, { version, value: structuredClone(value) });
        return { ok: true, version };
    }
}
