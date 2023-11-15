mod debug_print;

#[derive(Clone, Debug)]
pub enum Term {
    Identifier(String),
    Integer(i64),
    StringLiteral(Vec<StringLiteralComponent>),
    Declaration {
        term: Option<Box<Term>>,
        ty: Option<Box<Term>>,
    },
    BinaryOperation {
        left_operand: Option<Box<Term>>,
        operator: Operator,
        right_operand: Option<Box<Term>>,
    },
    Assignment {
        left_hand_side: Option<Box<Term>>,
        operator: Operator,
        right_hand_side: Option<Box<Term>>,
    },
    Bracketed {
        antecedent: Option<Box<Term>>,
        bracket_kind: BracketKind,
        elements: Vec<Option<Term>>,
        has_trailing_comma: bool,
    },
}
#[derive(Clone, Debug)]
pub enum StringLiteralComponent {
    String(String),
    Expr(Term),
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BracketKind {
    Round,
    Square,
}

#[derive(Clone, Debug)]
pub enum Operator {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Assign,
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Term(Option<Term>),
    Block {
        antecedent: Option<Term>,
        stmts: Vec<Stmt>,
    },
}
