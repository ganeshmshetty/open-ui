//! Batch AST parser. Push uses the same lexer and splitter, with no boundary rescans.
use super::{
    LocatedToken, TokenKind as K,
    ast::*,
    extract::{ExtractResult, Extractor, Segment},
    splitter::{SplitResult, SplitStatement, Splitter, lower_name},
};
use crate::{
    diag::{Code, Diagnostic, Span, order_diagnostics},
    limits::SyntaxLimits,
};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParseResult {
    pub program: Program,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct BatchParser {
    splitter: Splitter,
    limits: SyntaxLimits,
    accepted: usize,
    truncated: bool,
}
impl BatchParser {
    pub fn new(limits: SyntaxLimits) -> Self {
        let limits = limits.bounded();
        Self {
            splitter: Splitter::new(limits.clone()),
            limits,
            accepted: 0,
            truncated: false,
        }
    }
    pub fn push(&mut self, bytes: &[u8]) {
        let available = self.limits.max_program_bytes.saturating_sub(self.accepted);
        let count = available.min(bytes.len());
        self.splitter.push(&bytes[..count]);
        self.accepted += count;
        self.truncated |= count < bytes.len();
    }
    pub fn finish(self) -> ParseResult {
        let mut result = parse_split(self.splitter.finish(), &self.limits);
        if self.truncated {
            result
                .diagnostics
                .push(Diagnostic::new(Code::ProgramSize, None, None));
        }
        order_diagnostics(&mut result.diagnostics);
        result
    }
}
impl Default for BatchParser {
    fn default() -> Self {
        Self::new(SyntaxLimits::default())
    }
}
pub fn parse_batch(bytes: &[u8], limits: SyntaxLimits) -> ParseResult {
    let mut parser = BatchParser::new(limits);
    parser.push(bytes);
    parser.finish()
}

pub struct MessageParser {
    extractor: Extractor,
    limits: SyntaxLimits,
}
impl MessageParser {
    pub fn new(limits: SyntaxLimits) -> Self {
        Self {
            extractor: Extractor::default(),
            limits,
        }
    }
    pub fn push(&mut self, bytes: &[u8]) {
        self.extractor.push(bytes);
    }
    pub fn finish(self) -> (ExtractResult, ParseResult) {
        let extracted = self.extractor.finish();
        let mut parsed = if extracted.rejected {
            ParseResult::default()
        } else if let Some(Segment::Program(source)) = extracted
            .segments
            .iter()
            .find(|s| matches!(s, Segment::Program(_)))
        {
            parse_batch(source.as_bytes(), self.limits)
        } else {
            ParseResult::default()
        };
        parsed.diagnostics.extend(extracted.diagnostics.clone());
        order_diagnostics(&mut parsed.diagnostics);
        (extracted, parsed)
    }
}
pub fn parse_message(bytes: &[u8], limits: SyntaxLimits) -> (ExtractResult, ParseResult) {
    let mut parser = MessageParser::new(limits);
    parser.push(bytes);
    parser.finish()
}

fn parse_split(split: SplitResult, limits: &SyntaxLimits) -> ParseResult {
    let mut result = ParseResult {
        program: Program::default(),
        diagnostics: split.diagnostics,
    };
    let recovered_nulls: BTreeSet<_> = result
        .diagnostics
        .iter()
        .filter(|d| matches!(d.code, Code::Overflow | Code::NonFinite | Code::Depth))
        .filter_map(|d| d.span.as_ref().map(|s| s.start))
        .collect();
    let mut names = BTreeSet::new();
    let mut count = 0;
    for statement in split.statements {
        let Some(header) = &statement.header else {
            result
                .diagnostics
                .push(Diagnostic::new(Code::StmtShape, None, Some(statement.span)));
            continue;
        };
        let stmt = Some(header.name.clone());
        count += 1;
        let rejection = if count > limits.max_stmt_count {
            Some(Code::StmtCount)
        } else if header.name.starts_with("__") {
            Some(Code::ReservedName)
        } else if !names.insert((header.kind == StatementKind::State, header.name.clone())) {
            Some(Code::DuplicateName)
        } else {
            None
        };
        if let Some(code) = rejection {
            result
                .diagnostics
                .push(Diagnostic::new(code, stmt, Some(statement.span)));
            continue;
        }
        let mut parser = ExpressionParser {
            statement: &statement,
            pos: header.rhs,
            limits,
            diagnostics: Vec::new(),
            lambda_depth: 0,
            depth_reported: false,
            recovered_nulls: &recovered_nulls,
        };
        let expression = parser.expression(0, 0, false);
        match expression {
            Ok(value) if header.kind != StatementKind::Query || valid_query(&value) => {
                if parser.pos < statement.tokens.len() {
                    parser.diagnostic(Code::Trailing);
                }
                result.program.statements.push(Statement {
                    kind: header.kind,
                    name: header.name.clone(),
                    value,
                });
            }
            _ => parser.diagnostic(Code::StmtShape),
        }
        result.diagnostics.extend(parser.diagnostics);
    }
    order_diagnostics(&mut result.diagnostics);
    result
}
fn valid_query(expr: &Expr) -> bool {
    matches!(expr, Expr::Call { name, args } if name == "tool" && args.len() >= 2
        && args[0].name.is_none() && matches!(args[0].value, Expr::String { .. }) && args[1].name.is_none()
        && args[2..].iter().all(|a| a.name.is_some()))
}

struct ExpressionParser<'a> {
    statement: &'a SplitStatement,
    pos: usize,
    limits: &'a SyntaxLimits,
    diagnostics: Vec<Diagnostic>,
    lambda_depth: usize,
    depth_reported: bool,
    recovered_nulls: &'a BTreeSet<usize>,
}
type Parsed<T> = Result<T, ()>;
impl ExpressionParser<'_> {
    fn peek(&self) -> Option<K> {
        self.statement.tokens.get(self.pos).map(|t| t.token.kind)
    }
    fn eat(&mut self, kind: K) -> bool {
        if self.peek() == Some(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn require(&mut self, kind: K) -> Parsed<()> {
        if self.eat(kind) { Ok(()) } else { Err(()) }
    }
    fn next(&mut self) -> Parsed<LocatedToken> {
        let t = self.statement.tokens.get(self.pos).cloned().ok_or(())?;
        self.pos += 1;
        Ok(t)
    }
    fn span(&self) -> Span {
        self.statement
            .tokens
            .get(self.pos)
            .map_or_else(|| self.statement.span.clone(), |t| t.span.clone())
    }
    fn diagnostic(&mut self, code: Code) {
        self.diagnostics.push(Diagnostic::new(
            code,
            self.statement.header.as_ref().map(|h| h.name.clone()),
            Some(self.span()),
        ));
    }
    fn depth_limit(&mut self) {
        if !self.depth_reported {
            self.depth_reported = true;
            self.diagnostic(Code::Depth);
        }
    }
    fn skip_expression(&mut self) {
        let mut depth = 0usize;
        while let Some(kind) = self.peek() {
            match kind {
                K::LParen | K::LBracket | K::LBrace => depth += 1,
                K::RParen | K::RBracket | K::RBrace if depth == 0 => break,
                K::RParen | K::RBracket | K::RBrace => depth -= 1,
                K::Comma if depth == 0 => break,
                _ => {}
            }
            self.pos += 1;
        }
    }
    fn expression(&mut self, minimum: u8, depth: usize, allow_lambda: bool) -> Parsed<Expr> {
        if depth >= self.limits.max_expr_depth {
            self.depth_limit();
            self.skip_expression();
            return Ok(Expr::recovered_null());
        }
        let mut left = self.primary(depth, allow_lambda)?;
        let mut used_eq = false;
        let mut used_cmp = false;
        let mut previous_precedence = None;
        while let Some((op, precedence)) = self.peek().and_then(operator) {
            if previous_precedence.is_some_and(|previous| precedence > previous)
                || precedence < minimum
                || (precedence == 3 && used_eq)
                || (precedence == 4 && used_cmp)
            {
                break;
            }
            if depth.saturating_add(tree_depth(&left)) >= self.limits.max_expr_depth {
                self.depth_limit();
                self.skip_expression();
                break;
            }
            previous_precedence = Some(precedence);
            used_eq |= precedence == 3;
            used_cmp |= precedence == 4;
            self.pos += 1;
            let right = self.expression(precedence + 1, depth + 1, false)?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }
    fn primary(&mut self, depth: usize, allow_lambda: bool) -> Parsed<Expr> {
        let token = self.next()?;
        let mut value = match token.token.kind {
            K::Null => {
                if self.recovered_nulls.contains(&token.span.start) {
                    Expr::recovered_null()
                } else {
                    Expr::null()
                }
            }
            K::True | K::False => Expr::Bool {
                value: token.token.kind == K::True,
            },
            K::Int => Expr::Int {
                value: token.token.text.replace('_', "").parse().map_err(|_| ())?,
            },
            K::Float => {
                let value: f64 = token.token.text.replace('_', "").parse().map_err(|_| ())?;
                if !value.is_finite() {
                    return Err(());
                }
                Expr::Float { value }
            }
            K::String => {
                let mut value = token.token.text;
                if value.len() > self.limits.max_string_bytes {
                    let mut end = self.limits.max_string_bytes;
                    while !value.is_char_boundary(end) {
                        end -= 1;
                    }
                    value.truncate(end);
                    self.diagnostic(Code::StringSize);
                }
                Expr::String { value }
            }
            K::State => Expr::State {
                name: token.token.text,
            },
            K::Name => {
                let name = token.token.text;
                if self.eat(K::LParen) {
                    let args = self.arguments(
                        depth + 1,
                        matches!(name.as_str(), "each" | "map" | "filter" | "sort_by"),
                    )?;
                    let query_root = self
                        .statement
                        .header
                        .as_ref()
                        .is_some_and(|h| h.kind == StatementKind::Query)
                        && depth == 0;
                    if name == "tool" && !query_root {
                        self.diagnostic(Code::UnknownFn);
                        if let Some(d) = self.diagnostics.last_mut() {
                            d.hint = Some(
                                "Use query name = tool(\"tool.id\", args, default: value).".into(),
                            );
                        }
                        Expr::recovered_null()
                    } else {
                        Expr::Call { name, args }
                    }
                } else if name.as_bytes().first().is_some_and(u8::is_ascii_uppercase) {
                    self.diagnostic(Code::BareComponent);
                    Expr::Call {
                        name,
                        args: Vec::new(),
                    }
                } else {
                    Expr::Name { name }
                }
            }
            K::Minus | K::Not => {
                let expr = self.expression(7, depth + 1, false)?;
                return Ok(Expr::Unary {
                    op: if token.token.kind == K::Minus {
                        UnaryOp::Neg
                    } else {
                        UnaryOp::Not
                    },
                    expr: Box::new(expr),
                });
            }
            K::LParen => {
                let value = self.expression(0, depth + 1, false)?;
                self.require(K::RParen)?;
                value
            }
            K::LBracket => Expr::List {
                items: self.list(depth + 1)?,
            },
            K::LBrace => Expr::Map {
                entries: self.map(depth + 1)?,
            },
            K::Pipe => return self.lambda(depth + 1, allow_lambda),
            _ => return Err(()),
        };
        loop {
            if depth.saturating_add(tree_depth(&value)) >= self.limits.max_expr_depth {
                if matches!(self.peek(), Some(K::Dot | K::LBracket)) {
                    self.depth_limit();
                    self.skip_expression();
                }
                break;
            }
            if self.eat(K::Dot) {
                let key = self.next()?;
                if !word(key.token.kind) {
                    return Err(());
                }
                value = Expr::Member {
                    base: Box::new(value),
                    key: key.token.text,
                };
            } else if self.eat(K::LBracket) {
                let index = self.expression(0, depth + 1, false)?;
                self.require(K::RBracket)?;
                value = Expr::Index {
                    base: Box::new(value),
                    index: Box::new(index),
                };
            } else {
                break;
            }
        }
        Ok(value)
    }
    fn list(&mut self, depth: usize) -> Parsed<Vec<Expr>> {
        let mut items = Vec::new();
        while self.peek() != Some(K::RBracket) {
            items.push(self.expression(0, depth, false)?);
            self.separator(K::RBracket)?;
        }
        self.require(K::RBracket)?;
        Ok(items)
    }
    fn map(&mut self, depth: usize) -> Parsed<Vec<Entry>> {
        let mut entries = Vec::new();
        let mut keys = BTreeSet::new();
        while self.peek() != Some(K::RBrace) {
            let key = self.next()?;
            if !word(key.token.kind) && key.token.kind != K::String {
                return Err(());
            }
            self.require(K::Colon)?;
            let value = self.expression(0, depth, false)?;
            if keys.insert(key.token.text.clone()) {
                entries.push(Entry {
                    key: key.token.text,
                    value,
                });
            } else {
                self.diagnostic(Code::DupKey);
            }
            self.separator(K::RBrace)?;
        }
        self.require(K::RBrace)?;
        Ok(entries)
    }
    fn arguments(&mut self, depth: usize, allow_lambda: bool) -> Parsed<Vec<Argument>> {
        let mut args = Vec::new();
        let mut named = false;
        while self.peek() != Some(K::RParen) {
            let name = if self.peek().is_some_and(word)
                && self
                    .statement
                    .tokens
                    .get(self.pos + 1)
                    .is_some_and(|t| t.token.kind == K::Colon)
            {
                let name = self.next()?.token.text;
                self.pos += 1;
                Some(name)
            } else {
                None
            };
            let value = self.expression(0, depth, allow_lambda)?;
            if named && name.is_none() {
                self.diagnostic(Code::PositionalAfterNamed);
            } else {
                named |= name.is_some();
                args.push(Argument { name, value });
            }
            self.separator(K::RParen)?;
        }
        self.require(K::RParen)?;
        Ok(args)
    }
    fn separator(&mut self, closer: K) -> Parsed<()> {
        if self.eat(K::Comma) || self.peek() == Some(closer) {
            Ok(())
        } else if self.peek().is_some_and(starts_expression) {
            self.diagnostic(Code::MissingComma);
            Ok(())
        } else {
            Err(())
        }
    }
    fn lambda(&mut self, depth: usize, allowed: bool) -> Parsed<Expr> {
        let mut params = vec![self.next()?];
        if self.eat(K::Comma) {
            params.push(self.next()?);
        }
        if !params.iter().all(|t| lower_name(&t.token)) {
            return Err(());
        }
        self.require(K::Pipe)?;
        let legal = allowed && self.lambda_depth == 0;
        self.lambda_depth += 1;
        let body = self.expression(0, depth, false);
        self.lambda_depth -= 1;
        let body = body?;
        if !legal {
            self.diagnostic(Code::LambdaMisuse);
            Ok(Expr::recovered_null())
        } else {
            Ok(Expr::Lambda {
                params: params.into_iter().map(|t| t.token.text).collect(),
                body: Box::new(body),
            })
        }
    }
}
fn word(kind: K) -> bool {
    matches!(kind, K::Name | K::True | K::False | K::Null)
}
fn starts_expression(kind: K) -> bool {
    matches!(
        kind,
        K::Name
            | K::State
            | K::Int
            | K::Float
            | K::String
            | K::True
            | K::False
            | K::Null
            | K::LParen
            | K::LBracket
            | K::LBrace
            | K::Minus
            | K::Not
            | K::Pipe
    )
}
fn operator(kind: K) -> Option<(BinaryOp, u8)> {
    use BinaryOp::*;
    Some(match kind {
        K::Or => (Or, 1),
        K::And => (And, 2),
        K::Eq => (Eq, 3),
        K::Ne => (Ne, 3),
        K::Lt => (Lt, 4),
        K::Le => (Le, 4),
        K::Gt => (Gt, 4),
        K::Ge => (Ge, 4),
        K::Plus => (Add, 5),
        K::Minus => (Sub, 5),
        K::Star => (Mul, 6),
        K::Slash => (Div, 6),
        K::Percent => (Rem, 6),
        _ => return None,
    })
}
fn tree_depth(expr: &Expr) -> usize {
    1 + match expr {
        Expr::Unary { expr, .. }
        | Expr::Member { base: expr, .. }
        | Expr::Lambda { body: expr, .. } => tree_depth(expr),
        Expr::Binary { left, right, .. }
        | Expr::Index {
            base: left,
            index: right,
        } => tree_depth(left).max(tree_depth(right)),
        Expr::List { items } => items.iter().map(tree_depth).max().unwrap_or(0),
        Expr::Map { entries } => entries
            .iter()
            .map(|e| tree_depth(&e.value))
            .max()
            .unwrap_or(0),
        Expr::Call { args, .. } => args.iter().map(|a| tree_depth(&a.value)).max().unwrap_or(0),
        _ => 0,
    }
}
