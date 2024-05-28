use std::iter;

use codegen::ir;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Module;
use isa::CallConv;

enum Expr {
    Var(usize),
    Int(i64),
    Ptr(i64),
    Assign,
    Deref(Ty),
    Id,
    Add,
    Call(Vec<Ty>, Box<Ty>, Vec<Ty>),
    Comp {
        outer_func: Box<Expr>,
        inner_funcs: Vec<Expr>,
    },
}

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

fn define(
    module: &mut JITModule,
    exprs: &[Expr],
    args_ty: &[Ty],
    ret_ty: &Ty,
    vars_ty: &[Ty],
) -> *const u8 {
    let mut ctx = module.make_context();
    ctx.func.signature = module.make_signature();
    let ptr_ty = module.isa().pointer_type();
    let call_conv = ctx.func.signature.call_conv;
    ctx.func.signature.params.push(AbiParam::new(ptr_ty));
    for arg_ty in args_ty {
        ctx.func
            .signature
            .params
            .push(AbiParam::new(arg_ty.translate(ptr_ty)));
    }
    ctx.func
        .signature
        .returns
        .push(AbiParam::new(ret_ty.translate(ptr_ty)));
    let mut fn_builder_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fn_builder_ctx);
    let slots: Vec<_> = {
        vars_ty
            .iter()
            .map(|ty| {
                let data = StackSlotData {
                    kind: StackSlotKind::ExplicitSlot,
                    size: ty.translate(ptr_ty).bytes(),
                };
                builder.create_sized_stack_slot(data)
            })
            .collect()
    };
    let block = builder.create_block();
    builder.append_block_params_for_function_params(block);
    builder.switch_to_block(block);
    let args = builder.block_params(block).to_vec();
    let ret = exprs
        .iter()
        .map(|expr| expr.translate(&mut builder, ptr_ty, &args, &slots, call_conv))
        .last()
        .unwrap();
    builder.ins().return_(&[ret]);
    let func = module
        .declare_anonymous_function(&ctx.func.signature)
        .unwrap();
    module.define_function(func, &mut ctx).unwrap();
    println!("{}", ctx.func.display());
    module.finalize_definitions().unwrap();
    module.get_finalized_function(func)
}

impl Expr {
    fn translate(
        &self,
        builder: &mut FunctionBuilder,
        ptr_ty: Type,
        args: &[Value],
        slots: &[ir::StackSlot],
        call_conv: CallConv,
    ) -> Value {
        match *self {
            Expr::Var(idx) => builder.ins().stack_addr(ptr_ty, slots[idx], 0),
            Expr::Assign => {
                builder.ins().store(MemFlags::new(), args[2], args[1], 0);
                args[1]
            }
            Expr::Deref(ref ty) => {
                builder
                    .ins()
                    .load(ty.translate(ptr_ty), MemFlags::new(), args[1], 0)
            }
            Expr::Id => args[1],
            Expr::Int(value) => builder.ins().iconst(types::I64, value),
            Expr::Ptr(value) => builder.ins().iconst(ptr_ty, value),
            Expr::Add => builder.ins().iadd(args[1], args[2]),
            Expr::Call(ref args_ty, ref ret_ty, ref vars_ty) => {
                let func = {
                    let define_ptr = builder.ins().iconst(ptr_ty, define as i64);
                    let define_args = vec![
                        args[0],
                        args[1],
                        builder.ins().iconst(ptr_ty, 1),
                        builder.ins().iconst(ptr_ty, args_ty as *const _ as i64),
                        builder.ins().iconst(ptr_ty, args_ty.len() as i64),
                        builder.ins().iconst(ptr_ty, ret_ty as *const _ as i64),
                        builder.ins().iconst(ptr_ty, vars_ty as *const _ as i64),
                        builder.ins().iconst(ptr_ty, vars_ty.len() as i64),
                    ];

                    let mut sig = Signature::new(call_conv);
                    for _ in &define_args {
                        sig.params.push(AbiParam::new(ptr_ty));
                    }
                    sig.returns.push(AbiParam::new(ptr_ty));
                    let sig = builder.import_signature(sig);

                    let define_inst = builder.ins().call_indirect(sig, define_ptr, &define_args);
                    builder.inst_results(define_inst)[0]
                };

                let mut sig = Signature::new(call_conv);
                sig.params.push(AbiParam::new(ptr_ty));
                for arg_ty in args_ty {
                    sig.params.push(AbiParam::new(arg_ty.translate(ptr_ty)));
                }
                sig.returns.push(AbiParam::new(ret_ty.translate(ptr_ty)));
                let sig = builder.import_signature(sig);
                let new_args: Vec<_> = iter::once(&args[0]).chain(&args[2..]).cloned().collect();
                let inst = builder.ins().call_indirect(sig, func, &new_args);
                builder.inst_results(inst)[0]
            }
            Expr::Comp {
                ref outer_func,
                ref inner_funcs,
            } => {
                let intermediates: Vec<_> = iter::once(args[0])
                    .chain(inner_funcs.iter().map(|inner_func| {
                        inner_func.translate(builder, ptr_ty, args, slots, call_conv)
                    }))
                    .collect();
                outer_func.translate(builder, ptr_ty, &intermediates, slots, call_conv)
            }
        }
    }
}

fn main() {
    let jit_module_builder = JITBuilder::new(cranelift_module::default_libcall_names()).unwrap();
    let mut jit_module = JITModule::new(jit_module_builder);

    {
        let expr = Expr::Comp {
            outer_func: Box::new(Expr::Deref(Ty::Int)),
            inner_funcs: vec![Expr::Comp {
                outer_func: Box::new(Expr::Assign),
                inner_funcs: vec![Expr::Var(0), Expr::Int(10)],
            }],
        };
        let ptr = define(&mut jit_module, &[expr], &[], &Ty::Int, &[Ty::Int]);
        let func: unsafe fn(*mut JITModule) -> i32 = unsafe { std::mem::transmute(ptr) };
        dbg!(unsafe { func(&mut jit_module) });
    }
    {
        let expr = Expr::Comp {
            outer_func: Box::new(Expr::Add),
            inner_funcs: vec![Expr::Id, Expr::Int(1)],
        };
        let ptr = define(&mut jit_module, &[expr], &[Ty::Int], &Ty::Int, &[]);
        let func: unsafe fn(*mut JITModule, i64) -> i64 = unsafe { std::mem::transmute(ptr) };
        dbg!(unsafe { func(&mut jit_module, 10) });
    }
    {
        let expr1 = Expr::Comp {
            outer_func: Box::new(Expr::Assign),
            inner_funcs: vec![
                Expr::Var(0),
                Expr::Ptr(Box::into_raw(Box::new(Expr::Comp {
                    outer_func: Box::new(Expr::Add),
                    inner_funcs: vec![Expr::Id, Expr::Int(1)],
                })) as i64),
            ],
        };
        let expr2 = Expr::Comp {
            outer_func: Box::new(Expr::Call(vec![Ty::Int], Box::new(Ty::Int), vec![])),
            inner_funcs: vec![
                Expr::Comp {
                    outer_func: Box::new(Expr::Deref(Ty::Int)),
                    inner_funcs: vec![Expr::Var(0)],
                },
                Expr::Int(10),
            ],
        };
        let ptr = define(&mut jit_module, &[expr1, expr2], &[], &Ty::Int, &[Ty::Int]);
        let func: unsafe fn(*mut JITModule) -> i64 = unsafe { std::mem::transmute(ptr) };
        dbg!(unsafe { func(&mut jit_module) });
    }
}
