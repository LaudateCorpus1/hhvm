// Copyright (c) 2019, Facebook, Inc.
// All rights reserved.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the "hack" directory of this source tree.

use naming_special_names_rust::special_idents;
use oxidized::{
    aast,
    aast_visitor::{visit_mut, AstParams, NodeMut, VisitorMut},
    ast::*,
    local_id,
    pos::Pos,
};
use parser_core_types::{
    syntax_error,
    syntax_error::{Error as ErrorMsg, SyntaxError},
};
use std::collections::HashMap;

#[derive(PartialEq, Copy, Clone)]
pub enum Rty {
    Readonly,
    Mutable,
}

struct Context {
    locals: HashMap<String, Rty>,
    readonly_return: Rty,
    this_ty: Rty,
}

impl Context {
    fn new(readonly_ret: Rty, this_ty: Rty) -> Self {
        Self {
            locals: HashMap::new(),
            readonly_return: readonly_ret,
            this_ty,
        }
    }

    pub fn add_local(&mut self, var_name: &String, rty: Rty) {
        match self.locals.get(var_name) {
            Some(_) => {
                // per @jjwu, "...once a variable is assigned a value, it
                // can only be assigned a value of the same Rty
                // for the rest of the function." See D29320968 for more context.
            }
            None => {
                self.locals.insert(var_name.clone(), rty);
            }
        }
    }

    pub fn is_new_local(&self, var_name: &String) -> bool {
        !self.locals.contains_key(var_name)
    }

    pub fn get_rty<S: ?Sized>(&self, var_name: &S) -> Rty
    where
        String: std::borrow::Borrow<S>,
        S: std::hash::Hash + Eq,
    {
        match self.locals.get(var_name) {
            Some(&x) => x,
            None => Rty::Mutable,
        }
    }
}

fn ro_expr_list(context: &mut Context, exprs: &Vec<Expr>) -> Rty {
    if exprs.iter().any(|e| rty_expr(context, &e) == Rty::Readonly) {
        Rty::Readonly
    } else {
        Rty::Mutable
    }
}

fn ro_expr_list2<T>(context: &mut Context, exprs: &Vec<(T, Expr)>) -> Rty {
    if exprs
        .iter()
        .any(|e| rty_expr(context, &e.1) == Rty::Readonly)
    {
        Rty::Readonly
    } else {
        Rty::Mutable
    }
}

fn ro_kind_to_rty(ro: Option<oxidized::ast_defs::ReadonlyKind>) -> Rty {
    match ro {
        Some(oxidized::ast_defs::ReadonlyKind::Readonly) => Rty::Readonly,
        _ => Rty::Mutable,
    }
}

fn rty_expr(context: &mut Context, expr: &Expr) -> Rty {
    let aast::Expr(_, _, exp) = &*expr;
    use aast::Expr_::*;
    match exp {
        ReadonlyExpr(_) => Rty::Readonly,
        ObjGet(og) => {
            let (obj, _member_name, _null_flavor, _reffiness) = &**og;
            rty_expr(context, &obj)
        }
        Lvar(id_orig) => {
            let var_name = local_id::get_name(&id_orig.1);
            let is_this = var_name == special_idents::THIS;
            if is_this {
                context.this_ty
            } else {
                context.get_rty(var_name)
            }
        }
        Darray(d) => {
            let (_, exprs) = &**d;
            ro_expr_list2(context, exprs)
        }
        Varray(v) => {
            let (_, exprs) = &**v;
            ro_expr_list(context, exprs)
        }
        Shape(fields) => ro_expr_list2(context, fields),
        ValCollection(v) => {
            let (_, _, exprs) = &**v;
            ro_expr_list(context, exprs)
        }
        KeyValCollection(kv) => {
            let (_, _, fields) = &**kv;
            if fields
                .iter()
                .any(|f| rty_expr(context, &f.1) == Rty::Readonly)
            {
                Rty::Readonly
            } else {
                Rty::Mutable
            }
        }
        Collection(c) => {
            let (_, _, fields) = &**c;
            if fields.iter().any(|f| match f {
                aast::Afield::AFvalue(e) => rty_expr(context, &e) == Rty::Readonly,
                aast::Afield::AFkvalue(_, e) => rty_expr(context, &e) == Rty::Readonly,
            }) {
                Rty::Readonly
            } else {
                Rty::Mutable
            }
        }
        Record(r) => {
            let (_, fields) = &**r;
            ro_expr_list2(context, &fields)
        }
        Xml(_) | Efun(_) | Lfun(_) => Rty::Mutable,
        Callconv(c) => {
            let (_, expr) = &**c;
            rty_expr(context, &expr)
        }
        Tuple(t) => ro_expr_list(context, t),
        // Only list destructuring
        List(_) => Rty::Mutable,
        // Boolean statement always mutable
        Is(_) => Rty::Mutable,
        //
        As(a) => {
            // Readonlyness of inner expression
            let (exp, _, _) = &**a;
            rty_expr(context, &exp)
        }
        Eif(e) => {
            // $x ? a : b is readonly if either a or b are readonly
            let (_, exp1_opt, exp2) = &**e;
            if let Some(exp1) = exp1_opt {
                match (rty_expr(context, exp1), rty_expr(context, exp2)) {
                    (_, Rty::Readonly) | (Rty::Readonly, _) => Rty::Readonly,
                    (Rty::Mutable, Rty::Mutable) => Rty::Mutable,
                }
            } else {
                rty_expr(context, &exp2)
            }
        }
        Pair(p) => {
            let (_, exp1, exp2) = &**p;
            match (rty_expr(context, exp1), rty_expr(context, exp2)) {
                (_, Rty::Readonly) | (Rty::Readonly, _) => Rty::Readonly,
                (Rty::Mutable, Rty::Mutable) => Rty::Mutable,
            }
        }
        Hole(h) => {
            let (expr, _, _, _) = &**h;
            rty_expr(context, &expr)
        }
        Cast(_) => Rty::Mutable, // Casts are only valid on primitive types, so its always mutable
        New(_) => Rty::Mutable,
        // FWIW, this does not appear on the aast at this stage(it only appears after naming in typechecker),
        // but we can handle it for future in case that changes
        This => context.this_ty,
        ArrayGet(ag) => {
            let (expr, _) = &**ag;
            rty_expr(context, &expr)
        }
        Await(expr) => {
            let expr = &**expr;
            rty_expr(context, &expr)
        }
        // Primitive types are mutable
        Null | True | False | Omitted => Rty::Mutable,
        Int(_) | Float(_) | String(_) | String2(_) | PrefixedString(_) => Rty::Mutable,
        Id(_) => Rty::Mutable,
        // TODO: Need to handle dollardollar with pipe expressions correctly
        Dollardollar(_) => Rty::Mutable,
        Clone(_) => Rty::Mutable,
        // Mutable unless wrapped in a readonly expression
        Call(_) | ClassGet(_) | ClassConst(_) => Rty::Mutable,
        FunctionPointer(_) => Rty::Mutable,
        // This is really just a statement, does not have a value
        Yield(_) => Rty::Mutable,
        // Operators are all primitive in result
        Unop(_) | Binop(_) => Rty::Mutable,
        // TODO: track the left side of pipe expressions' readonlyness for $$
        Pipe(_) => Rty::Mutable,
        ExpressionTree(_) | EnumClassLabel(_) | ETSplice(_) => Rty::Mutable,
        Import(_) | Lplaceholder(_) => Rty::Mutable,
        // More function values which are always mutable
        MethodId(_) | MethodCaller(_) | SmethodId(_) | FunId(_) => Rty::Mutable,
    }
}

fn explicit_readonly(expr: &mut Expr) {
    match &expr.2 {
        aast::Expr_::ReadonlyExpr(_) => {}
        _ => {
            expr.2 = aast::Expr_::ReadonlyExpr(Box::new(expr.clone()));
        }
    }
}

// For assignments to local variables, i.e.
// $x = new Foo();
fn check_assignment_local(
    context: &mut Context,
    checker: &mut Checker,
    pos: &Pos,
    id_orig: &Box<Lid>,
    rhs: &mut Expr,
) {
    let var_name = local_id::get_name(&id_orig.1).to_string();
    let rhs_rty = rty_expr(context, &rhs);
    if context.is_new_local(&var_name) {
        context.add_local(&var_name, rhs_rty);
    } else if context.get_rty(&var_name) != rhs_rty {
        checker.add_error(
            &pos,
            syntax_error::redefined_assignment_different_mutability(&var_name),
        );
    }
}

// For assignments to nonlocals, i.e.
// $x->prop[0] = new Foo();
fn check_assignment_nonlocal(
    context: &mut Context,
    checker: &mut Checker,
    pos: &Pos,
    lhs: &mut Expr,
    rhs: &mut Expr,
) {
    match &mut lhs.2 {
        aast::Expr_::ObjGet(o) => {
            let (obj, _get, _, _) = &**o;
            // If obj is readonly, throw error
            match rty_expr(context, &obj) {
                Rty::Readonly => {
                    checker.add_error(&pos, syntax_error::assignment_to_readonly);
                }
                Rty::Mutable => {
                    match rty_expr(context, &rhs) {
                        Rty::Readonly => {
                            // make the readonly expression explicit, since if it is readonly we need to make sure the property is a readonly prop
                            explicit_readonly(rhs);
                        }
                        // Mutable case does not require special checks
                        Rty::Mutable => {}
                    }
                }
            }
        }
        // On an array get <expr>[0] = <rhs>, recurse and check compatibility of inner <expr> with <rhs>
        aast::Expr_::ArrayGet(ag) => {
            let (array, _) = &mut **ag;
            check_assignment_nonlocal(context, checker, pos, array, rhs);
        }
        _ => {
            // Base case: here we just check whether the lhs expression is readonly compared to the rhs
            match (rty_expr(context, &lhs), rty_expr(context, &rhs)) {
                (Rty::Mutable, Rty::Readonly) => {
                    // error, can't assign a readonly value to a mutable collection
                    checker.add_error(
                        &rhs.1, // Position of readonly expression
                        syntax_error::assign_readonly_to_mutable_collection,
                    )
                }
                (Rty::Readonly, Rty::Readonly) => {
                    // make rhs explicit (to make sure we are not writing a readonly value to a mutable one)
                    explicit_readonly(rhs);
                }
                (_, Rty::Mutable) => {
                    // Assigning to a mutable value always succeeds, so no explicit checks are needed
                }
            }
        }
    }
}

// Toplevel assignment check
fn check_assignment_validity(
    context: &mut Context,
    checker: &mut Checker,
    pos: &Pos,
    lhs: &mut Expr,
    rhs: &mut Expr,
) {
    match &mut lhs.2 {
        aast::Expr_::Lvar(id_orig) => {
            check_assignment_local(context, checker, pos, id_orig, rhs);
        }
        // list assignment
        aast::Expr_::List(l) => {
            let exprs = &mut **l;
            for e in exprs.iter_mut() {
                check_assignment_validity(context, checker, &e.1.clone(), e, rhs);
            }
        }
        _ => {
            check_assignment_nonlocal(context, checker, pos, lhs, rhs);
        }
    }
}

struct Checker {
    errors: Vec<SyntaxError>,
}

impl Checker {
    fn new() -> Self {
        Self { errors: vec![] }
    }

    fn add_error(&mut self, pos: &Pos, msg: ErrorMsg) {
        let (start_offset, end_offset) = pos.info_raw();
        self.errors
            .push(SyntaxError::make(start_offset, end_offset, msg));
    }

    fn subtype(&mut self, pos: &Pos, r_sub: &Rty, r_sup: &Rty, reason: &str) {
        use Rty::*;
        match (r_sub, r_sup) {
            (Readonly, Mutable) => self.add_error(
                &pos,
                syntax_error::invalid_readonly("readonly", "mutable", &reason),
            ),
            _ => {}
        }
    }
}

impl<'ast> VisitorMut<'ast> for Checker {
    type P = AstParams<Context, ()>;

    fn object(&mut self) -> &mut dyn VisitorMut<'ast, P = Self::P> {
        self
    }

    fn visit_method_(
        &mut self,
        _context: &mut Context,
        m: &mut aast::Method_<(), ()>,
    ) -> Result<(), ()> {
        let readonly_return = ro_kind_to_rty(m.readonly_ret);
        let readonly_this = if m.readonly_this {
            Rty::Readonly
        } else {
            Rty::Mutable
        };
        let mut context = Context::new(readonly_return, readonly_this);

        for p in m.params.iter() {
            if let Some(_) = p.readonly {
                context.add_local(&p.name, Rty::Readonly)
            } else {
                context.add_local(&p.name, Rty::Mutable)
            }
        }
        m.recurse(&mut context, self.object())
    }

    fn visit_fun_(&mut self, _context: &mut Context, f: &mut aast::Fun_<(), ()>) -> Result<(), ()> {
        let readonly_return = ro_kind_to_rty(f.readonly_ret);
        let readonly_this = ro_kind_to_rty(f.readonly_this);
        let mut context = Context::new(readonly_return, readonly_this);

        for p in f.params.iter() {
            if let Some(_) = p.readonly {
                context.add_local(&p.name, Rty::Readonly)
            } else {
                context.add_local(&p.name, Rty::Mutable)
            }
        }
        f.recurse(&mut context, self.object())
    }

    fn visit_expr(&mut self, context: &mut Context, p: &mut aast::Expr<(), ()>) -> Result<(), ()> {
        match &mut p.2 {
            aast::Expr_::Binop(x) => {
                let (bop, e_lhs, e_rhs) = x.as_mut();
                if let Bop::Eq(_) = bop {
                    check_assignment_validity(context, self, &p.1, e_lhs, e_rhs);
                }
            }
            aast::Expr_::Call(x) => {
                let (_caller, _targs, params, _variadic) = &mut **x;
                for param in params.iter_mut() {
                    match rty_expr(context, param) {
                        Rty::Readonly => explicit_readonly(param),
                        Rty::Mutable => {}
                    }
                }
            }
            _ => {}
        }


        p.recurse(context, self.object())
    }

    fn visit_stmt(
        &mut self,
        context: &mut Context,
        s: &mut aast::Stmt<(), ()>,
    ) -> std::result::Result<(), ()> {
        if let aast::Stmt_::Return(r) = &mut s.1 {
            if let Some(expr) = r.as_mut() {
                self.subtype(&expr.1, &rty_expr(context, &expr), &context.readonly_return, "this function does not return readonly. Please mark it to return readonly if needed.")
            }
        }
        s.recurse(context, self.object())
    }
}

pub fn check_program(program: &mut aast::Program<(), ()>) -> Vec<SyntaxError> {
    let mut checker = Checker::new();
    let mut context = Context::new(Rty::Mutable, Rty::Mutable);
    visit_mut(&mut checker, &mut context, program).unwrap();
    checker.errors
}
