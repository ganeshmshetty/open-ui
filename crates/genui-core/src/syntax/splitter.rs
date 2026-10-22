//! The sole statement-boundary algorithm (SPEC §2.4), used by batch and push.
use super::{LexEvent, Lexer, LocatedToken, Token, TokenKind as K, ast::StatementKind};
use crate::{
    diag::{Code, Diagnostic, Span, order_diagnostics},
    limits::SyntaxLimits,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Header {
    pub kind: StatementKind,
    pub name: String,
    pub rhs: usize,
}

pub fn statement_start(tokens: &[LocatedToken]) -> Option<Header> {
    let first = tokens.first()?;
    let second = tokens.get(1)?;
    if matches!(second.token.kind, K::Assign | K::Bind) {
        let kind = if first.token.kind == K::State {
            StatementKind::State
        } else if lower_name(&first.token) {
            StatementKind::Bind
        } else {
            return None;
        };
        return Some(Header {
            kind,
            name: first.token.text.clone(),
            rhs: 2,
        });
    }
    if first.token.kind == K::Name
        && first.token.text == "query"
        && lower_name(&second.token)
        && tokens
            .get(2)
            .is_some_and(|t| matches!(t.token.kind, K::Assign | K::Bind))
    {
        return Some(Header {
            kind: StatementKind::Query,
            name: second.token.text.clone(),
            rhs: 3,
        });
    }
    None
}
pub(crate) fn lower_name(token: &Token) -> bool {
    token.kind == K::Name
        && token
            .text
            .as_bytes()
            .first()
            .is_some_and(|b| b.is_ascii_lowercase() || *b == b'_')
}

#[derive(Clone, Debug, PartialEq)]
pub struct SplitStatement {
    pub tokens: Vec<LocatedToken>,
    pub header: Option<Header>,
    pub span: Span,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SplitResult {
    pub statements: Vec<SplitStatement>,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct Splitter {
    lexer: Lexer,
    state: ScanState,
}
impl Splitter {
    pub fn new(limits: SyntaxLimits) -> Self {
        Self {
            lexer: Lexer::default(),
            state: ScanState::new(limits.bounded()),
        }
    }
    pub fn push(&mut self, bytes: &[u8]) {
        self.lexer.push(bytes);
        let events = self.lexer.take_events();
        self.state.events(events, self.lexer.normalized_source());
    }
    pub fn sealed(&self) -> &[SplitStatement] {
        &self.state.result.statements
    }
    pub fn finish(self) -> SplitResult {
        let (source, events) = self.lexer.finish_events();
        let mut state = self.state;
        state.events(events, &source);
        state.begin_line(&source);
        state.check_size(source.len(), &source);
        state.seal(false);
        if let Some(start) = state.skip_from.take() {
            state.skipped_ranges.push((start, source.len()));
        }
        state.result.diagnostics.retain(|diagnostic| {
            if !matches!(
                diagnostic.code,
                Code::BadChar
                    | Code::BadEscape
                    | Code::UnterminatedString
                    | Code::Overflow
                    | Code::NonFinite
            ) {
                return true;
            }
            diagnostic.span.as_ref().is_none_or(|span| {
                !state
                    .skipped_ranges
                    .iter()
                    .any(|(start, end)| span.start < *end && span.end > *start)
            })
        });
        order_diagnostics(&mut state.result.diagnostics);
        state.result
    }
}
impl Default for Splitter {
    fn default() -> Self {
        Self::new(SyntaxLimits::default())
    }
}
pub fn split(bytes: &[u8], limits: SyntaxLimits) -> SplitResult {
    let mut splitter = Splitter::new(limits);
    splitter.push(bytes);
    splitter.finish()
}

struct Open {
    tokens: Vec<LocatedToken>,
    header: Option<Header>,
    span: Span,
    brackets: Vec<(K, bool)>,
    depth_reported: bool,
}
struct ScanState {
    limits: SyntaxLimits,
    result: SplitResult,
    open: Option<Open>,
    prefix: Vec<LocatedToken>,
    line_decided: bool,
    line_start: usize,
    skipping: bool,
    skip_from: Option<usize>,
    skipped_ranges: Vec<(usize, usize)>,
}
impl ScanState {
    fn new(limits: SyntaxLimits) -> Self {
        Self {
            limits,
            result: SplitResult::default(),
            open: None,
            prefix: Vec::new(),
            line_decided: false,
            line_start: 0,
            skipping: false,
            skip_from: None,
            skipped_ranges: Vec::new(),
        }
    }
    fn events(&mut self, events: Vec<LexEvent>, source: &str) {
        for event in events {
            match event {
                LexEvent::Diagnostic(d) => self.result.diagnostics.push(d),
                LexEvent::Token(t) if t.token.kind == K::Nl => {
                    self.begin_line(source);
                    self.check_size(t.span.start, source);
                    if let Some(open) = &mut self.open {
                        open.span.end = t.span.start;
                    }
                    if self.open.as_ref().is_some_and(|s| {
                        s.brackets.is_empty()
                            && !s.tokens.last().is_some_and(|t| dangling(t.token.kind))
                    }) {
                        self.seal(false);
                    }
                    self.line_decided = false;
                    self.line_start = t.span.end;
                }
                LexEvent::Token(t) => {
                    if self.line_decided {
                        self.token(t, source);
                    } else {
                        self.prefix.push(t);
                        let first = self.prefix.first();
                        let decidable = statement_start(&self.prefix).is_some()
                            || self.prefix.len() >= 3
                            || first
                                .is_some_and(|t| !lower_name(&t.token) && t.token.kind != K::State)
                            || (self.prefix.len() == 2
                                && first.is_some_and(|t| t.token.text != "query"));
                        if decidable {
                            self.begin_line(source);
                        }
                    }
                }
            }
        }
    }
    fn begin_line(&mut self, source: &str) {
        if self.line_decided {
            return;
        }
        self.line_decided = true;
        if self.prefix.is_empty() {
            return;
        }
        let header = statement_start(&self.prefix);
        let column_zero = self.prefix.first().is_some_and(|t| t.span.column == 1);
        if column_zero && header.is_some() {
            self.seal(false);
            if let Some(start) = self.skip_from.take() {
                let end = self.prefix.first().map_or(source.len(), |t| t.span.start);
                self.skipped_ranges.push((start, end));
            }
            self.skipping = false;
        }
        if self.skipping {
            self.prefix.clear();
            return;
        }
        if self.open.is_none()
            && let Some(first) = self.prefix.first()
        {
            let mut span = first.span.clone();
            span.start = self.line_start;
            span.column = 1;
            self.open = Some(Open {
                tokens: Vec::new(),
                header,
                span,
                brackets: Vec::new(),
                depth_reported: false,
            });
        }
        for token in std::mem::take(&mut self.prefix) {
            self.token(token, source);
        }
    }
    fn name(&self) -> Option<String> {
        self.open
            .as_ref()
            .and_then(|s| s.header.as_ref().map(|h| h.name.clone()))
    }
    fn diagnostic(&mut self, code: Code, span: Option<Span>) {
        let name = self.name();
        self.result
            .diagnostics
            .push(Diagnostic::new(code, name, span));
    }
    fn token(&mut self, token: LocatedToken, source: &str) {
        if self.skipping {
            return;
        }
        if self.check_size(token.span.end, source) {
            return;
        }
        self.repair(token, false);
    }
    fn repair(&mut self, token: LocatedToken, suppress: bool) {
        let Some(open) = &mut self.open else {
            return;
        };
        open.span.end = token.span.end;
        let kind = token.token.kind;
        if let Some(close) = closing(kind) {
            let skip = open.brackets.len() >= self.limits.max_bracket_depth;
            if skip && !open.brackets.iter().any(|(_, skip)| *skip) {
                if !open.depth_reported {
                    open.depth_reported = true;
                    let name = open.header.as_ref().map(|h| h.name.clone());
                    self.result.diagnostics.push(Diagnostic::new(
                        Code::Depth,
                        name,
                        Some(token.span.clone()),
                    ));
                }
                open.tokens
                    .push(synthetic(K::Null, "null", token.span.clone()));
            }
            open.brackets.push((close, skip));
            if !skip {
                open.tokens.push(token);
            }
        } else if matches!(kind, K::RParen | K::RBracket | K::RBrace) {
            if let Some(index) = open.brackets.iter().rposition(|(k, _)| *k == kind) {
                while open.brackets.len() > index + 1 {
                    if let Some((close, skipped)) = open.brackets.pop()
                        && !skipped
                    {
                        open.tokens
                            .push(synthetic(close, spelling(close), token.span.clone()));
                        if !suppress {
                            self.result.diagnostics.push(Diagnostic::new(
                                Code::UnclosedBracket,
                                open.header.as_ref().map(|h| h.name.clone()),
                                Some(token.span.clone()),
                            ));
                        }
                    }
                }
                if let Some((_, skipped)) = open.brackets.pop()
                    && !skipped
                {
                    open.tokens.push(token);
                }
            } else if !suppress && !open.brackets.iter().any(|(_, skipped)| *skipped) {
                self.result.diagnostics.push(Diagnostic::new(
                    Code::StrayCloser,
                    open.header.as_ref().map(|h| h.name.clone()),
                    Some(token.span),
                ));
            }
        } else if !open.brackets.iter().any(|(_, skipped)| *skipped) {
            open.tokens.push(token);
        }
    }
    fn check_size(&mut self, end: usize, source: &str) -> bool {
        let Some(open) = &self.open else {
            return false;
        };
        if end.saturating_sub(open.span.start) <= self.limits.max_stmt_bytes {
            return false;
        }
        let start = open.span.start;
        let start_line = open.span.line;
        let mut cut = (start + self.limits.max_stmt_bytes).min(source.len());
        while !source.is_char_boundary(cut) {
            cut = cut.saturating_sub(1);
        }
        self.diagnostic(
            Code::StmtSize,
            Some(Span {
                end: cut,
                ..open.span.clone()
            }),
        );
        let result = super::lex(&source.as_bytes()[start..cut]);
        if let Some(open) = &mut self.open {
            open.tokens.clear();
            open.brackets.clear();
        }
        for (token, mut span) in result.tokens.into_iter().zip(result.spans) {
            if token.kind == K::Nl {
                continue;
            }
            span.line += start_line - 1;
            span.start += start;
            span.end += start;
            self.repair(LocatedToken { token, span }, true);
        }
        self.seal(true);
        self.skipping = true;
        self.skip_from = Some(cut);
        true
    }
    fn seal(&mut self, suppress: bool) {
        let Some(mut open) = self.open.take() else {
            return;
        };
        let stmt = open.header.as_ref().map(|h| h.name.clone());
        if open.tokens.last().is_some_and(|t| dangling(t.token.kind)) {
            if !suppress {
                self.result.diagnostics.push(Diagnostic::new(
                    Code::TrailingOperator,
                    stmt.clone(),
                    Some(open.span.clone()),
                ));
            }
            // Keep the binder so the parser can still claim a name on an empty RHS.
            let keep = open.header.as_ref().map_or(0, |h| h.rhs);
            while open.tokens.len() > keep
                && open.tokens.last().is_some_and(|t| dangling(t.token.kind))
            {
                open.tokens.pop();
            }
        }
        while let Some((close, skipped)) = open.brackets.pop() {
            if !skipped {
                open.tokens
                    .push(synthetic(close, spelling(close), open.span.clone()));
                if !suppress {
                    self.result.diagnostics.push(Diagnostic::new(
                        Code::UnclosedBracket,
                        stmt.clone(),
                        Some(open.span.clone()),
                    ));
                }
            }
        }
        self.result.statements.push(SplitStatement {
            tokens: open.tokens,
            header: open.header,
            span: open.span,
        });
    }
}
fn synthetic(kind: K, text: &str, span: Span) -> LocatedToken {
    LocatedToken {
        token: Token {
            kind,
            text: text.into(),
        },
        span,
    }
}
fn closing(kind: K) -> Option<K> {
    match kind {
        K::LParen => Some(K::RParen),
        K::LBracket => Some(K::RBracket),
        K::LBrace => Some(K::RBrace),
        _ => None,
    }
}
fn spelling(kind: K) -> &'static str {
    match kind {
        K::RParen => ")",
        K::RBracket => "]",
        K::RBrace => "}",
        _ => "",
    }
}
fn dangling(kind: K) -> bool {
    matches!(
        kind,
        K::Assign
            | K::Bind
            | K::Eq
            | K::Ne
            | K::Lt
            | K::Le
            | K::Gt
            | K::Ge
            | K::Plus
            | K::Minus
            | K::Star
            | K::Slash
            | K::Percent
            | K::And
            | K::Or
            | K::Comma
            | K::Colon
            | K::Pipe
            | K::Not
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookahead_releases_decidable_non_header_prefixes() {
        // Once resync is impossible, consume the prefix without waiting for LF.
        // Valid starts still seal via statement_start; these exercise continuations.
        for prefix in ["[ ", "$x + ", "query x + "] {
            let mut splitter = Splitter::default();
            splitter.push(b"root=[\n");
            splitter.push(prefix.as_bytes());
            assert!(splitter.state.line_decided, "{prefix}");
            assert!(splitter.state.prefix.is_empty(), "{prefix}");
            assert!(splitter.sealed().is_empty(), "{prefix}");
        }
    }
}
