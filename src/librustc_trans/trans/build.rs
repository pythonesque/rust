// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(dead_code)] // FFI wrappers
#![allow(non_snake_case)]

use llvm;
use llvm::{CallConv, AtomicBinOp, AtomicOrdering, SynchronizationScope, AsmDialect, AttrBuilder};
use llvm::{Opcode, IntPredicate, RealPredicate};
use llvm::{ValueRef, BasicBlockRef};
use trans::common::*;
use syntax::codemap::Span;

use trans::builder::Builder;
use trans::type_::Type;
use trans::debuginfo::DebugLoc;

use libc::{c_uint, c_char};

pub fn terminate(cx: &mut BlockContext, _: &str) {
    debug!("terminate({})", cx.to_str());
    cx.bl.terminated.set(true);
}

pub fn check_not_terminated(cx: &mut BlockContext) {
    if cx.bl.terminated.get() {
        panic!("already terminated!");
    }
}

pub fn B<'r, 'blk, 'tcx>(cx: &mut BlockContext<'r, 'blk, 'tcx>) -> Builder<'blk, 'tcx> {
    let b = cx.fcx.ccx.builder();
    b.position_at_end(cx.bl.llbb);
    b
}

// The difference between a block being unreachable and being terminated is
// somewhat obscure, and has to do with error checking. When a block is
// terminated, we're saying that trying to add any further statements in the
// block is an error. On the other hand, if something is unreachable, that
// means that the block was terminated in some way that we don't want to check
// for (panic/break/return statements, call to diverging functions, etc), and
// further instructions to the block should simply be ignored.

pub fn RetVoid(cx: &mut BlockContext, debug_loc: DebugLoc) {
    if cx.bl.unreachable.get() {
        return;
    }
    check_not_terminated(cx);
    terminate(cx, "RetVoid");
    debug_loc.apply(cx.fcx);
    B(cx).ret_void();
}

pub fn Ret(cx: &mut BlockContext, v: ValueRef, debug_loc: DebugLoc) {
    if cx.bl.unreachable.get() {
        return;
    }
    check_not_terminated(cx);
    terminate(cx, "Ret");
    debug_loc.apply(cx.fcx);
    B(cx).ret(v);
}

pub fn AggregateRet(cx: &mut BlockContext,
                    ret_vals: &[ValueRef],
                    debug_loc: DebugLoc) {
    if cx.bl.unreachable.get() {
        return;
    }
    check_not_terminated(cx);
    terminate(cx, "AggregateRet");
    debug_loc.apply(cx.fcx);
    B(cx).aggregate_ret(ret_vals);
}

pub fn Br(cx: &mut BlockContext, dest: BasicBlockRef, debug_loc: DebugLoc) {
    if cx.bl.unreachable.get() {
        return;
    }
    check_not_terminated(cx);
    terminate(cx, "Br");
    debug_loc.apply(cx.fcx);
    B(cx).br(dest);
}

pub fn CondBr(cx: &mut BlockContext,
              if_: ValueRef,
              then: BasicBlockRef,
              else_: BasicBlockRef,
              debug_loc: DebugLoc) {
    if cx.bl.unreachable.get() {
        return;
    }
    check_not_terminated(cx);
    terminate(cx, "CondBr");
    debug_loc.apply(cx.fcx);
    B(cx).cond_br(if_, then, else_);
}

pub fn Switch(cx: &mut BlockContext, v: ValueRef, else_: BasicBlockRef, num_cases: usize)
    -> ValueRef {
    if cx.bl.unreachable.get() { return _Undef(v); }
    check_not_terminated(cx);
    terminate(cx, "Switch");
    B(cx).switch(v, else_, num_cases)
}

pub fn AddCase(s: ValueRef, on_val: ValueRef, dest: BasicBlockRef) {
    unsafe {
        if llvm::LLVMIsUndef(s) == llvm::True { return; }
        llvm::LLVMAddCase(s, on_val, dest);
    }
}

pub fn IndirectBr(cx: &mut BlockContext,
                  addr: ValueRef,
                  num_dests: usize,
                  debug_loc: DebugLoc) {
    if cx.bl.unreachable.get() {
        return;
    }
    check_not_terminated(cx);
    terminate(cx, "IndirectBr");
    debug_loc.apply(cx.fcx);
    B(cx).indirect_br(addr, num_dests);
}

pub fn Invoke(cx: &mut BlockContext,
              fn_: ValueRef,
              args: &[ValueRef],
              then: BasicBlockRef,
              catch: BasicBlockRef,
              attributes: Option<AttrBuilder>,
              debug_loc: DebugLoc)
              -> ValueRef {
    if cx.bl.unreachable.get() {
        return C_null(Type::i8(cx.ccx()));
    }
    check_not_terminated(cx);
    terminate(cx, "Invoke");
    debug!("Invoke({} with arguments ({}))",
           cx.val_to_string(fn_),
           args.iter().map(|a| cx.val_to_string(*a)).collect::<Vec<String>>().connect(", "));
    debug_loc.apply(cx.fcx);
    B(cx).invoke(fn_, args, then, catch, attributes)
}

pub fn Unreachable(cx: &mut BlockContext) {
    if cx.bl.unreachable.get() {
        return
    }
    cx.bl.unreachable.set(true);
    if !cx.bl.terminated.get() {
        B(cx).unreachable();
    }
}

pub fn _Undef(val: ValueRef) -> ValueRef {
    unsafe {
        return llvm::LLVMGetUndef(val_ty(val).to_ref());
    }
}

/* Arithmetic */
pub fn Add(cx: &mut BlockContext,
           lhs: ValueRef,
           rhs: ValueRef,
           debug_loc: DebugLoc)
           -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).add(lhs, rhs)
}

pub fn NSWAdd(cx: &mut BlockContext,
              lhs: ValueRef,
              rhs: ValueRef,
              debug_loc: DebugLoc)
              -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).nswadd(lhs, rhs)
}

pub fn NUWAdd(cx: &mut BlockContext,
              lhs: ValueRef,
              rhs: ValueRef,
              debug_loc: DebugLoc)
              -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).nuwadd(lhs, rhs)
}

pub fn FAdd(cx: &mut BlockContext,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).fadd(lhs, rhs)
}

pub fn Sub(cx: &mut BlockContext,
           lhs: ValueRef,
           rhs: ValueRef,
           debug_loc: DebugLoc)
           -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).sub(lhs, rhs)
}

pub fn NSWSub(cx: &mut BlockContext,
              lhs: ValueRef,
              rhs: ValueRef,
              debug_loc: DebugLoc)
              -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).nswsub(lhs, rhs)
}

pub fn NUWSub(cx: &mut BlockContext,
              lhs: ValueRef,
              rhs: ValueRef,
              debug_loc: DebugLoc)
              -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).nuwsub(lhs, rhs)
}

pub fn FSub(cx: &mut BlockContext,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).fsub(lhs, rhs)
}

pub fn Mul(cx: &mut BlockContext,
           lhs: ValueRef,
           rhs: ValueRef,
           debug_loc: DebugLoc)
           -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).mul(lhs, rhs)
}

pub fn NSWMul(cx: &mut BlockContext,
              lhs: ValueRef,
              rhs: ValueRef,
              debug_loc: DebugLoc)
              -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).nswmul(lhs, rhs)
}

pub fn NUWMul(cx: &mut BlockContext,
              lhs: ValueRef,
              rhs: ValueRef,
              debug_loc: DebugLoc)
              -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).nuwmul(lhs, rhs)
}

pub fn FMul(cx: &mut BlockContext,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).fmul(lhs, rhs)
}

pub fn UDiv(cx: &mut BlockContext,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).udiv(lhs, rhs)
}

pub fn SDiv(cx: &mut BlockContext,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).sdiv(lhs, rhs)
}

pub fn ExactSDiv(cx: &mut BlockContext,
                 lhs: ValueRef,
                 rhs: ValueRef,
                 debug_loc: DebugLoc)
                 -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).exactsdiv(lhs, rhs)
}

pub fn FDiv(cx: &mut BlockContext,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).fdiv(lhs, rhs)
}

pub fn URem(cx: &mut BlockContext,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).urem(lhs, rhs)
}

pub fn SRem(cx: &mut BlockContext,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).srem(lhs, rhs)
}

pub fn FRem(cx: &mut BlockContext,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).frem(lhs, rhs)
}

pub fn Shl(cx: &mut BlockContext,
           lhs: ValueRef,
           rhs: ValueRef,
           debug_loc: DebugLoc)
           -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).shl(lhs, rhs)
}

pub fn LShr(cx: &mut BlockContext,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).lshr(lhs, rhs)
}

pub fn AShr(cx: &mut BlockContext,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).ashr(lhs, rhs)
}

pub fn And(cx: &mut BlockContext,
           lhs: ValueRef,
           rhs: ValueRef,
           debug_loc: DebugLoc)
           -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).and(lhs, rhs)
}

pub fn Or(cx: &mut BlockContext,
          lhs: ValueRef,
          rhs: ValueRef,
          debug_loc: DebugLoc)
          -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).or(lhs, rhs)
}

pub fn Xor(cx: &mut BlockContext,
           lhs: ValueRef,
           rhs: ValueRef,
           debug_loc: DebugLoc)
           -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).xor(lhs, rhs)
}

pub fn BinOp(cx: &mut BlockContext,
             op: Opcode,
             lhs: ValueRef,
             rhs: ValueRef,
             debug_loc: DebugLoc)
          -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).binop(op, lhs, rhs)
}

pub fn Neg(cx: &mut BlockContext, v: ValueRef, debug_loc: DebugLoc) -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(v);
    }
    debug_loc.apply(cx.fcx);
    B(cx).neg(v)
}

pub fn NSWNeg(cx: &mut BlockContext, v: ValueRef, debug_loc: DebugLoc) -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(v);
    }
    debug_loc.apply(cx.fcx);
    B(cx).nswneg(v)
}

pub fn NUWNeg(cx: &mut BlockContext, v: ValueRef, debug_loc: DebugLoc) -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(v);
    }
    debug_loc.apply(cx.fcx);
    B(cx).nuwneg(v)
}
pub fn FNeg(cx: &mut BlockContext, v: ValueRef, debug_loc: DebugLoc) -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(v);
    }
    debug_loc.apply(cx.fcx);
    B(cx).fneg(v)
}

pub fn Not(cx: &mut BlockContext, v: ValueRef, debug_loc: DebugLoc) -> ValueRef {
    if cx.bl.unreachable.get() {
        return _Undef(v);
    }
    debug_loc.apply(cx.fcx);
    B(cx).not(v)
}

/* Memory */
pub fn Malloc(cx: &mut BlockContext, ty: Type, debug_loc: DebugLoc) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::i8p(cx.ccx()).to_ref());
        }
        debug_loc.apply(cx.fcx);
        B(cx).malloc(ty)
    }
}

pub fn ArrayMalloc(cx: &mut BlockContext,
                   ty: Type,
                   val: ValueRef,
                   debug_loc: DebugLoc) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::i8p(cx.ccx()).to_ref());
        }
        debug_loc.apply(cx.fcx);
        B(cx).array_malloc(ty, val)
    }
}

pub fn Alloca(cx: &mut BlockContext, ty: Type, name: &str) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(ty.ptr_to().to_ref()); }
        AllocaFcx(cx.fcx, ty, name)
    }
}

pub fn AllocaFcx(fcx: &mut FunctionContext, ty: Type, name: &str) -> ValueRef {
    let b = fcx.ccx.builder();
    b.position_before(fcx.alloca_insert_pt.unwrap());
    DebugLoc::None.apply(fcx);
    b.alloca(ty, name)
}

pub fn ArrayAlloca(cx: &mut BlockContext, ty: Type, val: ValueRef) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(ty.ptr_to().to_ref()); }
        let b = cx.fcx.ccx.builder();
        b.position_before(cx.fcx.alloca_insert_pt.unwrap());
        DebugLoc::None.apply(cx.fcx);
        b.array_alloca(ty, val)
    }
}

pub fn Free(cx: &mut BlockContext, pointer_val: ValueRef) {
    if cx.bl.unreachable.get() { return; }
    B(cx).free(pointer_val)
}

pub fn Load(cx: &mut BlockContext, pointer_val: ValueRef) -> ValueRef {
    unsafe {
        let ccx = cx.fcx.ccx;
        if cx.bl.unreachable.get() {
            let ty = val_ty(pointer_val);
            let eltty = if ty.kind() == llvm::Array {
                ty.element_type()
            } else {
                ccx.int_type()
            };
            return llvm::LLVMGetUndef(eltty.to_ref());
        }
        B(cx).load(pointer_val)
    }
}

pub fn VolatileLoad(cx: &mut BlockContext, pointer_val: ValueRef) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).to_ref());
        }
        B(cx).volatile_load(pointer_val)
    }
}

pub fn AtomicLoad(cx: &mut BlockContext, pointer_val: ValueRef, order: AtomicOrdering) -> ValueRef {
    unsafe {
        let ccx = cx.fcx.ccx;
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(ccx.int_type().to_ref());
        }
        B(cx).atomic_load(pointer_val, order)
    }
}


pub fn LoadRangeAssert(cx: &mut BlockContext, pointer_val: ValueRef, lo: u64,
                       hi: u64, signed: llvm::Bool) -> ValueRef {
    if cx.bl.unreachable.get() {
        let ccx = cx.fcx.ccx;
        let ty = val_ty(pointer_val);
        let eltty = if ty.kind() == llvm::Array {
            ty.element_type()
        } else {
            ccx.int_type()
        };
        unsafe {
            llvm::LLVMGetUndef(eltty.to_ref())
        }
    } else {
        B(cx).load_range_assert(pointer_val, lo, hi, signed)
    }
}

pub fn LoadNonNull(cx: &mut BlockContext, ptr: ValueRef) -> ValueRef {
    if cx.bl.unreachable.get() {
        let ccx = cx.fcx.ccx;
        let ty = val_ty(ptr);
        let eltty = if ty.kind() == llvm::Array {
            ty.element_type()
        } else {
            ccx.int_type()
        };
        unsafe {
            llvm::LLVMGetUndef(eltty.to_ref())
        }
    } else {
        B(cx).load_nonnull(ptr)
    }
}

pub fn Store(cx: &mut BlockContext, val: ValueRef, ptr: ValueRef) -> ValueRef {
    if cx.bl.unreachable.get() { return C_nil(cx.ccx()); }
    B(cx).store(val, ptr)
}

pub fn VolatileStore(cx: &mut BlockContext, val: ValueRef, ptr: ValueRef) -> ValueRef {
    if cx.bl.unreachable.get() { return C_nil(cx.ccx()); }
    B(cx).volatile_store(val, ptr)
}

pub fn AtomicStore(cx: &mut BlockContext, val: ValueRef, ptr: ValueRef, order: AtomicOrdering) {
    if cx.bl.unreachable.get() { return; }
    B(cx).atomic_store(val, ptr, order)
}

pub fn GEP(cx: &mut BlockContext, pointer: ValueRef, indices: &[ValueRef]) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).ptr_to().to_ref());
        }
        B(cx).gep(pointer, indices)
    }
}

// Simple wrapper around GEP that takes an array of ints and wraps them
// in C_i32()
#[inline]
pub fn GEPi(cx: &mut BlockContext, base: ValueRef, ixs: &[usize]) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).ptr_to().to_ref());
        }
        B(cx).gepi(base, ixs)
    }
}

pub fn InBoundsGEP(cx: &mut BlockContext, pointer: ValueRef, indices: &[ValueRef]) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).ptr_to().to_ref());
        }
        B(cx).inbounds_gep(pointer, indices)
    }
}

pub fn StructGEP(cx: &mut BlockContext, pointer: ValueRef, idx: usize) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).ptr_to().to_ref());
        }
        B(cx).struct_gep(pointer, idx)
    }
}

pub fn GlobalString(cx: &mut BlockContext, _str: *const c_char) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::i8p(cx.ccx()).to_ref());
        }
        B(cx).global_string(_str)
    }
}

pub fn GlobalStringPtr(cx: &mut BlockContext, _str: *const c_char) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::i8p(cx.ccx()).to_ref());
        }
        B(cx).global_string_ptr(_str)
    }
}

/* Casts */
pub fn Trunc(cx: &mut BlockContext, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).trunc(val, dest_ty)
    }
}

pub fn ZExt(cx: &mut BlockContext, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).zext(val, dest_ty)
    }
}

pub fn SExt(cx: &mut BlockContext, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).sext(val, dest_ty)
    }
}

pub fn FPToUI(cx: &mut BlockContext, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).fptoui(val, dest_ty)
    }
}

pub fn FPToSI(cx: &mut BlockContext, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).fptosi(val, dest_ty)
    }
}

pub fn UIToFP(cx: &mut BlockContext, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).uitofp(val, dest_ty)
    }
}

pub fn SIToFP(cx: &mut BlockContext, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).sitofp(val, dest_ty)
    }
}

pub fn FPTrunc(cx: &mut BlockContext, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).fptrunc(val, dest_ty)
    }
}

pub fn FPExt(cx: &mut BlockContext, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).fpext(val, dest_ty)
    }
}

pub fn PtrToInt(cx: &mut BlockContext, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).ptrtoint(val, dest_ty)
    }
}

pub fn IntToPtr(cx: &mut BlockContext, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).inttoptr(val, dest_ty)
    }
}

pub fn BitCast(cx: &mut BlockContext, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).bitcast(val, dest_ty)
    }
}

pub fn ZExtOrBitCast(cx: &mut BlockContext, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).zext_or_bitcast(val, dest_ty)
    }
}

pub fn SExtOrBitCast(cx: &mut BlockContext, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).sext_or_bitcast(val, dest_ty)
    }
}

pub fn TruncOrBitCast(cx: &mut BlockContext, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).trunc_or_bitcast(val, dest_ty)
    }
}

pub fn Cast(cx: &mut BlockContext, op: Opcode, val: ValueRef, dest_ty: Type,
            _: *const u8)
     -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).cast(op, val, dest_ty)
    }
}

pub fn PointerCast(cx: &mut BlockContext, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).pointercast(val, dest_ty)
    }
}

pub fn IntCast(cx: &mut BlockContext, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).intcast(val, dest_ty)
    }
}

pub fn FPCast(cx: &mut BlockContext, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).fpcast(val, dest_ty)
    }
}


/* Comparisons */
pub fn ICmp(cx: &mut BlockContext,
            op: IntPredicate,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::i1(cx.ccx()).to_ref());
        }
        debug_loc.apply(cx.fcx);
        B(cx).icmp(op, lhs, rhs)
    }
}

pub fn FCmp(cx: &mut BlockContext,
            op: RealPredicate,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::i1(cx.ccx()).to_ref());
        }
        debug_loc.apply(cx.fcx);
        B(cx).fcmp(op, lhs, rhs)
    }
}

/* Miscellaneous instructions */
pub fn EmptyPhi(cx: &mut BlockContext, ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(ty.to_ref()); }
        B(cx).empty_phi(ty)
    }
}

pub fn Phi(cx: &mut BlockContext, ty: Type, vals: &[ValueRef],
           bbs: &[BasicBlockRef]) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(ty.to_ref()); }
        B(cx).phi(ty, vals, bbs)
    }
}

pub fn AddIncomingToPhi(phi: ValueRef, val: ValueRef, bb: BasicBlockRef) {
    unsafe {
        if llvm::LLVMIsUndef(phi) == llvm::True { return; }
        llvm::LLVMAddIncoming(phi, &val, &bb, 1 as c_uint);
    }
}

pub fn _UndefReturn(cx: &mut BlockContext, fn_: ValueRef) -> ValueRef {
    unsafe {
        let ccx = cx.fcx.ccx;
        let ty = val_ty(fn_);
        let retty = if ty.kind() == llvm::Function {
            ty.return_type()
        } else {
            ccx.int_type()
        };
        B(cx).count_insn("ret_undef");
        llvm::LLVMGetUndef(retty.to_ref())
    }
}

pub fn add_span_comment(cx: &mut BlockContext, sp: Span, text: &str) {
    B(cx).add_span_comment(sp, text)
}

pub fn add_comment(cx: &mut BlockContext, text: &str) {
    B(cx).add_comment(text)
}

pub fn InlineAsmCall(cx: &mut BlockContext, asm: *const c_char, cons: *const c_char,
                     inputs: &[ValueRef], output: Type,
                     volatile: bool, alignstack: bool,
                     dia: AsmDialect) -> ValueRef {
    B(cx).inline_asm_call(asm, cons, inputs, output, volatile, alignstack, dia)
}

pub fn Call(cx: &mut BlockContext,
            fn_: ValueRef,
            args: &[ValueRef],
            attributes: Option<AttrBuilder>,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.bl.unreachable.get() {
        return _UndefReturn(cx, fn_);
    }
    debug_loc.apply(cx.fcx);
    B(cx).call(fn_, args, attributes)
}

pub fn CallWithConv(cx: &mut BlockContext,
                    fn_: ValueRef,
                    args: &[ValueRef],
                    conv: CallConv,
                    attributes: Option<AttrBuilder>,
                    debug_loc: DebugLoc)
                    -> ValueRef {
    if cx.bl.unreachable.get() {
        return _UndefReturn(cx, fn_);
    }
    debug_loc.apply(cx.fcx);
    B(cx).call_with_conv(fn_, args, conv, attributes)
}

pub fn AtomicFence(cx: &mut BlockContext, order: AtomicOrdering, scope: SynchronizationScope) {
    if cx.bl.unreachable.get() { return; }
    B(cx).atomic_fence(order, scope)
}

pub fn Select(cx: &mut BlockContext, if_: ValueRef, then: ValueRef, else_: ValueRef) -> ValueRef {
    if cx.bl.unreachable.get() { return _Undef(then); }
    B(cx).select(if_, then, else_)
}

pub fn VAArg(cx: &mut BlockContext, list: ValueRef, ty: Type) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(ty.to_ref()); }
        B(cx).va_arg(list, ty)
    }
}

pub fn ExtractElement(cx: &mut BlockContext, vec_val: ValueRef, index: ValueRef) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).to_ref());
        }
        B(cx).extract_element(vec_val, index)
    }
}

pub fn InsertElement(cx: &mut BlockContext, vec_val: ValueRef, elt_val: ValueRef,
                     index: ValueRef) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).to_ref());
        }
        B(cx).insert_element(vec_val, elt_val, index)
    }
}

pub fn ShuffleVector(cx: &mut BlockContext, v1: ValueRef, v2: ValueRef,
                     mask: ValueRef) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).to_ref());
        }
        B(cx).shuffle_vector(v1, v2, mask)
    }
}

pub fn VectorSplat(cx: &mut BlockContext, num_elts: usize, elt_val: ValueRef) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).to_ref());
        }
        B(cx).vector_splat(num_elts, elt_val)
    }
}

pub fn ExtractValue(cx: &mut BlockContext, agg_val: ValueRef, index: usize) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).to_ref());
        }
        B(cx).extract_value(agg_val, index)
    }
}

pub fn InsertValue(cx: &mut BlockContext,
                   agg_val: ValueRef,
                   elt_val: ValueRef,
                   index: usize) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).to_ref());
        }
        B(cx).insert_value(agg_val, elt_val, index)
    }
}

pub fn IsNull(cx: &mut BlockContext, val: ValueRef) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::i1(cx.ccx()).to_ref());
        }
        B(cx).is_null(val)
    }
}

pub fn IsNotNull(cx: &mut BlockContext, val: ValueRef) -> ValueRef {
    unsafe {
        if cx.bl.unreachable.get() {
            return llvm::LLVMGetUndef(Type::i1(cx.ccx()).to_ref());
        }
        B(cx).is_not_null(val)
    }
}

pub fn PtrDiff(cx: &mut BlockContext, lhs: ValueRef, rhs: ValueRef) -> ValueRef {
    unsafe {
        let ccx = cx.fcx.ccx;
        if cx.bl.unreachable.get() { return llvm::LLVMGetUndef(ccx.int_type().to_ref()); }
        B(cx).ptrdiff(lhs, rhs)
    }
}

pub fn Trap(cx: &mut BlockContext) {
    if cx.bl.unreachable.get() { return; }
    B(cx).trap();
}

pub fn LandingPad(cx: &mut BlockContext, ty: Type, pers_fn: ValueRef,
                  num_clauses: usize) -> ValueRef {
    check_not_terminated(cx);
    assert!(!cx.bl.unreachable.get());
    B(cx).landing_pad(ty, pers_fn, num_clauses)
}

pub fn SetCleanup(cx: &mut BlockContext, landing_pad: ValueRef) {
    B(cx).set_cleanup(landing_pad)
}

pub fn Resume(cx: &mut BlockContext, exn: ValueRef) -> ValueRef {
    check_not_terminated(cx);
    terminate(cx, "Resume");
    B(cx).resume(exn)
}

// Atomic Operations
pub fn AtomicCmpXchg(cx: &mut BlockContext, dst: ValueRef,
                     cmp: ValueRef, src: ValueRef,
                     order: AtomicOrdering,
                     failure_order: AtomicOrdering) -> ValueRef {
    B(cx).atomic_cmpxchg(dst, cmp, src, order, failure_order)
}
pub fn AtomicRMW(cx: &mut BlockContext, op: AtomicBinOp,
                 dst: ValueRef, src: ValueRef,
                 order: AtomicOrdering) -> ValueRef {
    B(cx).atomic_rmw(op, dst, src, order)
}
