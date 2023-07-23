#[derive(Debug, Clone, PartialEq, Eq)]
/// Information about a hook call or function.
pub struct HookInfo {
    /// The name of the hook, e.g. `use_state`.
    pub name: String,
    /// The span of the hook, e.g. `use_state(cx, || 0)`.
    pub span: Span,
    /// The span of the name, e.g. `use_state`.
    pub name_span: Span,
}

impl HookInfo {
    pub const fn new(span: Span, name_span: Span, name: String) -> Self {
        Self {
            span,
            name_span,
            name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionalInfo {
    If(IfInfo),
    Match(MatchInfo),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfInfo {
    /// The span of the `if` statement, e.g. `if true { ... }`.
    pub span: Span,
    /// The span of the `if` keyword only.
    pub keyword_span: Span,
}

impl IfInfo {
    pub const fn new(span: Span, keyword_span: Span) -> Self {
        Self { span, keyword_span }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchInfo {
    /// The span of the `match` statement, e.g. `match true { ... }`.
    pub span: Span,
    /// The span of the `match` keyword only.
    pub keyword_span: Span,
}

impl MatchInfo {
    pub const fn new(span: Span, keyword_span: Span) -> Self {
        Self { span, keyword_span }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Information about one of the possible loop types.
pub enum AnyLoopInfo {
    For(ForInfo),
    While(WhileInfo),
    Loop(LoopInfo),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Information about a `for` loop.
pub struct ForInfo {
    pub span: Span,
    pub keyword_span: Span,
}

impl ForInfo {
    pub const fn new(span: Span, keyword_span: Span) -> Self {
        Self { span, keyword_span }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Information about a `while` loop.
pub struct WhileInfo {
    pub span: Span,
    pub keyword_span: Span,
}

impl WhileInfo {
    pub const fn new(span: Span, keyword_span: Span) -> Self {
        Self { span, keyword_span }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Information about a `loop` loop.
pub struct LoopInfo {
    pub span: Span,
    pub keyword_span: Span,
}

impl LoopInfo {
    pub const fn new(span: Span, keyword_span: Span) -> Self {
        Self { span, keyword_span }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Information about a closure.
pub struct ClosureInfo {
    pub span: Span,
}

impl ClosureInfo {
    pub const fn new(span: Span) -> Self {
        Self { span }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Information about a component function.
pub struct ComponentInfo {
    pub span: Span,
    pub name: String,
    pub name_span: Span,
}

impl ComponentInfo {
    pub const fn new(span: Span, name: String, name_span: Span) -> Self {
        Self {
            span,
            name,
            name_span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Information about a non-component, non-hook function.
pub struct FnInfo {
    pub span: Span,
    pub name: String,
    pub name_span: Span,
}

impl FnInfo {
    pub const fn new(span: Span, name: String, name_span: Span) -> Self {
        Self {
            span,
            name,
            name_span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A span of text in a source code file.
pub struct Span {
    pub start: LineColumn,
    pub end: LineColumn,
}

impl From<proc_macro2::Span> for Span {
    fn from(span: proc_macro2::Span) -> Self {
        Self {
            start: span.start().into(),
            end: span.end().into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A location in a source code file.
pub struct LineColumn {
    pub line: usize,
    pub column: usize,
}

impl From<proc_macro2::LineColumn> for LineColumn {
    fn from(lc: proc_macro2::LineColumn) -> Self {
        Self {
            line: lc.line,
            column: lc.column,
        }
    }
}
