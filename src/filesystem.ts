import { readFile, lstat, open, rename, unlink } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';
import { resolve } from 'node:path';
import { compile } from './core/compile.js';
import { applyChange, type Proposal } from './core/proposals.js';
import type { Compilation } from './core/model.js';
export async function loadFile(path: string): Promise<Compilation> {
    const uri = resolve(path);
    return compile(await readFile(uri, 'utf8'), uri);
}
export async function writeNewJson(path: string, value: unknown): Promise<void> {
    const handle = await open(resolve(path), 'wx', 0o600);
    try {
        await handle.writeFile(JSON.stringify(value, null, 2) + '\n', 'utf8');
        await handle.sync();
    }
    finally {
        await handle.close();
    }
}
/** Atomic replacement; lock coordinates this tool only, not unrelated editors. See semantics.md. */
export async function applyFile(path: string, proposal: Proposal, reviewId: string): Promise<Compilation> {
    const uri = resolve(path);
    const lockPath = `${uri}.intent-lock`;
    const temporary = `${uri}.intent-${randomUUID()}.tmp`;
    const lock = await open(lockPath, 'wx', 0o600);
    let tempExists = false;
    try {
        const stat = await lstat(uri);
        if (!stat.isFile() || stat.isSymbolicLink())
            throw new Error('Apply requires a regular, non-symlink source file.');
        const before = await loadFile(uri);
        const after = applyChange(before, proposal, reviewId);
        const output = await open(temporary, 'wx', stat.mode & 0o777);
        tempExists = true;
        try {
            await output.writeFile(after.snapshot.source, 'utf8');
            await output.sync();
        }
        finally {
            await output.close();
        }
        const current = await loadFile(uri);
        const latest = await lstat(uri);
        if (current.snapshot.id !== before.snapshot.id || !latest.isFile() || latest.isSymbolicLink() || latest.ino !== stat.ino)
            throw new Error('SOURCE_MOVED: source changed during apply.');
        await rename(temporary, uri);
        tempExists = false;
        return after;
    }
    finally {
        if (tempExists)
            await unlink(temporary).catch(() => undefined);
        await lock.close();
        await unlink(lockPath);
    }
}
