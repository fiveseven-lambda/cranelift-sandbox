mod chars_peekable;
mod operators;
mod token;

use crate::pre_ast::{Stmt, Term};
use chars_peekable::CharsPeekable;
use enum_iterator::Sequence;
use token::Token;

pub fn parse(input: &str) -> Vec<Stmt> {
    let mut chars = CharsPeekable::new(input);
    let mut peeked = token::next(&mut chars);
    let ret = std::iter::from_fn(|| parse_stmt(&mut chars, &mut peeked)).collect();
    assert!(peeked.is_none());
    ret
}

fn parse_stmt(chars: &mut CharsPeekable, peeked: &mut Option<Token>) -> Option<Stmt> {
    let term = parse_term(chars, peeked);
    match peeked {
        Some(Token::Semicolon) => {
            *peeked = token::next(chars);
            Some(Stmt::Term(term))
        }
        Some(Token::OpeningBrace) => {
            *peeked = token::next(chars);
            let mut stmts = Vec::new();
            loop {
                if let Some(Token::ClosingBrace) = peeked {
                    *peeked = token::next(chars);
                    break;
                } else if let Some(stmt) = parse_stmt(chars, peeked) {
                    stmts.push(stmt);
                } else {
                    panic!();
                }
            }
            Some(Stmt::Block {
                antecedent: term,
                stmts,
            })
        }
        Some(_) => panic!(),
        None => None,
    }
}
fn parse_term(chars: &mut CharsPeekable, peeked: &mut Option<Token>) -> Option<Term> {
    parse_assign(chars, peeked)
}
fn parse_assign(chars: &mut CharsPeekable, peeked: &mut Option<Token>) -> Option<Term> {
    let left_hand_side = parse_binary_operation(chars, peeked);
    if let Some(operator) = peeked.as_ref().and_then(operators::assignment) {
        *peeked = token::next(chars);
        let right_hand_side = parse_assign(chars, peeked);
        Some(Term::Assignment {
            left_hand_side: left_hand_side.map(Box::new),
            operator,
            right_hand_side: right_hand_side.map(Box::new),
        })
    } else {
        left_hand_side
    }
}
fn parse_binary_operation(chars: &mut CharsPeekable, peeked: &mut Option<Token>) -> Option<Term> {
    parse_binary_operation_rec(chars, peeked, operators::Precedence::first())
}
fn parse_binary_operation_rec(
    chars: &mut CharsPeekable,
    peeked: &mut Option<Token>,
    precedence: Option<operators::Precedence>,
) -> Option<Term> {
    let Some(precedence) = precedence else {
            return parse_factor(chars, peeked);
        };
    let mut left_operand = parse_binary_operation_rec(chars, peeked, precedence.next());
    while let Some(operator) = peeked
        .as_ref()
        .and_then(|token| operators::infix(token, precedence))
    {
        *peeked = token::next(chars);
        let right_operand = parse_binary_operation_rec(chars, peeked, precedence.next());
        left_operand = Some(Term::BinaryOperation {
            left_operand: left_operand.map(Box::new),
            operator,
            right_operand: right_operand.map(Box::new),
        });
    }
    left_operand
}
fn parse_factor(chars: &mut CharsPeekable, peeked: &mut Option<Token>) -> Option<Term> {
    let Some(first_token) = peeked else {
            return None;
        };
    let mut antecedent = match first_token {
        Token::Identifier(name) => {
            let ret = Term::Identifier(name.clone());
            *peeked = token::next(chars);
            Some(ret)
        }
        Token::Integer(value) => {
            let ret = Term::Integer(*value);
            *peeked = token::next(chars);
            Some(ret)
        }
        Token::StringLiteral(components) => {
            let ret = Term::StringLiteral(components.clone());
            *peeked = token::next(chars);
            Some(ret)
        }
        _ => None,
    };
    loop {
        match *peeked {
            Some(Token::OpeningBracket(bracket_kind)) => {
                *peeked = token::next(chars);
                let mut elements = Vec::new();
                let has_trailing_comma;
                loop {
                    let element = parse_assign(chars, peeked);
                    if let Some(Token::Comma) = peeked {
                        *peeked = token::next(chars);
                        elements.push(element);
                    } else {
                        if let Some(element) = element {
                            has_trailing_comma = false;
                            elements.push(Some(element));
                        } else {
                            has_trailing_comma = true;
                        }
                        break;
                    }
                }
                assert!(matches!(*peeked,
                    Some(Token::ClosingBracket(closing_bracket_kind)) if closing_bracket_kind == bracket_kind
                ));
                *peeked = token::next(chars);
                antecedent = Some(Term::Bracketed {
                    antecedent: antecedent.map(Box::new),
                    bracket_kind,
                    elements,
                    has_trailing_comma,
                });
            }
            Some(Token::Colon) => {
                *peeked = token::next(chars);
                let ty = parse_factor(chars, peeked);
                antecedent = Some(Term::Declaration {
                    term: antecedent.map(Box::new),
                    ty: ty.map(Box::new),
                })
            }
            _ => return antecedent,
        }
    }
}
