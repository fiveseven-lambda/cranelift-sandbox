use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Module};
use std::sync::{LazyLock, Mutex};

static JIT_MODULE: LazyLock<Mutex<JITModule>> = LazyLock::new(|| {
    let jit_builder = JITBuilder::new(cranelift_module::default_libcall_names()).unwrap();
    Mutex::new(JITModule::new(jit_builder))
});

static FUNCTION_BUILDER_CONTEXT: LazyLock<Mutex<FunctionBuilderContext>> =
    LazyLock::new(|| Mutex::new(FunctionBuilderContext::new()));

#[derive(Debug)]
enum Expr {
    Param(usize),
    Int(i64),
    Exprs(Vec<Expr>),
    Tys(Vec<Ty>),
    Ty(Ty),
    Add(Box<Expr>, Box<Expr>),
    App(Func, Vec<Expr>),
}

#[derive(Debug)]
enum Func {
    Builtin(i64, Vec<Ty>, Ty),
    Defined(FuncId),
    Compile(Box<Expr>, Vec<Ty>, Ty),
}

#[derive(Debug, Clone, Copy)]
enum Ty {
    Int,
    Ptr,
}

impl Ty {
    fn translate(&self) -> Type {
        match self {
            Ty::Int => types::I64,
            Ty::Ptr => JIT_MODULE.lock().unwrap().isa().pointer_type(),
        }
    }
}

fn compile(func_id: FuncId, exprs: Vec<Expr>, args_ty: &[Ty], ret_ty: &Ty) -> *const u8 {
    let module = JIT_MODULE.lock().unwrap();
    let mut ctx = module.make_context();
    ctx.func.signature = module.make_signature();
    drop(module);
    for arg_ty in args_ty {
        ctx.func
            .signature
            .params
            .push(AbiParam::new(arg_ty.translate()));
    }
    ctx.func
        .signature
        .returns
        .push(AbiParam::new(ret_ty.translate()));
    {
        let mut builder_ctx = FUNCTION_BUILDER_CONTEXT.lock().unwrap();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let args = builder.block_params(entry).to_vec();
        let ret = exprs
            .into_iter()
            .map(|expr| expr.translate(&mut builder, &args))
            .last()
            .unwrap();
        builder.ins().return_(&[ret]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    let mut module = JIT_MODULE.lock().unwrap();
    module.define_function(func_id, &mut ctx).unwrap();
    module.finalize_definitions().unwrap();
    module.get_finalized_function(func_id)
}

unsafe extern "C" fn compile_expr(
    expr: *mut Expr,
    args_ty: *mut Vec<Ty>,
    ret_ty: *mut Ty,
) -> *const u8 {
    let module = JIT_MODULE.lock().unwrap();
    let mut ctx = module.make_context();
    ctx.func.signature = module.make_signature();
    drop(module);
    let args_ty = *unsafe { Box::from_raw(args_ty) };
    let ret_ty = *unsafe { Box::from_raw(ret_ty) };
    for arg_ty in args_ty {
        ctx.func
            .signature
            .params
            .push(AbiParam::new(arg_ty.translate()));
    }
    ctx.func
        .signature
        .returns
        .push(AbiParam::new(ret_ty.translate()));
    {
        let mut builder_ctx = FUNCTION_BUILDER_CONTEXT.lock().unwrap();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let args = builder.block_params(entry).to_vec();
        let expr = *unsafe { Box::from_raw(expr) };
        let ret = expr.translate(&mut builder, &args);
        builder.ins().return_(&[ret]);
    }
    let mut module = JIT_MODULE.lock().unwrap();
    let func_id = module
        .declare_anonymous_function(&ctx.func.signature)
        .unwrap();
    module.define_function(func_id, &mut ctx).unwrap();
    module.finalize_definitions().unwrap();
    module.get_finalized_function(func_id)
}

impl Expr {
    fn translate(self, builder: &mut FunctionBuilder, v_args: &[Value]) -> Value {
        match self {
            Expr::Param(index) => v_args[index],
            Expr::Int(value) => builder.ins().iconst(types::I64, value),
            Expr::App(Func::Builtin(ptr, args_ty, ret_ty), args) => {
                let mut sig = Signature::new(builder.func.signature.call_conv);
                for arg_ty in args_ty {
                    sig.params.push(AbiParam::new(arg_ty.translate()));
                }
                sig.returns.push(AbiParam::new(ret_ty.translate()));
                let sig = builder.import_signature(sig);
                let func = builder.ins().iconst(types::I64, ptr);
                let args: Vec<_> = args
                    .into_iter()
                    .map(|arg| arg.translate(builder, v_args))
                    .collect();
                let inst = builder.ins().call_indirect(sig, func, &args);
                builder.inst_results(inst)[0]
            }
            Expr::App(Func::Compile(expr, args_ty, ret_ty), args) => {
                let mut sig = Signature::new(builder.func.signature.call_conv);
                for _ in 0..3 {
                    sig.params.push(AbiParam::new(types::I64));
                }
                sig.returns.push(AbiParam::new(types::I64));
                let sig = builder.import_signature(sig);

                let mut sig2 = Signature::new(builder.func.signature.call_conv);
                for arg_ty in &args_ty {
                    sig2.params.push(AbiParam::new(arg_ty.translate()));
                }
                sig2.returns.push(AbiParam::new(ret_ty.translate()));
                let sig2 = builder.import_signature(sig2);

                let v_compile_expr = builder.ins().iconst(types::I64, compile_expr as i64);
                let v_expr = expr.translate(builder, v_args);
                let v_args_ty = builder
                    .ins()
                    .iconst(types::I64, Box::into_raw(Box::new(args_ty)) as i64);
                let v_ret_ty = builder
                    .ins()
                    .iconst(types::I64, Box::into_raw(Box::new(ret_ty)) as i64);
                let inst = builder.ins().call_indirect(
                    sig,
                    v_compile_expr,
                    &[v_expr, v_args_ty, v_ret_ty],
                );
                let v_func = builder.inst_results(inst)[0];

                let args: Vec<_> = args
                    .into_iter()
                    .map(|arg| arg.translate(builder, v_args))
                    .collect();
                let inst = builder.ins().call_indirect(sig2, v_func, &args);
                builder.inst_results(inst)[0]
            }
            Expr::Ty(ty) => builder
                .ins()
                .iconst(types::I64, Box::into_raw(Box::new(ty)) as i64),
            Expr::Exprs(exprs) => {
                let v_new_vec_expr = builder.ins().iconst(types::I64, new_vec_expr as i64);
                let mut new_vec_expr_sig = Signature::new(builder.func.signature.call_conv);
                new_vec_expr_sig
                    .returns
                    .push(AbiParam::new(Ty::Ptr.translate()));
                let new_vec_expr_sig = builder.import_signature(new_vec_expr_sig);

                let v_push_vec_expr = builder.ins().iconst(types::I64, push_vec_expr as i64);
                let mut push_vec_expr_sig = Signature::new(builder.func.signature.call_conv);
                push_vec_expr_sig
                    .params
                    .push(AbiParam::new(Ty::Ptr.translate()));
                push_vec_expr_sig
                    .params
                    .push(AbiParam::new(Ty::Ptr.translate()));
                let push_vec_expr_sig = builder.import_signature(push_vec_expr_sig);

                let inst = builder
                    .ins()
                    .call_indirect(new_vec_expr_sig, v_new_vec_expr, &[]);
                let v_vec = builder.inst_results(inst)[0];
                for expr in exprs {
                    let v_expr = expr.translate(builder, v_args);
                    builder.ins().call_indirect(
                        push_vec_expr_sig,
                        v_push_vec_expr,
                        &[v_vec, v_expr],
                    );
                }
                v_vec
            }
            Expr::Tys(tys) => builder
                .ins()
                .iconst(types::I64, Box::into_raw(Box::new(tys)) as i64),
            Expr::Add(left, right) => {
                let left = left.translate(builder, v_args);
                let right = right.translate(builder, v_args);
                builder.ins().iadd(left, right)
            }
            _ => todo!(),
        }
    }
}

fn const_expr(expr: Expr) -> Expr {
    match expr {
        Expr::Int(value) => Expr::App(
            Func::Builtin(make_int as i64, vec![Ty::Int], Ty::Ptr),
            vec![Expr::Int(value)],
        ),
        Expr::Tys(value) => Expr::App(
            Func::Builtin(make_tys as i64, vec![Ty::Ptr], Ty::Ptr),
            vec![Expr::Tys(value)],
        ),
        Expr::Ty(value) => Expr::App(
            Func::Builtin(make_ty as i64, vec![Ty::Ptr], Ty::Ptr),
            vec![Expr::Ty(value)],
        ),
        Expr::Exprs(exprs) => Expr::App(
            Func::Builtin(make_exprs as i64, vec![Ty::Ptr], Ty::Ptr),
            vec![Expr::Exprs(exprs.into_iter().map(const_expr).collect())],
        ),
        Expr::App(func, args) => Expr::App(
            Func::Builtin(make_app as i64, vec![Ty::Ptr, Ty::Ptr], Ty::Ptr),
            vec![
                const_func(func),
                Expr::Exprs(args.into_iter().map(const_expr).collect()),
            ],
        ),
        Expr::Param(index) => Expr::App(
            Func::Builtin(make_param as i64, vec![Ty::Int], Ty::Ptr),
            vec![Expr::Int(index as i64)],
        ),
        Expr::Add(left, right) => Expr::App(
            Func::Builtin(make_add as i64, vec![Ty::Ptr, Ty::Ptr], Ty::Ptr),
            vec![const_expr(*left), const_expr(*right)],
        ),
    }
}

fn const_func(func: Func) -> Expr {
    match func {
        Func::Builtin(ptr, args_ty, ret_ty) => Expr::App(
            Func::Builtin(
                make_builtin as i64,
                vec![Ty::Int, Ty::Ptr, Ty::Ptr],
                Ty::Ptr,
            ),
            vec![Expr::Int(ptr), Expr::Tys(args_ty), Expr::Ty(ret_ty)],
        ),
        _ => todo!(),
    }
}

extern "C" fn make_int(value: i64) -> *mut Expr {
    Box::into_raw(Box::new(Expr::Int(value)))
}

unsafe extern "C" fn make_app(func: *mut Func, args: *mut Vec<Expr>) -> *mut Expr {
    let func = *unsafe { Box::from_raw(func) };
    let args = *unsafe { Box::from_raw(args) };
    Box::into_raw(Box::new(Expr::App(func, args)))
}

extern "C" fn debug_integer(value: i64) -> i64 {
    dbg!(value)
}

unsafe extern "C" fn make_exprs(exprs: *mut Vec<Expr>) -> *mut Expr {
    let exprs = *unsafe { Box::from_raw(exprs) };
    Box::into_raw(Box::new(Expr::Exprs(exprs)))
}

unsafe extern "C" fn make_tys(tys: *mut Vec<Ty>) -> *mut Expr {
    let tys = *unsafe { Box::from_raw(tys) };
    Box::into_raw(Box::new(Expr::Tys(tys)))
}

unsafe extern "C" fn make_ty(ty: *mut Ty) -> *mut Expr {
    let ty = *unsafe { Box::from_raw(ty) };
    Box::into_raw(Box::new(Expr::Ty(ty)))
}

extern "C" fn new_vec_expr() -> *mut Vec<Expr> {
    Box::into_raw(Box::new(Vec::new()))
}

unsafe extern "C" fn push_vec_expr(vec: *mut Vec<Expr>, expr: *mut Expr) {
    unsafe { (*vec).push(*Box::from_raw(expr)) };
}

unsafe extern "C" fn make_builtin(ptr: i64, args_ty: *mut Vec<Ty>, ret_ty: *mut Ty) -> *mut Func {
    let args_ty = *unsafe { Box::from_raw(args_ty) };
    let ret_ty = *unsafe { Box::from_raw(ret_ty) };
    Box::into_raw(Box::new(Func::Builtin(ptr, args_ty, ret_ty)))
}

extern "C" fn make_param(index: usize) -> *mut Expr {
    Box::into_raw(Box::new(Expr::Param(index)))
}

extern "C" fn make_add(left: *mut Expr, right: *mut Expr) -> *mut Expr {
    let left = unsafe { Box::from_raw(left) };
    let right = unsafe { Box::from_raw(right) };
    Box::into_raw(Box::new(Expr::Add(left, right)))
}

fn declare_anonymous_function() -> FuncId {
    let mut module = JIT_MODULE.lock().unwrap();
    let signature = module.make_signature();
    module.declare_anonymous_function(&signature).unwrap()
}

fn main() {
    println!("make_builtin: {0} = 0x{0:x}", make_builtin as usize);
    println!("make_int: {0} = 0x{0:x}", make_int as usize);
    println!("make_app: {0} = 0x{0:x}", make_app as usize);
    println!("new_vec_expr: {0} = 0x{0:x}", new_vec_expr as usize);
    println!("push_vec_expr: {0} = 0x{0:x}", push_vec_expr as usize);

    test1();
    test2();
    test3();
}

fn test1() {
    let mut expr = Expr::Int(42);

    let n = 6;

    for _ in 0..n {
        expr = const_expr(expr);
    }

    println!("{expr:?}");

    for _ in 0..n {
        let ptr = compile(declare_anonymous_function(), vec![expr], &[], &Ty::Ptr);
        let func: unsafe fn() -> *mut Expr = unsafe { std::mem::transmute(ptr) };
        expr = *unsafe { Box::from_raw(func()) };
    }

    let ptr = compile(declare_anonymous_function(), vec![expr], &[], &Ty::Int);
    let func: unsafe fn() -> i64 = unsafe { std::mem::transmute(ptr) };
    dbg!(unsafe { func() });
}

fn test2() {
    let expr = Expr::App(
        Func::Builtin(make_add as i64, vec![Ty::Ptr, Ty::Ptr], Ty::Ptr),
        vec![
            const_expr(Expr::Param(0)),
            Expr::App(
                Func::Builtin(make_int as i64, vec![Ty::Int], Ty::Ptr),
                vec![Expr::Param(0)],
            ),
        ],
    );

    println!("{expr:?}");

    let ptr = compile(
        declare_anonymous_function(),
        vec![expr],
        &[Ty::Int],
        &Ty::Ptr,
    );
    let func: unsafe fn(i64) -> *mut Expr = unsafe { std::mem::transmute(ptr) };
    let expr = unsafe { *Box::from_raw(func(10)) };
    let ptr = compile(
        declare_anonymous_function(),
        vec![expr],
        &[Ty::Int],
        &Ty::Int,
    );
    let func: unsafe fn(i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    dbg!(unsafe { func(20) });
}

fn test3() {
    let expr = Expr::App(
        Func::Compile(
            Box::new(const_expr(Expr::App(
                Func::Builtin(debug_integer as i64, vec![Ty::Int], Ty::Int),
                vec![Expr::Add(Box::new(Expr::Param(0)), Box::new(Expr::Int(2)))],
            ))),
            vec![Ty::Int],
            Ty::Int,
        ),
        vec![Expr::Int(40)],
    );
    println!("{expr:?}");
    let ptr = compile(declare_anonymous_function(), vec![expr], &[], &Ty::Int);
    let func: unsafe fn() -> i64 = unsafe { std::mem::transmute(ptr) };
    unsafe { func() };
}
