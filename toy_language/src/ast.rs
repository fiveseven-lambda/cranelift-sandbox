pub enum Expr {
    Variable(usize),
    Global(usize),
    Func(usize),
    Integer(i64),
    Float(f64),
    String(String),
    Call(Box<Expr>, Vec<Expr>),
}
