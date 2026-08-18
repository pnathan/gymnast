use crate::span::Span;

/// Identifier with source location.
#[derive(Debug, Clone, PartialEq)]
pub struct Ident {
    /// The identifier text.
    pub text: String,
    /// Source span of this identifier.
    pub span: Span,
}

/// A complete specification file.
#[derive(Debug, Clone, PartialEq)]
pub struct File {
    /// The spec declaration.
    pub spec: SpecDecl,
    /// All other declarations.
    pub decls: Vec<Decl>,
}

/// Specification header declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct SpecDecl {
    /// Specification name.
    pub name: Ident,
    /// Version string (e.g., "0.1").
    pub version: String,
    /// Owner identifier.
    pub owner: Ident,
    /// Exported names.
    pub exports: Vec<Ident>,
    /// Source span.
    pub span: Span,
}

/// A declaration in the specification.
#[derive(Debug, Clone, PartialEq)]
pub enum Decl {
    /// use declaration.
    Use(UseDecl),
    /// application declaration.
    Application(ApplicationDecl),
    /// actor declaration.
    Actor(ActorDecl),
    /// mode declaration.
    Mode(ModeDecl),
    /// component declaration.
    Component(ComponentDecl),
    /// interface declaration.
    Interface(InterfaceDecl),
    /// state declaration.
    State(StateDecl),
    /// flow declaration.
    Flow(FlowDecl),
    /// behavior declaration.
    Behavior(BehaviorDecl),
    /// invariant declaration.
    Invariant(InvariantDecl),
    /// constraint declaration.
    Constraint(ConstraintDecl),
    /// synthesis declaration.
    Synthesis(SynthesisDecl),
    /// acceptance declaration.
    Acceptance(AcceptanceDecl),
}

/// Use declaration for profile imports.
#[derive(Debug, Clone, PartialEq)]
pub struct UseDecl {
    /// Profile path (e.g., oddities/profiles/todo_standard).
    pub path: Vec<Ident>,
    /// Version string (e.g., "1.0").
    pub version: String,
    /// Configuration arguments.
    pub args: Pack,
    /// Source span.
    pub span: Span,
}

/// Application declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct ApplicationDecl {
    /// Application name.
    pub name: Ident,
    /// Attributes pack.
    pub attrs: Pack,
    /// Source span.
    pub span: Span,
}

/// Actor declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct ActorDecl {
    /// Actor name.
    pub name: Ident,
    /// Actor kind (e.g., person).
    pub kind: Ident,
    /// Attributes pack.
    pub attrs: Pack,
    /// Source span.
    pub span: Span,
}

/// Mode declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct ModeDecl {
    /// Mode name.
    pub name: Ident,
    /// Mode expression.
    pub expr: ModeExpr,
    /// Source span.
    pub span: Span,
}

/// Mode expression (type definition).
#[derive(Debug, Clone, PartialEq)]
pub enum ModeExpr {
    /// Opaque wrapper around another mode.
    Opaque(Box<ModeExpr>),
    /// Enumeration of named values.
    Enum(Vec<Ident>),
    /// Union/sum type with tagged alternatives.
    Union(Vec<(Ident, ModeExpr)>),
    /// Struct/record with typed fields.
    Struct(Vec<Field>),
    /// Optional value.
    Opt(Box<ModeExpr>),
    /// Row/array type (reserved in phase 1).
    Row(Box<ModeExpr>),
    /// Named type reference (possibly parameterized).
    Named {
        /// Type name.
        name: Ident,
        /// Type arguments.
        args: Vec<ModeExpr>,
    },
    /// Refined primitive with optional bounds.
    Refined {
        /// Type name (e.g., text, int).
        name: Ident,
        /// Lower bound (inclusive).
        lo: Option<i64>,
        /// Upper bound (inclusive).
        hi: Option<i64>,
    },
}

/// A struct field with type-first syntax.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// The field mode/type.
    pub mode: ModeExpr,
    /// The field name.
    pub name: Ident,
}

/// Component declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentDecl {
    /// Component name.
    pub name: Ident,
    /// Attributes pack.
    pub attrs: Pack,
    /// Source span.
    pub span: Span,
}

/// Interface declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceDecl {
    /// Interface name.
    pub name: Ident,
    /// Default actor for operations.
    pub default_actor: Ident,
    /// Operations.
    pub ops: Vec<OpDecl>,
    /// Source span.
    pub span: Span,
}

/// Operation kind (command or query).
#[derive(Debug, Clone, PartialEq)]
pub enum OpKind {
    /// Command (mutating operation).
    Cmd,
    /// Query (read-only operation).
    Qry,
}

/// Operation declaration in an interface.
#[derive(Debug, Clone, PartialEq)]
pub struct OpDecl {
    /// Operation kind.
    pub kind: OpKind,
    /// Operation name.
    pub name: Ident,
    /// Parameters.
    pub params: Vec<Field>,
    /// Output mode.
    pub output: ModeExpr,
    /// Possible error names.
    pub errors: Vec<Ident>,
    /// Source span.
    pub span: Span,
}

/// State declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct StateDecl {
    /// State name.
    pub name: Ident,
    /// Attributes pack.
    pub attrs: Pack,
    /// Source span.
    pub span: Span,
}

/// Flow declaration (transition with conditions).
#[derive(Debug, Clone, PartialEq)]
pub struct FlowDecl {
    /// Flow name.
    pub name: Ident,
    /// Source state.
    pub from: Ident,
    /// Target state.
    pub to: Ident,
    /// Flow kind/operation type.
    pub kind: Ident,
    /// Attributes pack.
    pub attrs: Pack,
    /// Source span.
    pub span: Span,
}

/// Behavior declaration (operation implementation contract).
#[derive(Debug, Clone, PartialEq)]
pub struct BehaviorDecl {
    /// Behavior name.
    pub name: Ident,
    /// Interface name.
    pub on_interface: Ident,
    /// Operation name.
    pub on_op: Ident,
    /// Bound variable names.
    pub binders: Vec<Ident>,
    /// Attributes pack (reads, writes, etc.).
    pub attrs: Pack,
    /// Behavior clauses (requires, ensures, etc.).
    pub clauses: Vec<Clause>,
    /// Source span.
    pub span: Span,
}

/// A clause in a behavior declaration.
#[derive(Debug, Clone, PartialEq)]
pub enum Clause {
    /// Precondition.
    Requires(Pred),
    /// Postcondition.
    Ensures(Pred),
    /// Return value specification.
    Returns(Expr),
    /// Failure mode specification.
    Fails {
        /// Error name.
        error: Ident,
        /// Condition under which error occurs.
        when: Pred,
        /// State to preserve.
        preserves: Option<Ident>,
    },
    /// Event emission.
    Emits {
        /// Event name.
        event: Ident,
        /// Qualifier identifiers.
        qualifier: Vec<Ident>,
    },
}

/// Invariant declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct InvariantDecl {
    /// Invariant name.
    pub name: Ident,
    /// Scope (state, interface, or component).
    pub scope: Ident,
    /// The invariant predicate.
    pub always: Pred,
    /// Source span.
    pub span: Span,
}

/// Constraint declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstraintDecl {
    /// Constraint name.
    pub name: Ident,
    /// Constraint class (e.g., workload).
    pub class: Ident,
    /// Scope.
    pub scope: Ident,
    /// Conditions pack.
    pub under: Pack,
    /// The constraint predicate.
    pub must: Pred,
    /// Source span.
    pub span: Span,
}

/// Synthesis declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct SynthesisDecl {
    /// Synthesis name.
    pub name: Ident,
    /// Target language.
    pub target_lang: Ident,
    /// Target framework (optional).
    pub target_framework: Option<Ident>,
    /// Attributes pack.
    pub attrs: Pack,
    /// Source span.
    pub span: Span,
}

/// Acceptance test declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct AcceptanceDecl {
    /// Acceptance test name.
    pub name: Ident,
    /// Subject of test.
    pub subject: Ident,
    /// Test blocks.
    pub blocks: Vec<AcceptanceBlock>,
    /// Source span.
    pub span: Span,
}

/// A block within an acceptance test.
#[derive(Debug, Clone, PartialEq)]
pub enum AcceptanceBlock {
    /// Property-based test.
    Property {
        /// Property name.
        name: Ident,
        /// Property body.
        body: Pack,
    },
    /// Scenario test.
    Scenario {
        /// Scenario name.
        name: Ident,
        /// Scenario steps.
        steps: Pack,
    },
    /// Concurrency test.
    Concurrency {
        /// Test name.
        name: Ident,
        /// Test attributes.
        attrs: Pack,
        /// Concurrency constraint.
        must: Pred,
    },
    /// Fault injection test.
    Fault {
        /// Test name.
        name: Ident,
        /// Test body.
        body: Pack,
        /// Fault assertion (`must <pred>`), the obligation's teeth.
        must: Option<Pred>,
    },
    /// Coverage specification.
    Coverage(Vec<Ident>),
    /// Execution configuration.
    Execution(Pack),
}

// ---- generic attribute packs ----

/// A pack is a list of key-value items.
pub type Pack = Vec<PackItem>;

/// A single item in a pack.
#[derive(Debug, Clone, PartialEq)]
pub struct PackItem {
    /// Key name.
    pub key: Ident,
    /// Value.
    pub value: PackValue,
    /// Source span.
    pub span: Span,
}

/// A value in a pack item.
#[derive(Debug, Clone, PartialEq)]
pub enum PackValue {
    /// No value (bare key).
    Unit,
    /// Single word (identifier).
    Word(Ident),
    /// Integer literal.
    Int(i64),
    /// String literal.
    Str(String),
    /// Quantity with unit (e.g., 30 min, 300 ms).
    Quantity {
        /// Numeric value.
        value: i64,
        /// Unit name.
        unit: Ident,
    },
    /// List of values.
    List(Vec<PackValue>),
    /// Path with dot separators.
    Path(Vec<Ident>),
    /// Function call.
    Call {
        /// Function name.
        name: Ident,
        /// Arguments.
        args: Vec<PackValue>,
    },
    /// Nested pack.
    Nested(Pack),
}

// ---- predicates and expressions ----

/// A predicate (boolean formula).
#[derive(Debug, Clone, PartialEq)]
pub enum Pred {
    /// Logical AND.
    And(Box<Pred>, Box<Pred>),
    /// Logical OR.
    Or(Box<Pred>, Box<Pred>),
    /// Logical NOT.
    Not(Box<Pred>),
    /// Comparison operation.
    Cmp {
        /// Comparison operator.
        op: CmpOp,
        /// Left-hand side expression.
        lhs: Expr,
        /// Right-hand side expression.
        rhs: Expr,
    },
    /// Universal quantification.
    ForAll {
        /// Mode of quantified variable.
        mode: Ident,
        /// Variable name.
        var: Ident,
        /// Body predicate.
        body: Box<Pred>,
    },
    /// Existential quantification.
    Exists {
        /// Mode of quantified variable.
        mode: Ident,
        /// Variable name.
        var: Ident,
        /// Body predicate.
        body: Box<Pred>,
    },
    /// Predicate call.
    Call {
        /// Predicate name.
        name: Ident,
        /// Arguments.
        args: Vec<Expr>,
    },
    /// Bare predicate name (abstract predicate).
    Word(Ident),
}

/// Comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    /// Equality.
    Eq,
    /// Less than.
    Lt,
    /// Less than or equal.
    Le,
}

/// An expression (can appear in predicates).
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Integer literal.
    Int(i64),
    /// String literal.
    Str(String),
    /// Path access (e.g., result, request.list).
    Path(Vec<Ident>),
    /// Function call.
    Call {
        /// Function name.
        name: Ident,
        /// Arguments.
        args: Vec<Expr>,
    },
}
