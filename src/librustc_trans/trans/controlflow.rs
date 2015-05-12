// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use llvm::ValueRef;
use middle::def;
use middle::lang_items::{PanicFnLangItem, PanicBoundsCheckFnLangItem};
use trans::base::*;
use trans::basic_block::BasicBlock;
use trans::build::*;
use trans::callee;
use trans::cleanup::CleanupMethods;
use trans::cleanup;
use trans::common::*;
use trans::consts;
use trans::debuginfo;
use trans::debuginfo::{DebugLoc, ToDebugLoc};
use trans::expr;
use trans;
use middle::ty;
use util::ppaux::Repr;

use syntax::ast;
use syntax::ast_util;
use syntax::parse::token::InternedString;
use syntax::parse::token;
use syntax::visit::Visitor;

pub fn trans_stmt<'r, 'blk, 'tcx>(&mut Block { bl, ref mut fcx }: &mut Block<'r, 'blk, 'tcx>,
                                  s: &ast::Stmt)
                                  -> &'blk BlockS {
    let mut bcx = &mut bl.with(fcx);

    let _icx = push_ctxt("trans_stmt");
    debug!("trans_stmt({})", s.repr(bcx.tcx()));

    if bcx.bl.unreachable.get() {
        return bcx.bl;
    }

    if bcx.sess().asm_comments() {
        let r = s.repr(bcx.tcx());
        add_span_comment(bcx, s.span, &r);
    }

    let id = ast_util::stmt_id(s);
    let cleanup_debug_loc =
        debuginfo::get_cleanup_debug_loc_for_ast_node(bcx.ccx(), id, s.span, false);
    bcx.fcx.push_ast_cleanup_scope(cleanup_debug_loc);

    match s.node {
        ast::StmtExpr(ref e, _) | ast::StmtSemi(ref e, _) => {
            bcx.bl = trans_stmt_semi(bcx, &**e);
        }
        ast::StmtDecl(ref d, _) => {
            match d.node {
                ast::DeclLocal(ref local) => {
                    bcx.bl = init_local(bcx, &**local);
                    debuginfo::create_local_var_metadata(bcx, &**local);
                }
                // Inner items are visited by `trans_item`/`trans_meth`.
                ast::DeclItem(_) => {},
            }
        }
        ast::StmtMac(..) => bcx.tcx().sess.bug("unexpanded macro")
    }

    bcx.fcx.pop_and_trans_ast_cleanup_scope(bcx.bl, ast_util::stmt_id(s))
}

pub fn trans_stmt_semi<'r, 'blk, 'tcx>(cx: &mut Block<'r, 'blk, 'tcx>, e: &ast::Expr)
                                       -> &'blk BlockS {
    let _icx = push_ctxt("trans_stmt_semi");

    if cx.bl.unreachable.get() {
        return cx.bl;
    }

    let ty = expr_ty(cx, e);
    if cx.fcx.type_needs_drop(ty) {
        expr::trans_to_lvalue(cx, e, "stmt").bcx
    } else {
        expr::trans_into(cx, e, expr::Ignore)
    }
}

pub fn trans_block<'r, 'blk, 'tcx>(&mut Block { bl, ref mut fcx }: &mut Block<'r, 'blk, 'tcx>,
                                   b: &ast::Block,
                                   mut dest: expr::Dest)
                                   -> &'blk BlockS {
    let _icx = push_ctxt("trans_block");

    if bl.unreachable.get() {
        return bl;
    }

    let mut bcx = &mut bl.with(fcx);

    let cleanup_debug_loc =
        debuginfo::get_cleanup_debug_loc_for_ast_node(bcx.ccx(), b.id, b.span, true);
    bcx.fcx.push_ast_cleanup_scope(cleanup_debug_loc);

    for s in &b.stmts {
        bcx.bl = trans_stmt(bcx, &**s);
    }

    if dest != expr::Ignore {
        let block_ty = node_id_type(bcx, b.id);

        if b.expr.is_none() || type_is_zero_size(bcx.ccx(), block_ty) {
            dest = expr::Ignore;
        } else if b.expr.is_some() {
            // If the block has an expression, but that expression isn't reachable,
            // don't save into the destination given, ignore it.
            if let Some(ref cfg) = bcx.fcx.cfg {
                if !cfg.node_is_reachable(b.expr.as_ref().unwrap().id) {
                    dest = expr::Ignore;
                }
            }
        }
    }

    match b.expr {
        Some(ref e) => {
            if !bcx.bl.unreachable.get() {
                bcx.bl = expr::trans_into(bcx, &**e, dest);
            }
        }
        None => {
            assert!(dest == expr::Ignore || bcx.bl.unreachable.get());
        }
    }

    bcx.fcx.pop_and_trans_ast_cleanup_scope(bcx.bl, b.id)
}

pub fn trans_if<'r, 'blk, 'tcx>(&mut Block { bl, ref mut fcx }: &mut Block<'r, 'blk, 'tcx>,
                                if_id: ast::NodeId,
                                cond: &ast::Expr,
                                thn: &ast::Block,
                                els: Option<&ast::Expr>,
                                dest: expr::Dest)
                                -> &'blk BlockS {
    let mut bcx = &mut bl.with(fcx);

    debug!("trans_if(bcx={}, if_id={}, cond={}, thn={}, dest={})",
           bcx.to_str(), if_id, bcx.expr_to_string(cond), thn.id,
           dest.to_string(bcx.ccx()));
    let _icx = push_ctxt("trans_if");

    if bcx.bl.unreachable.get() {
        return bcx.bl;
    }

    let cond_val = unpack_result!(bcx, expr::trans(bcx, cond).to_llbool(bcx.fcx));

    // Drop branches that are known to be impossible
    if is_const(cond_val) && !is_undef(cond_val) {
        if const_to_uint(cond_val) == 1 {
            match els {
                Some(elexpr) => {
                    let mut trans = TransItemVisitor { ccx: bcx.fcx.ccx };
                    trans.visit_expr(&*elexpr);
                }
                None => {}
            }
            // if true { .. } [else { .. }]
            bcx.bl = trans_block(bcx, &*thn, dest);
            trans::debuginfo::clear_source_location(bcx.fcx);
        } else {
            let mut trans = TransItemVisitor { ccx: bcx.fcx.ccx } ;
            trans.visit_block(&*thn);

            match els {
                // if false { .. } else { .. }
                Some(elexpr) => {
                    bcx.bl = expr::trans_into(bcx, &*elexpr, dest);
                    trans::debuginfo::clear_source_location(bcx.fcx);
                }

                // if false { .. }
                None => { }
            }
        }

        return bcx.bl;
    }

    let name = format!("then-block-{}-", thn.id);
    let then_bcx_in = bcx.fcx.new_id_block(&name[..], thn.id);
    let then_bcx_out = trans_block(&mut then_bcx_in.with(bcx.fcx), &*thn, dest);
    trans::debuginfo::clear_source_location(bcx.fcx);

    let cond_source_loc = cond.debug_loc();

    let next_bcx;
    match els {
        Some(elexpr) => {
            let else_bcx_in = bcx.fcx.new_id_block("else-block", elexpr.id);
            let else_bcx_out = expr::trans_into(&mut else_bcx_in.with(bcx.fcx), &*elexpr, dest);
            next_bcx = bcx.fcx.join_blocks(if_id,
                                           &[then_bcx_out, else_bcx_out]);
            CondBr(&mut bcx, cond_val,
                   then_bcx_in.llbb, else_bcx_in.llbb, cond_source_loc);
        }

        None => {
            next_bcx = bcx.fcx.new_id_block("next-block", if_id);
            Br(&mut then_bcx_out.with(bcx.fcx), next_bcx.llbb, DebugLoc::None);
            CondBr(bcx, cond_val, then_bcx_in.llbb, next_bcx.llbb, cond_source_loc);
        }
    }

    // Clear the source location because it is still set to whatever has been translated
    // right before.
    trans::debuginfo::clear_source_location(bcx.fcx);

    next_bcx
}

pub fn trans_while<'r, 'blk, 'tcx>(bcx: &mut Block<'r, 'blk, 'tcx>,
                                   loop_expr: &ast::Expr,
                                   cond: &ast::Expr,
                                   body: &ast::Block)
                                   -> &'blk BlockS {
    let _icx = push_ctxt("trans_while");

    if bcx.bl.unreachable.get() {
        return bcx.bl;
    }

    //            bcx
    //             |
    //         cond_bcx_in  <--------+
    //             |                 |
    //         cond_bcx_out          |
    //           |      |            |
    //           |    body_bcx_in    |
    // cleanup_blk      |            |
    //    |           body_bcx_out --+
    // next_bcx_in

    let next_bcx_in = bcx.fcx.new_id_block("while_exit", loop_expr.id);
    let cond_bcx_in = bcx.fcx.new_id_block("while_cond", cond.id);
    let body_bcx_in = bcx.fcx.new_id_block("while_body", body.id);

    bcx.fcx.push_loop_cleanup_scope(loop_expr.id, [next_bcx_in, cond_bcx_in]);

    Br(bcx, cond_bcx_in.llbb, loop_expr.debug_loc());

    // compile the block where we will handle loop cleanups
    let cleanup_llbb = bcx.fcx.normal_exit_block(loop_expr.id, cleanup::EXIT_BREAK);

    // compile the condition
    let Result {bcx: cond_bcx_out, val: cond_val} =
        expr::trans(&mut cond_bcx_in.with(bcx.fcx), cond).to_llbool(bcx.fcx);

    CondBr(&mut cond_bcx_out.with(bcx.fcx), cond_val,
           body_bcx_in.llbb, cleanup_llbb, cond.debug_loc());

    // loop body:
    let body_bcx_out = trans_block(&mut body_bcx_in.with(bcx.fcx), body, expr::Ignore);
    Br(&mut body_bcx_out.with(bcx.fcx), cond_bcx_in.llbb, DebugLoc::None);

    bcx.fcx.pop_loop_cleanup_scope(loop_expr.id);
    return next_bcx_in;
}

pub fn trans_loop<'r, 'blk, 'tcx>(bcx: &mut Block<'r, 'blk, 'tcx>,
                                  loop_expr: &ast::Expr,
                                  body: &ast::Block)
                                  -> &'blk BlockS {
    let _icx = push_ctxt("trans_loop");

    if bcx.bl.unreachable.get() {
        return bcx.bl;
    }

    //            bcx
    //             |
    //         body_bcx_in
    //             |
    //         body_bcx_out
    //
    // next_bcx
    //
    // Links between body_bcx_in and next_bcx are created by
    // break statements.

    let next_bcx_in = bcx.fcx.new_id_block("loop_exit", loop_expr.id);
    let body_bcx_in = bcx.fcx.new_id_block("loop_body", body.id);

    bcx.fcx.push_loop_cleanup_scope(loop_expr.id, [next_bcx_in, body_bcx_in]);

    Br(bcx, body_bcx_in.llbb, loop_expr.debug_loc());
    let body_bcx_out = trans_block(&mut body_bcx_in.with(bcx.fcx), body, expr::Ignore);
    Br(&mut body_bcx_out.with(bcx.fcx), body_bcx_in.llbb, DebugLoc::None);

    bcx.fcx.pop_loop_cleanup_scope(loop_expr.id);

    // If there are no predecessors for the next block, we just translated an endless loop and the
    // next block is unreachable
    if BasicBlock(next_bcx_in.llbb).pred_iter().next().is_none() {
        Unreachable(&mut next_bcx_in.with(bcx.fcx));
    }

    return next_bcx_in;
}

pub fn trans_break_cont<'r, 'blk, 'tcx>(bcx: &mut Block<'r, 'blk, 'tcx>,
                                        expr: &ast::Expr,
                                        opt_label: Option<ast::Ident>,
                                        exit: usize)
                                        -> &'blk BlockS {
    let _icx = push_ctxt("trans_break_cont");

    if bcx.bl.unreachable.get() {
        return bcx.bl;
    }

    // Locate loop that we will break to
    let loop_id = match opt_label {
        None => bcx.fcx.top_loop_scope(),
        Some(_) => {
            match bcx.tcx().def_map.borrow().get(&expr.id).map(|d| d.full_def())  {
                Some(def::DefLabel(loop_id)) => loop_id,
                r => {
                    bcx.tcx().sess.bug(&format!("{:?} in def-map for label", r))
                }
            }
        }
    };

    // Generate appropriate cleanup code and branch
    let cleanup_llbb = bcx.fcx.normal_exit_block(loop_id, exit);
    Br(bcx, cleanup_llbb, expr.debug_loc());
    Unreachable(bcx); // anything afterwards should be ignored
    bcx.bl
}

pub fn trans_break<'r, 'blk, 'tcx>(bcx: &mut Block<'r, 'blk, 'tcx>,
                                   expr: &ast::Expr,
                                   label_opt: Option<ast::Ident>)
                                   -> &'blk BlockS {
    return trans_break_cont(bcx, expr, label_opt, cleanup::EXIT_BREAK);
}

pub fn trans_cont<'r, 'blk, 'tcx>(bcx: &mut Block<'r, 'blk, 'tcx>,
                                  expr: &ast::Expr,
                                  label_opt: Option<ast::Ident>)
                                  -> &'blk BlockS {
    return trans_break_cont(bcx, expr, label_opt, cleanup::EXIT_LOOP);
}

pub fn trans_ret<'r, 'blk, 'tcx>(&mut Block { bl, ref mut fcx }: &mut Block<'r, 'blk, 'tcx>,
                                 return_expr: &ast::Expr,
                                 retval_expr: Option<&ast::Expr>)
                                 -> &'blk BlockS {
    let _icx = push_ctxt("trans_ret");

    if bl.unreachable.get() {
        return bl;
    }

    let mut bcx = &mut bl.with(fcx);
    let dest = match (bcx.fcx.llretslotptr, retval_expr) {
        (Some(_), Some(retval_expr)) => {
            let ret_ty = expr_ty_adjusted(bcx, &*retval_expr);
            expr::SaveIn(bcx.fcx.get_ret_slot(bcx.bl, ty::FnConverging(ret_ty), "ret_slot"))
        }
        _ => expr::Ignore,
    };
    if let Some(x) = retval_expr {
        bcx.bl = expr::trans_into(bcx, &*x, dest);
        match dest {
            expr::SaveIn(slot) if bcx.fcx.needs_ret_allocas => {
                let p = bcx.fcx.llretslotptr.unwrap();
                Store(bcx, slot, p);
            }
            _ => {}
        }
    }
    let cleanup_llbb = bcx.fcx.return_exit_block();
    Br(bcx, cleanup_llbb, return_expr.debug_loc());
    Unreachable(bcx);
    bcx.bl
}

pub fn trans_fail<'r, 'blk, 'tcx>(bcx: &mut Block<'r, 'blk, 'tcx>,
                                  call_info: NodeIdAndSpan,
                                  fail_str: InternedString)
                                  -> &'blk BlockS {
    let ccx = bcx.ccx();
    let _icx = push_ctxt("trans_fail_value");

    if bcx.bl.unreachable.get() {
        return bcx.bl;
    }

    let v_str = C_str_slice(ccx, fail_str);
    let loc = bcx.sess().codemap().lookup_char_pos(call_info.span.lo);
    let filename = token::intern_and_get_ident(&loc.file.name);
    let filename = C_str_slice(ccx, filename);
    let line = C_u32(ccx, loc.line as u32);
    let expr_file_line_const = C_struct(ccx, &[v_str, filename, line], false);
    let expr_file_line = consts::addr_of(ccx, expr_file_line_const, "panic_loc");
    let args = vec!(expr_file_line);
    let did = langcall(bcx, Some(call_info.span), "", PanicFnLangItem);
    let bl = callee::trans_lang_call(bcx,
                                     did,
                                     &args[..],
                                     Some(expr::Ignore),
                                     call_info.debug_loc()).bcx;
    Unreachable(&mut bl.with(bcx.fcx));
    bl
}

pub fn trans_fail_bounds_check<'r, 'blk, 'tcx>(bcx: &mut Block<'r, 'blk, 'tcx>,
                                               call_info: NodeIdAndSpan,
                                               index: ValueRef,
                                               len: ValueRef)
                                               -> &'blk BlockS {
    let ccx = bcx.ccx();
    let _icx = push_ctxt("trans_fail_bounds_check");

    if bcx.bl.unreachable.get() {
        return bcx.bl;
    }

    // Extract the file/line from the span
    let loc = bcx.sess().codemap().lookup_char_pos(call_info.span.lo);
    let filename = token::intern_and_get_ident(&loc.file.name);

    // Invoke the lang item
    let filename = C_str_slice(ccx,  filename);
    let line = C_u32(ccx, loc.line as u32);
    let file_line_const = C_struct(ccx, &[filename, line], false);
    let file_line = consts::addr_of(ccx, file_line_const, "panic_bounds_check_loc");
    let args = vec!(file_line, index, len);
    let did = langcall(bcx, Some(call_info.span), "", PanicBoundsCheckFnLangItem);
    let bl = callee::trans_lang_call(bcx,
                                     did,
                                     &args[..],
                                     Some(expr::Ignore),
                                     call_info.debug_loc()).bcx;
    Unreachable(&mut bl.with(bcx.fcx));
    bl
}
