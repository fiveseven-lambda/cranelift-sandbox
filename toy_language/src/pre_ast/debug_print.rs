use super::{Stmt, StringLiteralComponent, Term};

impl Term {
    pub fn _debug_print(&self, depth: usize) {
        let indent = "  ".repeat(depth);
        match self {
            Term::Identifier(name) => {
                println!("{indent}Identifier({name})");
            }
            Term::Integer(value) => {
                println!("{indent}Integer({value})");
            }
            Term::StringLiteral(components) => {
                println!("{indent}String literal");
                for component in components {
                    component._debug_print(depth + 1);
                }
            }
            Term::Declaration { term, ty } => {
                println!("{indent}Declaration");
                if let Some(term) = term {
                    term._debug_print(depth + 1);
                }
                if let Some(term) = ty {
                    term._debug_print(depth + 1);
                }
            }
            Term::Assignment {
                left_hand_side,
                operator,
                right_hand_side,
            } => {
                println!("{indent}Assignment({operator:?})");
                if let Some(term) = left_hand_side {
                    term._debug_print(depth + 1);
                }
                if let Some(term) = right_hand_side {
                    term._debug_print(depth + 1);
                }
            }
            Term::BinaryOperation {
                left_operand,
                operator,
                right_operand,
            } => {
                println!("{indent}Binary operation({operator:?})");
                if let Some(term) = left_operand {
                    term._debug_print(depth + 1);
                }
                if let Some(term) = right_operand {
                    term._debug_print(depth + 1);
                }
            }
            Term::Bracketed {
                antecedent,
                bracket_kind,
                elements,
                has_trailing_comma,
            } => {
                eprintln!("{indent}Bracketed({bracket_kind:?})");
                match antecedent {
                    Some(term) => term._debug_print(depth + 1),
                    None => eprintln!("{indent}  (no antecedent)"),
                }
                eprintln!(
                    "{indent}{} elements (trailing comma: {has_trailing_comma})",
                    elements.len()
                );
                for elem in elements {
                    match elem {
                        Some(term) => term._debug_print(depth + 1),
                        None => {
                            eprintln!("{indent}  (empty)")
                        }
                    }
                }
            }
        }
    }
}

impl StringLiteralComponent {
    pub fn _debug_print(&self, depth: usize) {
        let indent = "  ".repeat(depth);
        match self {
            StringLiteralComponent::Expr(expr) => {
                expr._debug_print(depth);
            }
            StringLiteralComponent::String(string) => {
                println!("{indent}{string}");
            }
        }
    }
}

impl Stmt {
    pub fn _debug_print(&self, depth: usize) {
        let indent = "  ".repeat(depth);
        match self {
            Stmt::Term(term) => {
                println!("{indent}Expression statement");
                if let Some(term) = term {
                    term._debug_print(depth + 1);
                }
            }
            Stmt::Block { antecedent, stmts } => {
                println!("{indent}Block");
                if let Some(term) = antecedent {
                    term._debug_print(depth + 1);
                }
                for stmt in stmts {
                    stmt._debug_print(depth + 1);
                }
            }
        }
    }
}
