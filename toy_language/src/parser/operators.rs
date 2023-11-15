use super::Token;
use crate::pre_ast::Operator;
use enum_iterator::Sequence;

#[derive(Clone, Copy, Sequence)]
pub enum Precedence {
    Equality,
    Inequality,
    AddSub,
    MulDivRem,
}
pub fn infix(token: &Token, precedence: Precedence) -> Option<Operator> {
    match (token, precedence) {
        (Token::Plus, Precedence::AddSub) => Some(Operator::Add),
        (Token::Hyphen, Precedence::AddSub) => Some(Operator::Sub),
        (Token::Asterisk, Precedence::MulDivRem) => Some(Operator::Mul),
        (Token::Slash, Precedence::MulDivRem) => Some(Operator::Div),
        (Token::Percent, Precedence::MulDivRem) => Some(Operator::Rem),
        (Token::DoubleEqual, Precedence::Equality) => Some(Operator::Equal),
        (Token::ExclamationEqual, Precedence::Equality) => Some(Operator::NotEqual),
        (Token::Greater, Precedence::Inequality) => Some(Operator::Greater),
        (Token::GreaterEqual, Precedence::Inequality) => Some(Operator::GreaterEqual),
        (Token::Less, Precedence::Inequality) => Some(Operator::Less),
        (Token::LessEqual, Precedence::Inequality) => Some(Operator::LessEqual),
        _ => None,
    }
}
pub fn assignment(token: &Token) -> Option<Operator> {
    match token {
        Token::Equal => Some(Operator::Assign),
        _ => None,
    }
}
