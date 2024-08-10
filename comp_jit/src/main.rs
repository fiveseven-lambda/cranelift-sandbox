use std::{mem::ManuallyDrop, rc::Rc};

use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Module;

#[derive(Debug, Clone, Copy)]
enum Ty {
    Int,
    Ptr,
}
impl Ty {
    fn translate(&self, ptr_ty: Type) -> Type {
        match self {
            Ty::Int => types::I64,
            Ty::Ptr => ptr_ty,
        }
    }
}
#[derive(Debug)]
enum Expr {
    Comp,
    Id,
    Int(i64),
    Func(usize, Vec<Ty>, Ty),
    Add,
    App(Rc<Expr>, Vec<Rc<Expr>>),
}

extern "C" fn new_int(value: i64) -> *const Expr {
    Rc::into_raw(Rc::new(Expr::Int(value)))
}
extern "C" fn new_add() -> *const Expr {
    Rc::into_raw(Rc::new(Expr::Add))
}
extern "C" fn new_args(capacity: &mut usize) -> *mut Rc<Expr> {
    let mut ret = ManuallyDrop::new(Vec::with_capacity(*capacity));
    *capacity = ret.capacity();
    return ret.as_mut_ptr();
}
extern "C" fn add_arg(
    arg_ptr: *const Expr,
    args_ptr: *mut Rc<Expr>,
    args_len: usize,
    args_cap: &mut usize,
) {
    let arg = unsafe { Rc::from_raw(arg_ptr) };
    let mut args = ManuallyDrop::new(unsafe { Vec::from_raw_parts(args_ptr, args_len, *args_cap) });
    args.push(arg);
    *args_cap = args.capacity();
}
extern "C" fn new_app(
    func_ptr: *const Expr,
    args_ptr: *mut Rc<Expr>,
    args_len: usize,
    args_cap: &mut usize,
) -> *const Expr {
    let func = unsafe { Rc::from_raw(func_ptr) };
    let args = unsafe { Vec::from_raw_parts(args_ptr, args_len, *args_cap) };
    Rc::into_raw(Rc::new(Expr::App(func, args)))
}

fn translate(
    expr: &Expr,
    module: &impl Module,
    builder: &mut FunctionBuilder,
    args: &[Value],
    ptr_ty: Type,
) -> Value {
    match *expr {
        Expr::Id => args[0],
        Expr::Int(value) => builder.ins().iconst(types::I64, value),
        Expr::Add => builder.ins().iadd(args[0], args[1]),
        Expr::App(ref outer, ref inner) => {
            let mids: Vec<_> = inner
                .iter()
                .map(|f| translate(f, module, builder, args, ptr_ty))
                .collect();
            translate(outer, module, builder, &mids, ptr_ty)
        }
        Expr::Func(ptr, ref args_ty, ref ret_ty) => {
            let ptr = builder.ins().iconst(ptr_ty, ptr as i64);
            let sig = {
                let mut sig = module.make_signature();
                for arg_ty in args_ty {
                    sig.params.push(AbiParam::new(arg_ty.translate(ptr_ty)));
                }
                sig.returns.push(AbiParam::new(ret_ty.translate(ptr_ty)));
                builder.import_signature(sig)
            };
            let inst = builder.ins().call_indirect(sig, ptr, args);
            builder.inst_results(inst)[0]
        }
        Expr::Comp => {
            let &[ref args @ .., func] = args else {
                panic!();
            };
            let args_len = builder.ins().iconst(ptr_ty, args.len() as i64);
            let args_cap_slot = builder.create_sized_stack_slot(StackSlotData {
                kind: StackSlotKind::ExplicitSlot,
                size: ptr_ty.bytes(),
            });
            builder.ins().stack_store(args_len, args_cap_slot, 0);
            let args_cap = builder.ins().stack_addr(ptr_ty, args_cap_slot, 0);
            let args_ptr = translate(
                &Expr::Func(new_args as usize, vec![Ty::Ptr], Ty::Ptr),
                module,
                builder,
                &[args_cap],
                ptr_ty,
            );
            {
                let add_arg = builder.ins().iconst(ptr_ty, add_arg as i64);
                let sig_add_arg = {
                    let mut sig = module.make_signature();
                    sig.params.push(AbiParam::new(ptr_ty));
                    sig.params.push(AbiParam::new(ptr_ty));
                    sig.params.push(AbiParam::new(ptr_ty));
                    sig.params.push(AbiParam::new(ptr_ty));
                    builder.import_signature(sig)
                };
                for (i, &arg) in args.iter().enumerate() {
                    let i = builder.ins().iconst(ptr_ty, i as i64);
                    builder.ins().call_indirect(
                        sig_add_arg,
                        add_arg,
                        &[arg, args_ptr, i, args_cap],
                    );
                }
            }
            translate(
                &Expr::Func(new_app as usize, vec![Ty::Ptr; 4], Ty::Ptr),
                module,
                builder,
                &[func, args_ptr, args_len, args_cap],
                ptr_ty,
            )
        }
    }
}

fn main() {
    let f1 = Expr::App(
        Rc::new(Expr::Comp),
        vec![
            Rc::new(Expr::Id),
            Rc::new(Expr::App(
                Rc::new(Expr::Func(new_int as usize, vec![Ty::Int], Ty::Ptr)),
                vec![Rc::new(Expr::Int(1))],
            )),
            Rc::new(Expr::App(
                Rc::new(Expr::Func(new_add as usize, vec![], Ty::Ptr)),
                vec![],
            )),
        ],
    );

    let module_builder = JITBuilder::new(cranelift_module::default_libcall_names()).unwrap();
    let mut module = JITModule::new(module_builder);

    let ptr_ty = module.isa().pointer_type();

    let f0 = {
        let mut ctx = module.make_context();
        ctx.func.signature = module.make_signature();
        ctx.func.signature.params.push(AbiParam::new(ptr_ty));
        ctx.func.signature.returns.push(AbiParam::new(ptr_ty));

        let mut fn_builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fn_builder_ctx);
        let block = builder.create_block();
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);
        let args = builder.block_params(block).to_owned();
        let ret = translate(&f1, &module, &mut builder, &args, ptr_ty);
        builder.ins().return_(&[ret]);
        let func = module
            .declare_anonymous_function(&ctx.func.signature)
            .unwrap();
        module.define_function(func, &mut ctx).unwrap();
        println!("{}", ctx.func.display());
        module.finalize_definitions().unwrap();
        let ptr = module.get_finalized_function(func);
        let func: unsafe fn(*const Expr) -> *const Expr = unsafe { std::mem::transmute(ptr) };
        let id = Rc::into_raw(Rc::new(Expr::Id));
        unsafe { Rc::from_raw(func(id)) }
    };
    println!("{f0:?}");

    let ret = {
        let mut ctx = module.make_context();
        ctx.func.signature = module.make_signature();
        ctx.func.signature.params.push(AbiParam::new(types::I64));
        ctx.func.signature.returns.push(AbiParam::new(types::I64));

        let mut fn_builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fn_builder_ctx);
        let block = builder.create_block();
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);
        let args = builder.block_params(block).to_owned();
        let ret = translate(&f0, &module, &mut builder, &args, ptr_ty);
        builder.ins().return_(&[ret]);
        let func = module
            .declare_anonymous_function(&ctx.func.signature)
            .unwrap();
        module.define_function(func, &mut ctx).unwrap();
        println!("{}", ctx.func.display());
        module.finalize_definitions().unwrap();
        let ptr = module.get_finalized_function(func);
        let func: unsafe fn(i64) -> i64 = unsafe { std::mem::transmute(ptr) };
        unsafe { func(10) }
    };
    println!("{ret:?}");
}

/*
extern "C" fn int(value: i64) -> *const Expr {
    Rc::into_raw(Rc::new(Expr::Int(value)))
}

struct Context {
    args: Vec<Value>,
}
impl Context {
    fn translate(&self, builder: &mut FunctionBuilder, expr: &Expr) -> Value {
        match *expr {
            Expr::Add(ref left, ref right) => {
                let left = self.translate(builder, left);
                let right = self.translate(builder, right);
                builder.ins().iadd(left, right)
            }
            Expr::Id => {
                self.args[0]
            }
            Expr::Int(value) => {
                builder.ins().iconst(types::I64, value)
            }
            _ => todo!(),
        }
    }
}

fn main() {
    let module_builder = JITBuilder::new(cranelift_module::default_libcall_names()).unwrap();
    let mut module = JITModule::new(module_builder);

    let ptr_ty = module.isa().pointer_type();

    let mut ctx = module.make_context();
    ctx.func.signature = module.make_signature();
    ctx.func.signature.returns.push(AbiParam::new(ptr_ty));

    let mut fn_builder_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fn_builder_ctx);
    let block = builder.create_block();
    builder.switch_to_block(block);
    let ret = {
        let int10 = {
            let func = builder.ins().iconst(ptr_ty, int as i64);
            let arg = builder.ins().iconst(types::I64, 10);
            let sig = {
                let mut sig = module.make_signature();
                sig.params.push(AbiParam::new(types::I64));
                sig.returns.push(AbiParam::new(ptr_ty));
                builder.import_signature(sig)
            };
            let inst = builder.ins().call_indirect(sig, func, &[arg]);
            builder.inst_results(inst)[0]
        };
        {
            let cap = builder.create_sized_stack_slot(StackSlotData {
                kind: StackSlotKind::ExplicitSlot,
                size: types::I64.bytes(),
            });
            let args = {
                let func = builder.ins().iconst(ptr_ty, init_args as i64);
                {
                    let value = builder.ins().iconst(types::I64, 2);
                    builder.ins().stack_store(value, cap, 0);
                }
                let arg = builder.ins().stack_addr(types::I64, cap, 0);
                let sig = {
                    let mut sig = module.make_signature();
                    sig.params.push(AbiParam::new(ptr_ty));
                    sig.returns.push(AbiParam::new(ptr_ty));
                    builder.import_signature(sig)
                };
                let inst = builder.ins().call_indirect(sig, func, &[arg]);
                builder.inst_results(inst)[0]
            };
            {
                let int1 = {
                    let func = builder.ins().iconst(ptr_ty, int as i64);
                    let arg = builder.ins().iconst(types::I64, 1);
                    let sig = {
                        let mut sig = module.make_signature();
                        sig.params.push(AbiParam::new(types::I64));
                        sig.returns.push(AbiParam::new(ptr_ty));
                        builder.import_signature(sig)
                    };
                    let inst = builder.ins().call_indirect(sig, func, &[arg]);
                    builder.inst_results(inst)[0]
                };
                let int2 = {
                    let func = builder.ins().iconst(ptr_ty, int as i64);
                    let arg = builder.ins().iconst(types::I64, 2);
                    let sig = {
                        let mut sig = module.make_signature();
                        sig.params.push(AbiParam::new(types::I64));
                        sig.returns.push(AbiParam::new(ptr_ty));
                        builder.import_signature(sig)
                    };
                    let inst = builder.ins().call_indirect(sig, func, &[arg]);
                    builder.inst_results(inst)[0]
                };
                // builder.ins().store(MemFlags, x, p, Offset)
            }
            builder.ins().iconst(ptr_ty, app as i64);
        }
        int10
    };
    builder.ins().return_(&[ret]);
    let func = module
        .declare_anonymous_function(&ctx.func.signature)
        .unwrap();
    module.define_function(func, &mut ctx).unwrap();
    println!("{}", ctx.func.display());
    module.finalize_definitions().unwrap();
    let ptr = module.get_finalized_function(func);
    let func: unsafe fn() -> *const Expr = unsafe { std::mem::transmute(ptr) };
    let expr = unsafe { Rc::from_raw(func()) };
    println!("{:?}", expr);
}
*/
