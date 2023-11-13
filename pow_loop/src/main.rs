use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Module;

fn main() {
    let mut module =
        JITModule::new(JITBuilder::new(cranelift_module::default_libcall_names()).unwrap());
    let ptr_ty = module.target_config().pointer_type();

    let mut ctx = module.make_context();
    ctx.func.signature = module.make_signature();
    ctx.func.signature.params.push(AbiParam::new(ptr_ty));
    ctx.func.signature.returns.push(AbiParam::new(ptr_ty));
    let mut fn_builder_ctx = FunctionBuilderContext::new();

    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fn_builder_ctx);

    let var = Variable::new(0);
    let counter = Variable::new(1);
    builder.declare_var(var, ptr_ty);
    builder.declare_var(counter, ptr_ty);

    let entry_block = builder.create_block();
    let while_block = builder.create_block();
    let ret_block = builder.create_block();

    builder.append_block_params_for_function_params(entry_block);
    builder.switch_to_block(entry_block);

    let arg = builder.block_params(entry_block)[0];
    let one = builder.ins().iconst(ptr_ty, 1);
    builder.def_var(var, one);
    let zero = builder.ins().iconst(ptr_ty, 0);
    builder.def_var(counter, zero);
    builder.ins().jump(while_block, &[]);
    builder.seal_block(entry_block);

    builder.switch_to_block(while_block);
    let c = builder.use_var(counter);
    let c1 = builder.ins().iadd(c, one);
    builder.def_var(counter, c1);
    let v = builder.use_var(var);
    let two = builder.ins().iconst(ptr_ty, 2);
    let v2 = builder.ins().imul(v, two);
    builder.def_var(var, v2);
    let c = builder.use_var(counter);
    let res = builder.ins().icmp(IntCC::SignedLessThan, c, arg);
    builder.ins().brif(res, while_block, &[], ret_block, &[]);
    builder.seal_block(while_block);

    builder.switch_to_block(ret_block);
    let ret = builder.use_var(var);
    builder.ins().return_(&[ret]);
    builder.seal_block(ret_block);
    builder.finalize();
    println!("{}", ctx.func.display());

    let func = module
        .declare_function("", cranelift_module::Linkage::Local, &ctx.func.signature)
        .unwrap();
    module.define_function(func, &mut ctx).unwrap();
    module.finalize_definitions().unwrap();
    let code = module.get_finalized_function(func);
    let ptr = unsafe { std::mem::transmute::<_, unsafe fn(i64) -> i64>(code) };
    println!("{}", unsafe { ptr(10) });
}
