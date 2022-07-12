use cranelift::prelude::*;
use cranelift_module::Module;

fn main() {
    let jit_builder =
        cranelift_jit::JITBuilder::new(cranelift_module::default_libcall_names()).unwrap();
    let mut module = cranelift_jit::JITModule::new(jit_builder);

    let mut sig = module.make_signature();
    sig.returns.push(AbiParam::new(types::F32));
    sig.params.push(AbiParam::new(types::I32));
    sig.params.push(AbiParam::new(types::I32));
    let mut fn_builder_ctx = FunctionBuilderContext::new();
    let func = module
        .declare_function("average", cranelift_module::Linkage::Local, &sig)
        .unwrap();

    let mut ctx = module.make_context();
    ctx.func.signature = sig;
    ctx.func.name = ExternalName::user(0, func.as_u32());
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fn_builder_ctx);

    let ss0 = builder.create_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8));
    let block1 = builder.create_block();
    let block2 = builder.create_block();
    let block3 = builder.create_block();
    let block4 = builder.create_block();
    let block5 = builder.create_block();

    builder.append_block_params_for_function_params(block1);
    builder.switch_to_block(block1);
    let v0 = builder.block_params(block1)[0];
    let v1 = builder.block_params(block1)[1];
    let v2 = builder.ins().f64const(0.);
    builder.ins().stack_store(v2, ss0, 0);
    builder.ins().brz(v1, block5, &[]);
    builder.ins().jump(block2, &[]);

    builder.switch_to_block(block2);
    let v3 = builder.ins().iconst(types::I32, 0);
    builder.ins().jump(block3, &[v3]);

    let v4 = builder.append_block_param(block3, types::I32);
    builder.switch_to_block(block3);
    let v5 = builder.ins().imul_imm(v4, 4);
    let v6 = builder.ins().iadd(v0, v5);
    let v7 = builder.ins().load(types::F32, MemFlags::new(), v6, 0);
    let v8 = builder.ins().fpromote(types::F64, v7);
    let v9 = builder.ins().stack_load(types::F64, ss0, 0);
    let v10 = builder.ins().fadd(v8, v9);
    builder.ins().stack_store(v10, ss0, 0);
    let v11 = builder.ins().iadd_imm(v4, 1);
    let v12 = builder.ins().icmp(IntCC::UnsignedLessThan, v11, v1);
    builder.ins().brnz(v12, block3, &[v11]);
    builder.ins().jump(block4, &[]);

    builder.switch_to_block(block4);
    let v13 = builder.ins().stack_load(types::F64, ss0, 0);
    let v14 = builder.ins().fcvt_from_uint(types::F64, v1);
    let v15 = builder.ins().fdiv(v13, v14);
    let v16 = builder.ins().fdemote(types::F32, v15);
    builder.ins().return_(&[v16]);

    builder.switch_to_block(block5);
    let v100 = builder.ins().f32const(f32::NAN);
    builder.ins().return_(&[v100]);

    builder.seal_all_blocks();
    builder.finalize();
    println!("{}", ctx.func.display());

    module.define_function(func, &mut ctx).unwrap();
    module.clear_context(&mut ctx);
    module.finalize_definitions();
    let code = module.get_finalized_function(func);
    let ptr = unsafe { std::mem::transmute::<_, fn(*const f32, u32) -> f32>(code) };
    let result = ptr(&[30f32, 20., 30.] as *const f32, 3);
    println!("{}", result);
}
