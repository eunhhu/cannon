use crate::inputs::{check_schema, InputReference, InputSpec};
use crate::{Diagnostic, Span, Type, Value, MAX_DEPTH, MAX_INT, MAX_SOURCE_UNITS, MAX_TOKENS};

#[derive(Clone, Debug)]
enum TokenKind { Literal(Value), Word(String), Symbol(String), End }
#[derive(Clone, Debug)]
struct Token { kind: TokenKind, span: Span }
impl Token {
    fn is(&self, text: &str) -> bool {
        matches!(&self.kind, TokenKind::Word(s) | TokenKind::Symbol(s) if s == text)
    }
}

struct Lexer<'a> { source: &'a str, byte: usize, offset: usize, line: usize, column: usize }
impl Lexer<'_> {
    fn span(&self) -> Span { Span { start: self.offset, end: self.offset, line: self.line, column: self.column } }
    fn peek(&self) -> Option<char> { self.source[self.byte..].chars().next() }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.byte += c.len_utf8();
        self.offset += c.len_utf16();
        if c == '\n' { self.line += 1; self.column = 1; } else { self.column += c.len_utf16(); }
        Some(c)
    }
    fn error(&self, message: &str, span: Span) -> Diagnostic { Diagnostic::new("PARSE_ERROR", message, span) }
    fn string(&mut self, start: Span) -> Result<Value, Diagnostic> {
        self.bump();
        let mut units = Vec::new();
        loop {
            match self.bump() {
                Some('"') => return Ok(Value::Text(units)),
                Some('\\') => {
                    let unit = match self.bump() {
                        Some('"') => 34, Some('\\') => 92, Some('/') => 47,
                        Some('b') => 8, Some('f') => 12, Some('n') => 10,
                        Some('r') => 13, Some('t') => 9,
                        Some('u') => {
                            let mut value = 0u16;
                            for _ in 0..4 {
                                let digit = self.bump().and_then(|c| c.to_digit(16))
                                    .ok_or_else(|| self.error("Expected four hexadecimal escape digits.", start))?;
                                value = value * 16 + digit as u16;
                            }
                            value
                        }
                        _ => return Err(self.error("Invalid JSON string escape.", start)),
                    };
                    units.push(unit);
                }
                Some(c) if (c as u32) < 32 => return Err(self.error("Unescaped control character in string.", start)),
                Some(c) => { let mut buf = [0; 2]; units.extend_from_slice(c.encode_utf16(&mut buf)); }
                None => return Err(self.error("Unterminated string.", start)),
            }
        }
    }
    fn tokens(mut self) -> Result<Vec<Token>, Diagnostic> {
        if self.source.encode_utf16().count() > MAX_SOURCE_UNITS {
            return Err(self.error("Source exceeds the UTF-16 code-unit limit.", self.span()));
        }
        let mut tokens = Vec::new();
        while let Some(c) = self.peek() {
            // Match ECMAScript whitespace, not Rust's broader Unicode whitespace set.
            if matches!(c, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}') {
                self.bump(); continue;
            }
            if self.source[self.byte..].starts_with("//") {
                while self.peek().is_some_and(|ch| ch != '\n') { self.bump(); }
                continue;
            }
            let start = self.span();
            let kind = if c == '"' {
                TokenKind::Literal(self.string(start)?)
            } else if c.is_ascii_digit() {
                let begin = self.byte;
                while self.peek().is_some_and(|ch| ch.is_ascii_digit()) { self.bump(); }
                // Accumulate with a checked bound, accepting arbitrarily many leading zeroes.
                let mut value = 0i64;
                for digit in self.source[begin..self.byte].bytes() {
                    value = value.checked_mul(10).and_then(|n| n.checked_add((digit - b'0') as i64))
                        .filter(|n| *n <= MAX_INT).ok_or_else(|| self.error("Integer literal is outside the safe integer range.", Span { end: self.offset, ..start }))?;
                }
                TokenKind::Literal(Value::Int(value))
            } else if c.is_ascii_alphabetic() || c == '_' {
                let begin = self.byte;
                while self.peek().is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_') { self.bump(); }
                match &self.source[begin..self.byte] {
                    "true" => TokenKind::Literal(Value::Bool(true)),
                    "false" => TokenKind::Literal(Value::Bool(false)),
                    word => TokenKind::Word(word.to_owned()),
                }
            } else {
                let rest = &self.source[self.byte..];
                if let Some(op) = ["<=", ">=", "==", "!=", "&&", "||"].into_iter().find(|op| rest.starts_with(*op)) {
                    self.bump(); self.bump(); TokenKind::Symbol(op.to_owned())
                } else if "()+-*<>!".contains(c) {
                    self.bump(); TokenKind::Symbol(c.to_string())
                } else { return Err(self.error("Unexpected character in native expression subset.", start)); }
            };
            tokens.push(Token { kind, span: Span { end: self.offset, ..start } });
            if tokens.len() > MAX_TOKENS { return Err(self.error("Token budget exceeded.", start)); }
        }
        tokens.push(Token { kind: TokenKind::End, span: self.span() });
        Ok(tokens)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op { Add, Sub, Mul, Lt, Le, Gt, Ge, Eq, Ne, And, Or, Neg, Not }
impl Op {
    pub(crate) fn label(self) -> &'static str {
        match self { Self::Add => "+", Self::Sub | Self::Neg => "-", Self::Mul => "*", Self::Lt => "<", Self::Le => "<=", Self::Gt => ">", Self::Ge => ">=", Self::Eq => "==", Self::Ne => "!=", Self::And => "and", Self::Or => "or", Self::Not => "not" }
    }
}
#[derive(Clone, Debug)]
pub(crate) enum NodeKind { Input(usize), Literal(Value), Unary(Op, usize), Binary(Op, usize, usize), If(usize, usize, usize) }
#[derive(Clone, Debug)]
pub(crate) struct Node { pub kind: NodeKind, pub span: Span, pub ty: Type, depth: usize }

/// Opaque, validated arena: callers cannot insert cycles, untyped nodes, or out-of-range integers.
/// An arena also avoids recursively dropping a deeply nested rejected syntax tree.
#[derive(Clone, Debug)]
pub struct CompiledExpression { pub(crate) nodes: Vec<Node>, pub(crate) root: usize, source: String, inputs: Vec<InputSpec> }
impl CompiledExpression {
    pub fn inputs(&self) -> &[InputSpec] { &self.inputs }
    pub fn input_references(&self) -> Vec<InputReference> {
        self.nodes.iter().filter_map(|node| {
            if let NodeKind::Input(index) = &node.kind {
                let input = &self.inputs[*index];
                Some(InputReference { id: input.id.clone(), name: input.name.clone(), span: node.span })
            } else { None }
        }).collect()
    }
    /// Compare bound syntax, not all-input behavioral equivalence. Names and spans
    /// are deliberately excluded; input identities and operand order are retained.
    pub fn same_logic(&self, other: &Self) -> bool {
        fn same(a: &CompiledExpression, ai: usize, b: &CompiledExpression, bi: usize) -> bool {
            match (&a.nodes[ai].kind, &b.nodes[bi].kind) {
                (NodeKind::Input(i), NodeKind::Input(j)) => a.inputs[*i].id == b.inputs[*j].id,
                (NodeKind::Literal(x), NodeKind::Literal(y)) => x == y,
                (NodeKind::Unary(o, x), NodeKind::Unary(p, y)) => o == p && same(a, *x, b, *y),
                (NodeKind::Binary(o, l, r), NodeKind::Binary(p, x, y)) => o == p && same(a, *l, b, *x) && same(a, *r, b, *y),
                (NodeKind::If(c, t, f), NodeKind::If(d, u, v)) => same(a, *c, b, *d) && same(a, *t, b, *u) && same(a, *f, b, *v),
                _ => false,
            }
        }
        same(self, self.root, other, other.root)
    }
    pub fn source(&self) -> &str { &self.source }
    pub fn value_type(&self) -> Type { self.nodes[self.root].ty }
    pub fn span(&self) -> Span { self.nodes[self.root].span }
}
struct Parser { tokens: Vec<Token>, index: usize, nodes: Vec<Node>, nesting: usize, inputs: Vec<InputSpec> }
impl Parser {
    fn peek(&self) -> &Token { &self.tokens[self.index] }
    fn take(&mut self) -> Token { let t = self.peek().clone(); if !matches!(t.kind, TokenKind::End) { self.index += 1; } t }
    fn accept(&mut self, text: &str) -> bool { if self.peek().is(text) { self.index += 1; true } else { false } }
    fn expect(&mut self, text: &str) -> Result<(), Diagnostic> {
        if self.accept(text) { Ok(()) } else { Err(Diagnostic::new("PARSE_ERROR", format!("Expected {text}."), self.peek().span)) }
    }
    fn end(&self) -> usize { self.tokens[self.index.saturating_sub(1)].span.end }
    fn node(&mut self, kind: NodeKind, span: Span) -> Result<usize, Diagnostic> {
        let mismatch = |span| Diagnostic::new("TYPE_MISMATCH", "Expression operand types are incompatible.", span);
        let (ty, depth) = match &kind {
            NodeKind::Input(index) => (self.inputs[*index].input_type.base_type(), 1),
            NodeKind::Literal(v) => (v.value_type(), 1),
            NodeKind::Unary(op, child) => {
                let target = if *op == Op::Neg { Type::Int } else { Type::Bool };
                if self.nodes[*child].ty != target { return Err(mismatch(span)); }
                (target, self.nodes[*child].depth + 1)
            }
            NodeKind::Binary(op, left, right) => {
                let l = &self.nodes[*left]; let r = &self.nodes[*right];
                let (want, result) = match op {
                    Op::Add | Op::Sub | Op::Mul => (Type::Int, Type::Int),
                    Op::And | Op::Or => (Type::Bool, Type::Bool),
                    Op::Eq | Op::Ne => (l.ty, Type::Bool),
                    _ => (Type::Int, Type::Bool),
                };
                if l.ty != want { return Err(mismatch(l.span)); }
                if r.ty != want { return Err(mismatch(r.span)); }
                (result, l.depth.max(r.depth) + 1)
            }
            NodeKind::If(cond, yes, no) => {
                let c = &self.nodes[*cond]; let y = &self.nodes[*yes]; let n = &self.nodes[*no];
                if c.ty != Type::Bool { return Err(mismatch(c.span)); }
                if y.ty != n.ty { return Err(mismatch(span)); }
                (y.ty, c.depth.max(y.depth).max(n.depth) + 1)
            }
        };
        if depth > MAX_DEPTH { return Err(Diagnostic::new("PARSE_ERROR", "Expression tree depth exceeds 128.", span)); }
        let id = self.nodes.len(); self.nodes.push(Node { kind, span, ty, depth }); Ok(id)
    }
    fn expression(&mut self, min: u8) -> Result<usize, Diagnostic> {
        self.nesting += 1;
        if self.nesting > MAX_DEPTH { return Err(Diagnostic::new("PARSE_ERROR", "Expression nesting exceeds 128.", self.peek().span)); }
        let result = self.expression_inner(min);
        self.nesting -= 1;
        result
    }
    fn expression_inner(&mut self, min: u8) -> Result<usize, Diagnostic> {
        let token = self.take(); let start = token.span;
        let mut left = match token.kind {
            TokenKind::Literal(value) => self.node(NodeKind::Literal(value), start)?,
            _ if token.is("(") => { let id = self.expression(0)?; self.expect(")")?; id }
            _ if token.is("-") || token.is("!") || token.is("not") => {
                let op = if token.is("-") { Op::Neg } else { Op::Not };
                let child = self.expression(7)?;
                self.node(NodeKind::Unary(op, child), Span { end: self.end(), ..start })?
            }
            _ if token.is("if") => {
                let cond = self.expression(0)?; self.expect("then")?;
                let yes = self.expression(0)?; self.expect("else")?;
                let no = self.expression(0)?;
                self.node(NodeKind::If(cond, yes, no), Span { end: self.end(), ..start })?
            }
            TokenKind::Word(name) => {
                let index = self.inputs.iter().position(|s| s.name == name)
                    .ok_or_else(|| Diagnostic::new("UNKNOWN_NAME", format!("Unknown input {name}; calls are not supported here."), start))?;
                self.node(NodeKind::Input(index), start)?
            }
            _ => return Err(Diagnostic::new("PARSE_ERROR", "Expected an expression.", start)),
        };
        loop {
            let operation = match &self.peek().kind {
                TokenKind::Symbol(s) | TokenKind::Word(s) => match s.as_str() {
                    "or" | "||" => Some((1, Op::Or)), "and" | "&&" => Some((2, Op::And)),
                    "==" => Some((3, Op::Eq)), "!=" => Some((3, Op::Ne)),
                    "<" => Some((4, Op::Lt)), "<=" => Some((4, Op::Le)), ">" => Some((4, Op::Gt)), ">=" => Some((4, Op::Ge)),
                    "+" => Some((5, Op::Add)), "-" => Some((5, Op::Sub)), "*" => Some((6, Op::Mul)), _ => None,
                }, _ => None,
            };
            let Some((precedence, op)) = operation else { break; };
            if precedence < min { break; }
            self.take();
            let right = self.expression(precedence + 1)?;
            let span = Span { end: self.nodes[right].span.end, ..self.nodes[left].span };
            left = self.node(NodeKind::Binary(op, left, right), span)?;
        }
        Ok(left)
    }
}

pub fn compile_expression(source: &str) -> Result<CompiledExpression, Diagnostic> {
    compile_with_inputs(source, &[])
}

pub fn compile_with_inputs(source: &str, inputs: &[InputSpec]) -> Result<CompiledExpression, Diagnostic> {
    check_schema(inputs)?;
    let tokens = Lexer { source, byte: 0, offset: 0, line: 1, column: 1 }.tokens()?;
    let mut parser = Parser { tokens, index: 0, nodes: Vec::new(), nesting: 0, inputs: inputs.to_vec() };
    let root = parser.expression(0)?;
    if !matches!(parser.peek().kind, TokenKind::End) { return Err(Diagnostic::new("PARSE_ERROR", "Unexpected token after expression.", parser.peek().span)); }
    Ok(CompiledExpression { nodes: parser.nodes, root, source: source.to_owned(), inputs: parser.inputs })
}

/// CLI data accepts scalar literals, not general expressions or host code.
/// Parentheses around a literal are harmless and accepted by the expression parser.
pub fn parse_input_literal(source: &str) -> Result<Value, Diagnostic> {
    let program = compile_expression(source)?;
    match &program.nodes[program.root].kind {
        NodeKind::Literal(value) => Ok(value.clone()),
        NodeKind::Unary(Op::Neg, child) => match &program.nodes[*child].kind {
            NodeKind::Literal(Value::Int(n)) => Ok(Value::Int(-n)),
            _ => Err(Diagnostic::new("INPUT_LITERAL", "Expected a scalar literal, not a calculation.", program.span())),
        },
        _ => Err(Diagnostic::new("INPUT_LITERAL", "Expected a scalar literal, not a calculation.", program.span())),
    }
}
