import type { Definition, Expr, Field, Parameter, Program, Span, TransitionDef } from '../core/model.js';
import { children, expressions } from '../core/model.js';
interface Token {
    text: string;
    kind: 'word' | 'number' | 'string' | 'symbol' | 'eof';
    span: Span;
}
export class ParseError extends Error {
    constructor(message: string, public readonly span: Span) { super(message); this.name = 'ParseError'; }
}
function tokenize(source: string, uri: string, sourceId: string): Token[] {
    const tokens: Token[] = [];
    let i = 0;
    let line = 1;
    let column = 1;
    const position = (): Span => ({ sourceId, uri, start: i, end: i, line, column });
    function advance(text: string): void {
        for (const c of text) {
            if (c === '\n') {
                line++;
                column = 1;
            }
            else
                column += c.length;
        }
        i += text.length;
    }
    if (source.length > 262144)
        throw new ParseError('Source exceeds the 262,144 UTF-16 code-unit prototype limit.', position());
    while (i < source.length) {
        const rest = source.slice(i);
        const white = /^(?:\s+|\/\/[^\n]*)/.exec(rest);
        if (white) {
            advance(white[0]);
            continue;
        }
        const start = position();
        let text: string;
        let kind: Token['kind'];
        const string = /^"(?:[^"\\\r\n]|\\(?:["\\/bfnrt]|u[0-9a-fA-F]{4}))*"/.exec(rest);
        const number = /^\d+/.exec(rest);
        const word = /^[A-Za-z_][A-Za-z0-9_]*/.exec(rest);
        const symbol = /^(?:->|<=|>=|==|!=|&&|\|\||[{}(),.:;@+*<>!=\-])/.exec(rest);
        if (string) {
            text = string[0];
            kind = 'string';
        }
        else if (number) {
            text = number[0];
            kind = 'number';
        }
        else if (word) {
            text = word[0];
            kind = 'word';
        }
        else if (symbol) {
            text = symbol[0];
            kind = 'symbol';
        }
        else
            throw new ParseError(`Unexpected character ${JSON.stringify(source[i])}.`, start);
        advance(text);
        tokens.push({ text, kind, span: { ...start, end: i } });
        if (tokens.length > 20000)
            throw new ParseError('Token budget exceeded.', start);
    }
    tokens.push({ text: '<eof>', kind: 'eof', span: position() });
    return tokens;
}
const precedence: Record<string, number> = { or: 1, '||': 1, and: 2, '&&': 2, '==': 3, '!=': 3, '<': 4, '<=': 4, '>': 4, '>=': 4, '+': 5, '-': 5, '*': 6 };
const reserved = new Set(['module', 'record', 'rule', 'transition', 'scenario', 'invariant', 'require', 'next', 'expect', 'if', 'then', 'else', 'true', 'false', 'and', 'or', 'not']);
class Parser {
    private index = 0;
    private expressionDepth = 0;
    constructor(private readonly tokens: Token[]) { }
    private peek(): Token { return this.tokens[this.index]!; }
    private take(): Token { const token = this.peek(); if (token.kind === 'eof')
        throw new ParseError('Unexpected end of source.', token.span); this.index++; return token; }
    private accept(text: string): boolean { if (this.peek().text === text) {
        this.index++;
        return true;
    } return false; }
    private expect(text: string): Token { const token = this.take(); if (token.text !== text)
        throw new ParseError(`Expected ${JSON.stringify(text)}, found ${JSON.stringify(token.text)}.`, token.span); return token; }
    private word(): Token { const token = this.take(); if (token.kind !== 'word' || reserved.has(token.text))
        throw new ParseError('Expected an identifier.', token.span); return token; }
    private string(): Token { const token = this.take(); if (token.kind !== 'string')
        throw new ParseError('Expected a double-quoted string.', token.span); return token; }
    private span(start: Span): Span { return { ...start, end: this.tokens[this.index - 1]?.span.end ?? start.end }; }
    private id(): string {
        this.expect('@');
        this.expect('id');
        this.expect('(');
        const token = this.string();
        this.expect(')');
        const id: string = JSON.parse(token.text);
        if (!/^[A-Za-z0-9_][A-Za-z0-9_.:-]{0,127}$/.test(id))
            throw new ParseError('IDs must be 1–128 ASCII identifier characters.', token.span);
        return id;
    }
    parse(): Program {
        const start = this.expect('module').span;
        const module = this.word().text;
        this.accept(';');
        const definitions: Definition[] = [];
        while (this.peek().kind !== 'eof') {
            if (definitions.length >= 512)
                throw new ParseError('Definition budget exceeded.', this.peek().span);
            const beginning = this.peek().span;
            const id = this.id();
            const kind = this.take();
            if (kind.text === 'record') {
                const name = this.word().text;
                this.expect('{');
                const fields: Field[] = [];
                const invariants: Expr[] = [];
                while (!this.accept('}')) {
                    if (this.accept('invariant')) {
                        invariants.push(this.expression());
                        this.accept(';');
                        continue;
                    }
                    const fs = this.peek().span;
                    const explicitId = this.peek().text === '@';
                    const explicit = explicitId ? this.id() : undefined;
                    const fieldName = this.word().text;
                    this.expect(':');
                    const type = this.word().text;
                    fields.push({ id: explicit ?? `${id}.${fieldName}`, explicitId, name: fieldName, type, span: this.span(fs) });
                    this.accept(',');
                    this.accept(';');
                }
                definitions.push({ kind: 'record', id, name, fields, invariants, span: this.span(beginning) });
            }
            else if (kind.text === 'rule' || kind.text === 'transition') {
                const name = this.word().text;
                const params = this.parameters();
                this.expect('->');
                const returns = this.word().text;
                if (kind.text === 'rule') {
                    this.expect('=');
                    const body = this.expression();
                    this.accept(';');
                    definitions.push({ kind: 'rule', id, name, params, returns, body, span: this.span(beginning) });
                }
                else {
                    this.expect('{');
                    const guards: TransitionDef['guards'] = [];
                    while (this.accept('require')) {
                        const gs = this.peek().span;
                        const condition = this.expression();
                        this.expect('else');
                        const error: string = JSON.parse(this.string().text);
                        guards.push({ condition, error, span: this.span(gs) });
                        this.accept(';');
                    }
                    this.expect('next');
                    const next = this.expression();
                    this.accept(';');
                    this.expect('}');
                    definitions.push({ kind: 'transition', id, name, params, returns, guards, next, span: this.span(beginning) });
                }
            }
            else if (kind.text === 'scenario') {
                const name: string = JSON.parse(this.string().text);
                this.expect('{');
                this.expect('expect');
                const assertion = this.expression();
                this.accept(';');
                this.expect('}');
                if (assertion.kind !== 'binary' || assertion.op !== '==')
                    throw new ParseError('A scenario must contain: expect <actual> == <expected>.', assertion.span);
                definitions.push({ kind: 'scenario', id, name, actual: assertion.left, expected: assertion.right, span: this.span(beginning) });
            }
            else
                throw new ParseError('Expected record, rule, transition, or scenario after @id.', kind.span);
        }
        const program: Program = { module, definitions, span: this.span(start) };
        // An iterative post-check also catches deeply left-associated expressions.
        for (const definition of definitions)
            for (const root of expressions(definition)) {
                const stack: [
                    Expr,
                    number
                ][] = [[root, 0]];
                while (stack.length) {
                    const [expr, depth] = stack.pop()!;
                    if (depth > 128)
                        throw new ParseError('Expression depth exceeds 128.', expr.span);
                    stack.push(...children(expr).map(child => [child, depth + 1] as [
                        Expr,
                        number
                    ]));
                }
            }
        return program;
    }
    private parameters(): Parameter[] {
        this.expect('(');
        const params: Parameter[] = [];
        if (!this.accept(')')) {
            do {
                const start = this.peek().span;
                const name = this.word().text;
                this.expect(':');
                const type = this.word().text;
                params.push({ name, type, span: this.span(start) });
            } while (this.accept(','));
            this.expect(')');
        }
        return params;
    }
    private expression(min = 0): Expr {
        this.expressionDepth++;
        if (this.expressionDepth > 128)
            throw new ParseError('Expression nesting exceeds 128.', this.peek().span);
        try {
            let left = this.primary();
            while (true) {
                if (this.accept('.')) {
                    const field = this.word();
                    left = { kind: 'field', object: left, name: field.text, span: { ...left.span, end: field.span.end } };
                    continue;
                }
                const token = this.peek();
                const p = precedence[token.text];
                if (p === undefined || p < min)
                    break;
                this.take();
                const right = this.expression(p + 1);
                left = { kind: 'binary', op: token.text === '&&' ? 'and' : token.text === '||' ? 'or' : token.text, left, right, span: { ...left.span, end: right.span.end } };
            }
            return left;
        }
        finally {
            this.expressionDepth--;
        }
    }
    private primary(): Expr {
        const token = this.take();
        const span = token.span;
        if (token.text === '(') {
            const expression = this.expression();
            this.expect(')');
            return expression;
        }
        if (token.text === '-' || token.text === '!' || token.text === 'not') {
            const operand = this.expression(7);
            return { kind: 'unary', op: token.text === '-' ? '-' : 'not', operand, span: this.span(span) };
        }
        if (token.text === 'if') {
            const condition = this.expression();
            this.expect('then');
            const then = this.expression();
            this.expect('else');
            const otherwise = this.expression();
            return { kind: 'if', condition, then, otherwise, span: this.span(span) };
        }
        if (token.kind === 'number') {
            const value = Number(token.text);
            if (!Number.isSafeInteger(value))
                throw new ParseError('Integer literal is outside the safe integer range.', span);
            return { kind: 'literal', value, span };
        }
        if (token.kind === 'string')
            return { kind: 'literal', value: JSON.parse(token.text) as string, span };
        if (token.text === 'true' || token.text === 'false')
            return { kind: 'literal', value: token.text === 'true', span };
        if (token.kind !== 'word' || reserved.has(token.text))
            throw new ParseError('Expected an expression.', span);
        if (this.accept('(')) {
            const args: Expr[] = [];
            if (!this.accept(')')) {
                do {
                    args.push(this.expression());
                } while (this.accept(','));
                this.expect(')');
            }
            return { kind: 'call', name: token.text, args, span: this.span(span) };
        }
        if (this.accept('{')) {
            const fields: Extract<Expr, {
                kind: 'record';
            }>['fields'] = [];
            if (!this.accept('}')) {
                do {
                    const field = this.word();
                    this.expect(':');
                    const value = this.expression();
                    fields.push({ name: field.text, value, span: this.span(field.span) });
                } while (this.accept(','));
                this.expect('}');
            }
            return { kind: 'record', name: token.text, fields, span: this.span(span) };
        }
        return { kind: 'ref', name: token.text, span };
    }
}
export function parse(source: string, uri: string, sourceId: string): Program { return new Parser(tokenize(source, uri, sourceId)).parse(); }
