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
    Int(i64),
    DefVar(usize, Box<Expr>),
    UseVar(usize),
    Exprs(Vec<Expr>),
    Tys(Vec<Ty>),
    Ty(Ty),
    App(Func, Vec<Expr>),
}

#[derive(Debug)]
enum Func {
    Builtin(i64, Vec<Ty>, Ty),
    Defined(FuncId),
    Expr(Box<Expr>, Vec<Ty>, Ty),
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
        let ret = exprs
            .into_iter()
            .map(|expr| expr.translate(&mut builder))
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
    num_args: usize,
    args_ty: *const Ty,
    ret_ty: &Ty,
) -> *const u8 {
    let module = JIT_MODULE.lock().unwrap();
    let mut ctx = module.make_context();
    ctx.func.signature = module.make_signature();
    drop(module);
    let args_ty = unsafe { std::slice::from_raw_parts(args_ty, num_args) };
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
        let expr = *unsafe { Box::from_raw(expr) };
        let ret = expr.translate(&mut builder);
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
    fn translate(self, builder: &mut FunctionBuilder) -> Value {
        match self {
            Expr::Int(value) => builder.ins().iconst(types::I64, value),
            Expr::App(Func::Builtin(ptr, args_ty, ret_ty), args) => {
                let mut sig = Signature::new(builder.func.signature.call_conv);
                for arg_ty in args_ty {
                    sig.params.push(AbiParam::new(arg_ty.translate()));
                }
                sig.returns.push(AbiParam::new(ret_ty.translate()));
                let sig = builder.import_signature(sig);
                let func = builder.ins().iconst(types::I64, ptr);
                let args: Vec<_> = args.into_iter().map(|arg| arg.translate(builder)).collect();
                let inst = builder.ins().call_indirect(sig, func, &args);
                builder.inst_results(inst)[0]
            }
            Expr::App(Func::Expr(expr, args_ty, ret_ty), args) => {
                let mut sig = Signature::new(builder.func.signature.call_conv);
                sig.params.resize(2, AbiParam::new(types::I64));
                let sig = builder.import_signature(sig);
                let compile_expr_ptr = builder.ins().iconst(types::I64, compile_expr as i64);
                let v_expr = expr.translate(builder);
                let v_num_args = builder.ins().iconst(types::I64, args.len() as i64);
                let v_args_ty = builder.ins().iconst(types::I64, args_ty.as_ptr() as i64);
                let v_ret_ty = builder
                    .ins()
                    .iconst(types::I64, &ret_ty as *const Ty as i64);
                let inst = builder.ins().call_indirect(
                    sig,
                    compile_expr_ptr,
                    &[v_expr, v_num_args, v_args_ty, v_ret_ty],
                );
                let func = builder.inst_results(inst)[0];
                let mut sig = Signature::new(builder.func.signature.call_conv);
                for arg_ty in args_ty {
                    sig.params.push(AbiParam::new(arg_ty.translate()));
                }
                sig.returns.push(AbiParam::new(ret_ty.translate()));
                let sig = builder.import_signature(sig);
                let args: Vec<_> = args.into_iter().map(|arg| arg.translate(builder)).collect();
                let inst = builder.ins().call_indirect(sig, func, &args);
                builder.inst_results(inst)[0]
            }
            Expr::Ty(ty) => builder
                .ins()
                .iconst(types::I64, Box::into_raw(Box::new(ty)) as i64),
            Expr::Exprs(exprs) => builder
                .ins()
                .iconst(types::I64, Box::into_raw(Box::new(exprs)) as i64),
            Expr::Tys(tys) => builder
                .ins()
                .iconst(types::I64, Box::into_raw(Box::new(tys)) as i64),
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
        Expr::App(func, args) => Expr::App(
            Func::Builtin(make_app as i64, vec![Ty::Ptr, Ty::Ptr], Ty::Ptr),
            vec![
                const_func(func),
                Expr::Exprs(args.into_iter().map(const_expr).collect()),
            ],
        ),
        _ => todo!(),
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

extern "C" fn print_integer(value: i64) -> i64 {
    println!("{}", value);
    value
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

fn main() {
    println!("make_builtin: {}", make_builtin as usize);
    println!("make_int: {}", make_int as usize);
    println!("make_app: {}", make_app as usize);
    let func_id = {
        let mut module = JIT_MODULE.lock().unwrap();
        let signature = module.make_signature();
        module.declare_anonymous_function(&signature).unwrap()
    };
    let expr = const_expr(const_expr(Expr::Int(42)));
    println!("{expr:?}");
    let ptr = compile(func_id, vec![expr], &[], &Ty::Ptr);
    let func: unsafe fn() -> *mut Expr = unsafe { std::mem::transmute(ptr) };
    let expr = *unsafe { Box::from_raw(func()) };
    println!("{expr:?}");
    let func_id = {
        let mut module = JIT_MODULE.lock().unwrap();
        let signature = module.make_signature();
        module.declare_anonymous_function(&signature).unwrap()
    };
    let ptr = compile(func_id, vec![expr], &[], &Ty::Int);
    let func: unsafe fn() -> *mut Expr = unsafe { std::mem::transmute(ptr) };
    let expr = *unsafe { Box::from_raw(func()) };
    println!("{expr:?}");
    let func_id = {
        let mut module = JIT_MODULE.lock().unwrap();
        let signature = module.make_signature();
        module.declare_anonymous_function(&signature).unwrap()
    };
    let ptr = compile(func_id, vec![expr], &[], &Ty::Int);
    let func: unsafe fn() -> i64 = unsafe { std::mem::transmute(ptr) };
    dbg!(unsafe { func() });
}
