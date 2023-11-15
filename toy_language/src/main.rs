mod ast;
mod parser;
mod pre_ast;

use std::io::Read;

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    let stmts = parser::parse(&input);
    for stmt in &stmts {
        stmt._debug_print(0);
    }
}
