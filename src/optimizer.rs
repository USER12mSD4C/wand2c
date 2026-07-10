#![allow(dead_code)]

use crate::ast::*;

pub struct Optimizer;

impl Optimizer {
    pub fn optimize_program(program: &mut Program) -> usize {
        let mut count = 0;
        for func in &mut program.functions {
            if let Some(body) = &mut func.body {
                for stmt in body {
                    count += Self::optimize_stmt(stmt);
                }
            }
        }
        count
    }

    fn optimize_stmt(stmt: &mut Stmt) -> usize {
        let mut count = 0;
        match stmt {
            Stmt::VarDefinition(decl) => {
                if let Some(ref mut init_expr) = decl.initial_value {
                    count += Self::optimize_expr(init_expr);
                }
            }
            Stmt::Assignment { targets, value } => {
                for target in targets {
                    count += Self::optimize_expr(target);
                }
                count += Self::optimize_expr(value);
            }
            Stmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                count += Self::optimize_expr(cond);
                for s in then_branch {
                    count += Self::optimize_stmt(s);
                }
                if let Some(else_stmts) = else_branch {
                    for s in else_stmts {
                        count += Self::optimize_stmt(s);
                    }
                }
            }
            Stmt::While { cond, body } => {
                count += Self::optimize_expr(cond);
                for s in body {
                    count += Self::optimize_stmt(s);
                }
            }
            Stmt::For {
                init,
                cond,
                post,
                body,
            } => {
                if let Some(ref mut i) = init {
                    count += Self::optimize_stmt(i);
                }
                count += Self::optimize_expr(cond);
                if let Some(ref mut p) = post {
                    count += Self::optimize_stmt(p);
                }
                for s in body {
                    count += Self::optimize_stmt(s);
                }
            }
            Stmt::Return(values) => {
                for (_, expr) in values {
                    count += Self::optimize_expr(expr);
                }
            }
            Stmt::Expr(expr) => {
                count += Self::optimize_expr(expr);
            }
            _ => {}
        }
        count
    }

    fn optimize_expr(expr: &mut Expr) -> usize {
        let mut count = 0;
        match expr {
            Expr::Binary { left, op, right } => {
                count += Self::optimize_expr(left);
                count += Self::optimize_expr(right);

                // Оптимизация алгебраических выражений (убирает лишние регистровые/стековые команды)
                if op == "OpAdd" {
                    if let Expr::Number(0) = &**left {
                        *expr = *right.clone();
                        return count + 1;
                    }
                    if let Expr::Number(0) = &**right {
                        *expr = *left.clone();
                        return count + 1;
                    }
                }
                if op == "OpSub" {
                    if let Expr::Number(0) = &**right {
                        *expr = *left.clone();
                        return count + 1;
                    }
                }
                if op == "OpMul" {
                    if let Expr::Number(1) = &**left {
                        *expr = *right.clone();
                        return count + 1;
                    }
                    if let Expr::Number(1) = &**right {
                        *expr = *left.clone();
                        return count + 1;
                    }
                    if let Expr::Number(0) = &**left {
                        *expr = Expr::Number(0);
                        return count + 1;
                    }
                    if let Expr::Number(0) = &**right {
                        *expr = Expr::Number(0);
                        return count + 1;
                    }
                }

                // Сворачивание числовых констант
                if let (Expr::Number(a), Expr::Number(b)) = (&**left, &**right) {
                    let val_a = *a;
                    let val_b = *b;
                    let folded = match op.as_str() {
                        "OpAdd" => Some(val_a.wrapping_add(val_b)),
                        "OpSub" => Some(val_a.wrapping_sub(val_b)),
                        "OpMul" => Some(val_a.wrapping_mul(val_b)),
                        _ => None,
                    };
                    if let Some(f_val) = folded {
                        *expr = Expr::Number(f_val);
                        count += 1;
                    }
                }
            }
            Expr::MemberAccess {
                expr: base_expr, ..
            } => {
                count += Self::optimize_expr(base_expr);
            }
            Expr::Index {
                expr: base_expr,
                index,
            } => {
                count += Self::optimize_expr(base_expr);
                count += Self::optimize_expr(index);
            }
            Expr::Call { args, .. } => {
                for arg in args {
                    Self::optimize_expr(arg);
                }
            }
            _ => {}
        }
        count
    }
}
