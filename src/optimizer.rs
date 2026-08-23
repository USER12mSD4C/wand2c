#![allow(dead_code)]

use crate::ast::*;
use std::collections::{HashMap, HashSet};

pub struct Optimizer;

impl Optimizer {
    pub fn optimize_program(program: &mut Program) -> usize {
        let mut total_optimized = 0;
        for func in &mut program.functions {
            if let Some(ref mut body) = func.body {
                let mut iteration = 0;
                loop {
                    let mut count = 0;
                    let mut escaped = HashSet::new();
                    collect_escaped(body, &mut escaped);

                    let mut reads = HashMap::new();
                    collect_reads(body, &mut reads);

                    let mut output_ptrs = HashSet::new();
                    for (_, name, access) in &func.params {
                        if *access == PtrAccess::Output
                            || *access == PtrAccess::InputOutput
                            || *access == PtrAccess::Volatile
                            || *access == PtrAccess::Atomic
                        {
                            output_ptrs.insert(name.clone());
                        }
                    }

                    let mut consts = HashMap::new();
                    let old_body = std::mem::take(body);
                    let new_body = optimize_statements(
                        old_body,
                        &mut consts,
                        &escaped,
                        &reads,
                        &mut output_ptrs,
                        &mut count,
                    );
                    *body = new_body;

                    total_optimized += count;
                    iteration += 1;
                    if count == 0 || iteration >= 10 {
                        break;
                    }
                }
            }
        }
        total_optimized
    }
}

fn collect_escaped(body: &[Stmt], escaped: &mut HashSet<String>) {
    for stmt in body {
        collect_escaped_stmt(stmt, escaped);
    }
}

fn collect_escaped_stmt(stmt: &Stmt, escaped: &mut HashSet<String>) {
    match stmt {
        Stmt::VarDefinition(decl) => {
            if let Some(ref init) = decl.initial_value {
                collect_escaped_expr(init, escaped);
            }
        }
        Stmt::Assignment { targets, value } => {
            for target in targets {
                collect_escaped_expr(target, escaped);
            }
            collect_escaped_expr(value, escaped);
        }
        Stmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            collect_escaped_expr(cond, escaped);
            collect_escaped(then_branch, escaped);
            if let Some(else_stmts) = else_branch {
                collect_escaped(else_stmts, escaped);
            }
        }
        Stmt::While { cond, body } => {
            collect_escaped_expr(cond, escaped);
            collect_escaped(body, escaped);
        }
        Stmt::For {
            init,
            cond,
            post,
            body,
        } => {
            if let Some(ref i) = init {
                collect_escaped_stmt(i, escaped);
            }
            collect_escaped_expr(cond, escaped);
            if let Some(ref p) = post {
                collect_escaped_stmt(p, escaped);
            }
            collect_escaped(body, escaped);
        }
        Stmt::Return(values) => {
            for (_, expr) in values {
                collect_escaped_expr(expr, escaped);
            }
        }
        Stmt::Jmpto { args, .. } => {
            for arg in args {
                collect_escaped_stmt(arg, escaped);
            }
        }
        Stmt::Expr(expr) => {
            collect_escaped_expr(expr, escaped);
        }
        Stmt::Match {
            expr,
            cases,
            default,
        } => {
            collect_escaped_expr(expr, escaped);
            for (ce, body) in cases {
                collect_escaped_expr(ce, escaped);
                collect_escaped(body, escaped);
            }
            if let Some(d) = default {
                collect_escaped(d, escaped);
            }
        }
        Stmt::Critical(body) => {
            collect_escaped(body, escaped);
        }
        _ => {}
    }
}

fn collect_escaped_expr(expr: &Expr, escaped: &mut HashSet<String>) {
    match expr {
        Expr::AddrOf(name) => {
            let root = if let Some(idx) = name.find(':') {
                &name[idx + 1..]
            } else {
                name.as_str()
            };
            escaped.insert(root.to_string());
        }
        Expr::Binary { left, right, .. } => {
            collect_escaped_expr(left, escaped);
            collect_escaped_expr(right, escaped);
        }
        Expr::Call { args, .. } => {
            for arg in args {
                collect_escaped_expr(arg, escaped);
            }
        }
        Expr::Index { expr: base, index } => {
            collect_escaped_expr(base, escaped);
            collect_escaped_expr(index, escaped);
        }
        Expr::MemberAccess { expr: base, .. } => {
            collect_escaped_expr(base, escaped);
        }
        Expr::AddrOfExpr(inner) => {
            collect_addr_target_roots(inner, escaped);
            collect_escaped_expr(inner, escaped);
        }
        _ => {}
    }
}

fn collect_addr_target_roots(expr: &Expr, escaped: &mut HashSet<String>) {
    match expr {
        Expr::Variable(name) => {
            escaped.insert(name.clone());
        }

        Expr::MemberAccess { expr: base, .. } => {
            collect_addr_target_roots(base, escaped);
        }

        Expr::Index { expr: base, index } => {
            collect_addr_target_roots(base, escaped);
            collect_addr_target_roots(index, escaped);
        }

        Expr::Binary { left, right, .. } => {
            collect_addr_target_roots(left, escaped);
            collect_addr_target_roots(right, escaped);
        }

        Expr::Call { args, .. } => {
            for arg in args {
                collect_addr_target_roots(arg, escaped);
            }
        }

        _ => {}
    }
}

fn collect_reads(body: &[Stmt], reads: &mut HashMap<String, usize>) {
    for stmt in body {
        collect_reads_stmt(stmt, reads);
    }
}

fn collect_reads_stmt(stmt: &Stmt, reads: &mut HashMap<String, usize>) {
    match stmt {
        Stmt::VarDefinition(decl) => {
            if let Some(ref init) = decl.initial_value {
                collect_reads_expr(init, reads);
            }
        }
        Stmt::Assignment { targets, value } => {
            for target in targets {
                collect_reads_lhs(target, reads);
            }
            collect_reads_expr(value, reads);
        }
        Stmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            collect_reads_expr(cond, reads);
            collect_reads(then_branch, reads);
            if let Some(else_stmts) = else_branch {
                collect_reads(else_stmts, reads);
            }
        }
        Stmt::While { cond, body } => {
            collect_reads_expr(cond, reads);
            collect_reads(body, reads);
        }
        Stmt::For {
            init,
            cond,
            post,
            body,
        } => {
            if let Some(ref i) = init {
                collect_reads_stmt(i, reads);
            }
            collect_reads_expr(cond, reads);
            if let Some(ref p) = post {
                collect_reads_stmt(p, reads);
            }
            collect_reads(body, reads);
        }
        Stmt::Return(values) => {
            for (_, expr) in values {
                collect_reads_expr(expr, reads);
            }
        }
        Stmt::Jmpto { args, .. } => {
            for arg in args {
                collect_reads_stmt(arg, reads);
            }
        }
        Stmt::Expr(expr) => {
            collect_reads_expr(expr, reads);
        }
        Stmt::Match {
            expr,
            cases,
            default,
        } => {
            collect_reads_expr(expr, reads);
            for (ce, body) in cases {
                collect_reads_expr(ce, reads);
                collect_reads(body, reads);
            }
            if let Some(d) = default {
                collect_reads(d, reads);
            }
        }
        Stmt::Critical(body) => {
            collect_reads(body, reads);
        }
        _ => {}
    }
}

fn collect_reads_lhs(expr: &Expr, reads: &mut HashMap<String, usize>) {
    match expr {
        Expr::Variable(_) => {}
        Expr::MemberAccess { expr: base, .. } => {
            collect_reads_expr(base, reads);
        }
        Expr::Index { expr: base, index } => {
            collect_reads_expr(base, reads);
            collect_reads_expr(index, reads);
        }
        _ => {
            collect_reads_expr(expr, reads);
        }
    }
}

fn collect_reads_expr(expr: &Expr, reads: &mut HashMap<String, usize>) {
    match expr {
        Expr::Variable(name) => {
            *reads.entry(name.clone()).or_insert(0) += 1;
        }
        Expr::Binary { left, right, .. } => {
            collect_reads_expr(left, reads);
            collect_reads_expr(right, reads);
        }
        Expr::Call { args, .. } => {
            for arg in args {
                collect_reads_expr(arg, reads);
            }
        }
        Expr::Index { expr: base, index } => {
            collect_reads_expr(base, reads);
            collect_reads_expr(index, reads);
        }
        Expr::MemberAccess { expr: base, .. } => {
            collect_reads_expr(base, reads);
        }
        Expr::AddrOfExpr(inner) => {
            collect_reads_expr(inner, reads);
        }
        _ => {}
    }
}

fn get_expr_path(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Variable(name) => Some(name.clone()),
        Expr::MemberAccess {
            expr: base, member, ..
        } => {
            let base_path = get_expr_path(base)?;
            Some(format!("{}.{}", base_path, member))
        }
        Expr::Index { expr: base, index } => {
            let base_path = get_expr_path(base)?;
            if let Expr::Number(n) = &**index {
                Some(format!("{}[{}]", base_path, n))
            } else {
                None
            }
        }
        Expr::AddrOfExpr(inner) => get_expr_path(inner),
        _ => None,
    }
}

fn is_constant_expr(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Number(_)
            | Expr::SignedNumber(_)
            | Expr::FloatLit(_)
            | Expr::StringLit(_)
            | Expr::Null
    )
}

fn has_side_effects(expr: &Expr) -> bool {
    match expr {
        Expr::Call { .. } => true,
        Expr::Binary { left, right, .. } => has_side_effects(left) || has_side_effects(right),
        Expr::Index { expr, index } => has_side_effects(expr) || has_side_effects(index),
        Expr::MemberAccess { expr, .. } => has_side_effects(expr),
        Expr::AddrOfExpr(inner) => has_side_effects(inner),
        _ => false,
    }
}

fn is_power_of_two(val: u64) -> bool {
    val > 0 && (val & (val - 1)) == 0
}

fn is_same_expr(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        (Expr::Number(n1), Expr::Number(n2)) => n1 == n2,
        (Expr::SignedNumber(n1), Expr::SignedNumber(n2)) => n1 == n2,
        (Expr::FloatLit(s1), Expr::FloatLit(s2)) => s1 == s2,
        (Expr::StringLit(s1), Expr::StringLit(s2)) => s1 == s2,
        (Expr::Variable(v1), Expr::Variable(v2)) => v1 == v2,
        (Expr::AddrOf(v1), Expr::AddrOf(v2)) => v1 == v2,
        (
            Expr::SectionAccess {
                section: s1,
                variable: v1,
            },
            Expr::SectionAccess {
                section: s2,
                variable: v2,
            },
        ) => s1 == s2 && v1 == v2,
        (
            Expr::MemberAccess {
                expr: e1,
                member: m1,
                is_arrow: a1,
            },
            Expr::MemberAccess {
                expr: e2,
                member: m2,
                is_arrow: a2,
            },
        ) => is_same_expr(e1, e2) && m1 == m2 && a1 == a2,
        (
            Expr::Index {
                expr: e1,
                index: i1,
            },
            Expr::Index {
                expr: e2,
                index: i2,
            },
        ) => is_same_expr(e1, e2) && is_same_expr(i1, i2),
        (
            Expr::Binary {
                left: l1,
                op: op1,
                right: r1,
            },
            Expr::Binary {
                left: l2,
                op: op2,
                right: r2,
            },
        ) => op1 == op2 && is_same_expr(l1, l2) && is_same_expr(r1, r2),
        (
            Expr::Call {
                name: n1,
                args: args1,
            },
            Expr::Call {
                name: n2,
                args: args2,
            },
        ) => {
            if n1 != n2 || args1.len() != args2.len() {
                return false;
            }
            args1
                .iter()
                .zip(args2.iter())
                .all(|(x, y)| is_same_expr(x, y))
        }
        (Expr::AddrOfExpr(a), Expr::AddrOfExpr(b)) => is_same_expr(a, b),
        _ => false,
    }
}

fn collect_commutative_chain(expr: &Expr, target_op: &str, operands: &mut Vec<Expr>) {
    if let Expr::Binary { left, op, right } = expr {
        if op == target_op {
            collect_commutative_chain(left, target_op, operands);
            collect_commutative_chain(right, target_op, operands);
            return;
        }
    }
    operands.push(expr.clone());
}

fn fold_commutative_chain(expr: &mut Expr) -> usize {
    if let Expr::Binary { op, .. } = expr {
        let op_str = op.clone();
        if op_str == "OpAdd"
            || op_str == "OpMul"
            || op_str == "OpBitAnd"
            || op_str == "OpBitOr"
            || op_str == "OpBitXor"
        {
            let mut operands = Vec::new();
            collect_commutative_chain(expr, &op_str, &mut operands);

            let mut constants = Vec::new();
            let mut non_constants = Vec::new();

            for op_expr in operands {
                if let Expr::Number(n) = op_expr {
                    constants.push(n);
                } else {
                    non_constants.push(op_expr);
                }
            }

            if constants.len() > 1 {
                let mut folded = constants[0];
                for &val in &constants[1..] {
                    folded = match op_str.as_str() {
                        "OpAdd" => folded.wrapping_add(val),
                        "OpMul" => folded.wrapping_mul(val),
                        "OpBitAnd" => folded & val,
                        "OpBitOr" => folded | val,
                        "OpBitXor" => folded ^ val,
                        _ => folded,
                    };
                }

                if non_constants.is_empty() {
                    *expr = Expr::Number(folded);
                } else {
                    let mut current = non_constants[0].clone();
                    for next_expr in &non_constants[1..] {
                        current = Expr::Binary {
                            left: Box::new(current),
                            op: op_str.clone(),
                            right: Box::new(next_expr.clone()),
                        };
                    }
                    *expr = Expr::Binary {
                        left: Box::new(current),
                        op: op_str.clone(),
                        right: Box::new(Expr::Number(folded)),
                    };
                }
                return 1;
            }
        }
    }
    0
}

fn optimize_expr_recursive(
    expr: &mut Expr,
    consts: &HashMap<String, Expr>,
    escaped: &HashSet<String>,
) -> usize {
    let mut count = 0;

    match expr {
        Expr::Binary { left, right, .. } => {
            count += optimize_expr_recursive(left, consts, escaped);
            count += optimize_expr_recursive(right, consts, escaped);
        }
        Expr::MemberAccess { expr: base, .. } => {
            count += optimize_expr_recursive(base, consts, escaped);
        }
        Expr::Index { expr: base, index } => {
            count += optimize_expr_recursive(base, consts, escaped);
            count += optimize_expr_recursive(index, consts, escaped);
        }
        Expr::Call { args, .. } => {
            for arg in args {
                count += optimize_expr_recursive(arg, consts, escaped);
            }
        }
        Expr::AddrOfExpr(inner) => {
            count += optimize_expr_recursive(inner, consts, escaped);
        }
        _ => {}
    }

    if let Some(path) = get_expr_path(expr) {
        let root = if let Some(idx) = path.find('.') {
            &path[..idx]
        } else if let Some(idx) = path.find('[') {
            &path[..idx]
        } else {
            path.as_str()
        };

        if !escaped.contains(root) && !path.contains(':') {
            if let Some(const_val) = consts.get(&path) {
                *expr = const_val.clone();
                return count + 1;
            }
        }
    }

    count += fold_commutative_chain(expr);

    if let Expr::Binary { left, op, right } = expr {
        let op_str = op.clone();
        let a_opt = match &**left {
            Expr::SignedNumber(n) => Some(*n),
            Expr::Number(n) => Some(*n as i64),
            _ => None,
        };
        let b_opt = match &**right {
            Expr::SignedNumber(n) => Some(*n),
            Expr::Number(n) => Some(*n as i64),
            _ => None,
        };
        let is_left_signed = matches!(&**left, Expr::SignedNumber(_));
        let is_right_signed = matches!(&**right, Expr::SignedNumber(_));

        if (a_opt.is_some() && b_opt.is_some()) && (is_left_signed || is_right_signed) {
            let val_a = a_opt.unwrap();
            let val_b = b_opt.unwrap();
            let folded = match op_str.as_str() {
                "OpAdd" => Some(val_a.wrapping_add(val_b)),
                "OpSub" => Some(val_a.wrapping_sub(val_b)),
                "OpMul" => Some(val_a.wrapping_mul(val_b)),
                "OpDiv" => {
                    if val_b != 0 {
                        Some(val_a / val_b)
                    } else {
                        None
                    }
                }
                "OpMod" => {
                    if val_b != 0 {
                        Some(val_a % val_b)
                    } else {
                        None
                    }
                }
                "OpLt" | "Lt" | "<" => Some(if val_a < val_b { 1 } else { 0 }),
                "OpLtEq" | "OpLe" | "<=" => Some(if val_a <= val_b { 1 } else { 0 }),
                "OpGt" | "Gt" | ">" => Some(if val_a > val_b { 1 } else { 0 }),
                "OpGtEq" | "OpGe" | ">=" => Some(if val_a >= val_b { 1 } else { 0 }),
                "OpEq" | "OpEqEq" | "==" => Some(if val_a == val_b { 1 } else { 0 }),
                "OpNotEq" | "OpNe" | "!=" => Some(if val_a != val_b { 1 } else { 0 }),
                _ => None,
            };
            if let Some(f_val) = folded {
                *expr = Expr::SignedNumber(f_val);
                return count + 1;
            }
        }
    }

    if let Expr::Binary { left, op, right } = expr {
        let op_str = op.clone();
        if let (Expr::Number(a), Expr::Number(b)) = (&**left, &**right) {
            let val_a = *a;
            let val_b = *b;
            let folded = match op_str.as_str() {
                "OpAdd" => Some(val_a.wrapping_add(val_b)),
                "OpSub" => Some(val_a.wrapping_sub(val_b)),
                "OpMul" => Some(val_a.wrapping_mul(val_b)),
                "OpDiv" => {
                    if val_b != 0 {
                        Some(val_a / val_b)
                    } else {
                        None
                    }
                }
                "OpMod" => {
                    if val_b != 0 {
                        Some(val_a % val_b)
                    } else {
                        None
                    }
                }
                "OpBitAnd" => Some(val_a & val_b),
                "OpBitOr" => Some(val_a | val_b),
                "OpBitXor" => Some(val_a ^ val_b),
                "OpShl" => Some(val_a.checked_shl(val_b as u32).unwrap_or(0)),
                "OpShr" => Some(val_a.checked_shr(val_b as u32).unwrap_or(0)),
                "OpEq" | "OpEqEq" | "==" => Some(if val_a == val_b { 1 } else { 0 }),
                "OpNotEq" | "OpNe" | "!=" => Some(if val_a != val_b { 1 } else { 0 }),
                "OpLt" | "Lt" | "<" => Some(if val_a < val_b { 1 } else { 0 }),
                "OpLtEq" | "OpLe" | "<=" => Some(if val_a <= val_b { 1 } else { 0 }),
                "OpGt" | "Gt" | ">" => Some(if val_a > val_b { 1 } else { 0 }),
                "OpGtEq" | "OpGe" | ">=" => Some(if val_a >= val_b { 1 } else { 0 }),
                "OpAnd" | "&&" => Some(if val_a != 0 && val_b != 0 { 1 } else { 0 }),
                "OpOr" | "||" => Some(if val_a != 0 || val_b != 0 { 1 } else { 0 }),
                _ => None,
            };
            if let Some(f_val) = folded {
                *expr = Expr::Number(f_val);
                return count + 1;
            }
        }

        let is_left_zero = matches!(&**left, Expr::Number(0));
        let is_right_zero = matches!(&**right, Expr::Number(0));
        let is_left_one = matches!(&**left, Expr::Number(1));
        let is_right_one = matches!(&**right, Expr::Number(1));

        if op_str == "OpAdd" {
            if is_left_zero {
                *expr = *right.clone();
                return count + 1;
            }
            if is_right_zero {
                *expr = *left.clone();
                return count + 1;
            }
        }
        if op_str == "OpSub" {
            if is_right_zero {
                *expr = *left.clone();
                return count + 1;
            }
            if is_same_expr(left, right) {
                *expr = Expr::Number(0);
                return count + 1;
            }
        }
        if op_str == "OpMul" {
            if is_left_one {
                *expr = *right.clone();
                return count + 1;
            }
            if is_right_one {
                *expr = *left.clone();
                return count + 1;
            }
            if is_left_zero || is_right_zero {
                *expr = Expr::Number(0);
                return count + 1;
            }

            if let Expr::Number(val) = &**right {
                if *val > 1 && is_power_of_two(*val) {
                    let shift = val.trailing_zeros() as u64;
                    *expr = Expr::Binary {
                        left: left.clone(),
                        op: "OpShl".to_string(),
                        right: Box::new(Expr::Number(shift)),
                    };
                    return count + 1;
                }
            }
            if let Expr::Number(val) = &**left {
                if *val > 1 && is_power_of_two(*val) {
                    let shift = val.trailing_zeros() as u64;
                    *expr = Expr::Binary {
                        left: right.clone(),
                        op: "OpShl".to_string(),
                        right: Box::new(Expr::Number(shift)),
                    };
                    return count + 1;
                }
            }
        }
        if op_str == "OpDiv" {
            if is_right_one {
                *expr = *left.clone();
                return count + 1;
            }
            if is_same_expr(left, right) {
                *expr = Expr::Number(1);
                return count + 1;
            }

            if let Expr::Number(val) = &**right {
                if *val > 1 && is_power_of_two(*val) {
                    let shift = val.trailing_zeros() as u64;
                    *expr = Expr::Binary {
                        left: left.clone(),
                        op: "OpShr".to_string(),
                        right: Box::new(Expr::Number(shift)),
                    };
                    return count + 1;
                }
            }
        }
        if op_str == "OpBitAnd" {
            if is_left_zero || is_right_zero {
                *expr = Expr::Number(0);
                return count + 1;
            }
            if is_same_expr(left, right) {
                *expr = *left.clone();
                return count + 1;
            }
        }
        if op_str == "OpBitOr" {
            if is_left_zero {
                *expr = *right.clone();
                return count + 1;
            }
            if is_right_zero {
                *expr = *left.clone();
                return count + 1;
            }
            if is_same_expr(left, right) {
                *expr = *left.clone();
                return count + 1;
            }
        }
        if op_str == "OpBitXor" {
            if is_left_zero {
                *expr = *right.clone();
                return count + 1;
            }
            if is_right_zero {
                *expr = *left.clone();
                return count + 1;
            }
            if is_same_expr(left, right) {
                *expr = Expr::Number(0);
                return count + 1;
            }
        }
        if op_str == "OpShl" || op_str == "OpShr" {
            if is_right_zero {
                *expr = *left.clone();
                return count + 1;
            }
        }
        if op_str == "OpAnd" || op_str == "&&" {
            if is_left_zero {
                *expr = Expr::Number(0);
                return count + 1;
            }
            if is_left_one {
                *expr = *right.clone();
                return count + 1;
            }
        }
        if op_str == "OpOr" || op_str == "||" {
            if is_left_one {
                *expr = Expr::Number(1);
                return count + 1;
            }
            if is_left_zero {
                *expr = *right.clone();
                return count + 1;
            }
        }

        if is_same_expr(left, right) {
            match op_str.as_str() {
                "OpEq" | "OpEqEq" | "==" | "OpLtEq" | "OpLe" | "<=" | "OpGtEq" | "OpGe" | ">=" => {
                    *expr = Expr::Number(1);
                    return count + 1;
                }
                "OpNotEq" | "OpNe" | "!=" | "OpLt" | "Lt" | "<" | "OpGt" | "Gt" | ">" => {
                    *expr = Expr::Number(0);
                    return count + 1;
                }
                _ => {}
            }
        }
    }

    if let Expr::Binary { left, op, .. } = expr {
        if op == "OpBitNot" {
            if let Expr::Binary {
                left: inner_left,
                op: inner_op,
                ..
            } = &**left
            {
                if inner_op == "OpBitNot" {
                    *expr = *inner_left.clone();
                    return count + 1;
                }
            }
        }
    }

    count
}

fn collect_assigned(body: &[Stmt], assigned: &mut HashSet<String>) {
    for stmt in body {
        collect_assigned_stmt(stmt, assigned);
    }
}

fn collect_assigned_stmt(stmt: &Stmt, assigned: &mut HashSet<String>) {
    match stmt {
        Stmt::VarDefinition(decl) => {
            assigned.insert(decl.name.clone());
            if let Some(ref init) = decl.initial_value {
                collect_assigned_expr(init, assigned);
            }
        }
        Stmt::Assignment { targets, value } => {
            for target in targets {
                if let Some(path) = get_expr_path(target) {
                    assigned.insert(path.clone());
                    let root = if let Some(idx) = path.find('.') {
                        &path[..idx]
                    } else if let Some(idx) = path.find('[') {
                        &path[..idx]
                    } else {
                        path.as_str()
                    };
                    assigned.insert(root.to_string());
                }
                collect_assigned_expr(target, assigned);
            }
            collect_assigned_expr(value, assigned);
        }
        Stmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            collect_assigned_expr(cond, assigned);
            collect_assigned(then_branch, assigned);
            if let Some(else_stmts) = else_branch {
                collect_assigned(else_stmts, assigned);
            }
        }
        Stmt::While { cond, body } => {
            collect_assigned_expr(cond, assigned);
            collect_assigned(body, assigned);
        }
        Stmt::For {
            init,
            cond,
            post,
            body,
        } => {
            if let Some(ref i) = init {
                collect_assigned_stmt(i, assigned);
            }
            collect_assigned_expr(cond, assigned);
            if let Some(ref p) = post {
                collect_assigned_stmt(p, assigned);
            }
            collect_assigned(body, assigned);
        }
        Stmt::Return(values) => {
            for (_, expr) in values {
                collect_assigned_expr(expr, assigned);
            }
        }
        Stmt::Jmpto { args, .. } => {
            for arg in args {
                collect_assigned_stmt(arg, assigned);
            }
        }
        Stmt::Expr(expr) => {
            collect_assigned_expr(expr, assigned);
        }
        Stmt::Match {
            expr,
            cases,
            default,
        } => {
            collect_assigned_expr(expr, assigned);
            for (ce, body) in cases {
                collect_assigned_expr(ce, assigned);
                collect_assigned(body, assigned);
            }
            if let Some(d) = default {
                collect_assigned(d, assigned);
            }
        }
        Stmt::Critical(body) => {
            collect_assigned(body, assigned);
        }
        _ => {}
    }
}

fn collect_assigned_expr(expr: &Expr, assigned: &mut HashSet<String>) {
    match expr {
        Expr::Binary { left, right, .. } => {
            collect_assigned_expr(left, assigned);
            collect_assigned_expr(right, assigned);
        }
        Expr::Call { args, .. } => {
            for arg in args {
                collect_assigned_expr(arg, assigned);
            }
        }
        Expr::Index { expr: base, index } => {
            collect_assigned_expr(base, assigned);
            collect_assigned_expr(index, assigned);
        }
        Expr::MemberAccess { expr: base, .. } => {
            collect_assigned_expr(base, assigned);
        }
        _ => {}
    }
}

fn optimize_lhs_expr_recursive(
    expr: &mut Expr,
    consts: &HashMap<String, Expr>,
    escaped: &HashSet<String>,
) -> usize {
    let mut count = 0;
    match expr {
        Expr::Variable(_) => {}
        Expr::MemberAccess { expr: base, .. } => {
            count += optimize_expr_recursive(base, consts, escaped);
        }
        Expr::Index { expr: base, index } => {
            count += optimize_expr_recursive(base, consts, escaped);
            count += optimize_expr_recursive(index, consts, escaped);
        }
        _ => {
            count += optimize_expr_recursive(expr, consts, escaped);
        }
    }
    count
}

fn optimize_statements(
    stmts: Vec<Stmt>,
    consts: &mut HashMap<String, Expr>,
    escaped: &HashSet<String>,
    reads: &HashMap<String, usize>,
    output_ptrs: &mut HashSet<String>,
    count: &mut usize,
) -> Vec<Stmt> {
    let mut new_stmts = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::VarDefinition(mut decl) => {
                if decl.modifier == PtrAccess::Output
                    || decl.modifier == PtrAccess::InputOutput
                    || decl.modifier == PtrAccess::Volatile
                    || decl.modifier == PtrAccess::Atomic
                {
                    output_ptrs.insert(decl.name.clone());
                }
                if let Some(ref mut init) = decl.initial_value {
                    *count += optimize_expr_recursive(init, consts, escaped);
                }
                if !escaped.contains(&decl.name)
                    && !output_ptrs.contains(&decl.name)
                    && reads.get(&decl.name).cloned().unwrap_or(0) == 0
                {
                    *count += 1;
                    if let Some(init) = decl.initial_value {
                        if has_side_effects(&init) {
                            new_stmts.push(Stmt::Expr(*init));
                        }
                    }
                    continue;
                }

                if let Some(ref init) = decl.initial_value {
                    let is_mmio = decl.modifier == PtrAccess::Volatile
                        || decl.modifier == PtrAccess::Atomic;
                    if is_constant_expr(init) && !escaped.contains(&decl.name) && !is_mmio {
                        consts.insert(decl.name.clone(), *init.clone());
                    }
                }
                new_stmts.push(Stmt::VarDefinition(decl));
            }
            Stmt::Assignment {
                mut targets,
                mut value,
            } => {
                *count += optimize_expr_recursive(&mut value, consts, escaped);
                for target in &mut targets {
                    *count += optimize_lhs_expr_recursive(target, consts, escaped);
                }

                if targets.len() == 1 {
                    if let Expr::Variable(name) = &targets[0] {
                        if !escaped.contains(name)
                            && !output_ptrs.contains(name)
                            && reads.get(name).cloned().unwrap_or(0) == 0
                        {
                            *count += 1;
                            if has_side_effects(&value) {
                                new_stmts.push(Stmt::Expr(value));
                            }
                            continue;
                        }
                    }
                }

                if targets.len() == 1 {
                    let target = &targets[0];
                    if let Some(target_path) = get_expr_path(target) {
                        let root = if let Some(idx) = target_path.find('.') {
                            &target_path[..idx]
                        } else if let Some(idx) = target_path.find('[') {
                            &target_path[..idx]
                        } else {
                            target_path.as_str()
                        };

                        if !escaped.contains(root)
                            && !target_path.contains(':')
                            && !output_ptrs.contains(&target_path)
                        {
                            if is_constant_expr(&value) {
                                consts.insert(target_path, value.clone());
                            } else {
                                consts.remove(&target_path);
                                let prefix = format!("{}.", target_path);
                                let sub_keys: Vec<String> = consts
                                    .keys()
                                    .filter(|k| k.starts_with(&prefix))
                                    .cloned()
                                    .collect();
                                for k in sub_keys {
                                    consts.remove(&k);
                                }
                            }
                        }
                    }
                } else {
                    for target in &targets {
                        if let Some(target_path) = get_expr_path(target) {
                            consts.remove(&target_path);
                        }
                    }
                }

                new_stmts.push(Stmt::Assignment { targets, value });
            }
            Stmt::If {
                mut cond,
                then_branch,
                else_branch,
            } => {
                *count += optimize_expr_recursive(&mut cond, consts, escaped);
                if let Expr::Number(n) = cond {
                    *count += 1;
                    if n != 0 {
                        let opt_then = optimize_statements(
                            then_branch,
                            consts,
                            escaped,
                            reads,
                            output_ptrs,
                            count,
                        );
                        new_stmts.extend(opt_then);
                    } else if let Some(else_b) = else_branch {
                        let opt_else =
                            optimize_statements(else_b, consts, escaped, reads, output_ptrs, count);
                        new_stmts.extend(opt_else);
                    }
                } else {
                    let mut then_consts = consts.clone();
                    let mut then_output = output_ptrs.clone();
                    let opt_then = optimize_statements(
                        then_branch,
                        &mut then_consts,
                        escaped,
                        reads,
                        &mut then_output,
                        count,
                    );

                    let mut else_consts = consts.clone();
                    let mut else_output = output_ptrs.clone();
                    let opt_else = if let Some(else_b) = else_branch {
                        Some(optimize_statements(
                            else_b,
                            &mut else_consts,
                            escaped,
                            reads,
                            &mut else_output,
                            count,
                        ))
                    } else {
                        None
                    };

                    consts.retain(|k, v| {
                        then_consts.get(k) == Some(v) && else_consts.get(k) == Some(v)
                    });

                    output_ptrs.extend(then_output);
                    output_ptrs.extend(else_output);

                    new_stmts.push(Stmt::If {
                        cond,
                        then_branch: opt_then,
                        else_branch: opt_else,
                    });
                }
            }
            Stmt::While { mut cond, body } => {
                let mut loop_assigned = HashSet::new();
                collect_assigned(&body, &mut loop_assigned);
                collect_assigned_expr(&cond, &mut loop_assigned);

                consts.retain(|k, _| {
                    let root = if let Some(idx) = k.find('.') {
                        &k[..idx]
                    } else if let Some(idx) = k.find('[') {
                        &k[..idx]
                    } else {
                        k.as_str()
                    };
                    !loop_assigned.contains(root)
                });

                *count += optimize_expr_recursive(&mut cond, consts, escaped);
                if let Expr::Number(0) = cond {
                    *count += 1;
                    continue;
                }

                let opt_body =
                    optimize_statements(body, consts, escaped, reads, output_ptrs, count);
                new_stmts.push(Stmt::While {
                    cond,
                    body: opt_body,
                });
            }
            Stmt::For {
                init,
                mut cond,
                post,
                body,
            } => {
                let mut loop_assigned = HashSet::new();
                collect_assigned(&body, &mut loop_assigned);
                if let Some(ref p) = post {
                    collect_assigned_stmt(p, &mut loop_assigned);
                }
                collect_assigned_expr(&cond, &mut loop_assigned);

                let opt_init = if let Some(i) = init {
                    let init_wrapper = vec![*i];
                    let mut opt_init_vec = optimize_statements(
                        init_wrapper,
                        consts,
                        escaped,
                        reads,
                        output_ptrs,
                        count,
                    );
                    if opt_init_vec.is_empty() {
                        None
                    } else {
                        Some(Box::new(opt_init_vec.remove(0)))
                    }
                } else {
                    None
                };

                consts.retain(|k, _| {
                    let root = if let Some(idx) = k.find('.') {
                        &k[..idx]
                    } else if let Some(idx) = k.find('[') {
                        &k[..idx]
                    } else {
                        k.as_str()
                    };
                    !loop_assigned.contains(root)
                });

                *count += optimize_expr_recursive(&mut cond, consts, escaped);

                let opt_post = if let Some(p) = post {
                    let post_wrapper = vec![*p];
                    let mut opt_post_vec = optimize_statements(
                        post_wrapper,
                        consts,
                        escaped,
                        reads,
                        output_ptrs,
                        count,
                    );
                    if opt_post_vec.is_empty() {
                        None
                    } else {
                        Some(Box::new(opt_post_vec.remove(0)))
                    }
                } else {
                    None
                };

                let opt_body =
                    optimize_statements(body, consts, escaped, reads, output_ptrs, count);

                if let Expr::Number(0) = cond {
                    *count += 1;
                    if let Some(init_stmt) = opt_init {
                        new_stmts.push(*init_stmt);
                    }
                } else {
                    new_stmts.push(Stmt::For {
                        init: opt_init,
                        cond,
                        post: opt_post,
                        body: opt_body,
                    });
                }
            }
            Stmt::Return(mut values) => {
                for (_, ref mut expr) in &mut values {
                    *count += optimize_expr_recursive(expr, consts, escaped);
                }
                new_stmts.push(Stmt::Return(values));
            }
            Stmt::Jmpto { module_name, args } => {
                let opt_args =
                    optimize_statements(args, consts, escaped, reads, output_ptrs, count);
                new_stmts.push(Stmt::Jmpto {
                    module_name,
                    args: opt_args,
                });
            }
            Stmt::Expr(mut expr) => {
                *count += optimize_expr_recursive(&mut expr, consts, escaped);
                new_stmts.push(Stmt::Expr(expr));
            }
            Stmt::Critical(body) => {
                let opt_body = optimize_statements(
                    body,
                    consts,
                    escaped,
                    reads,
                    output_ptrs,
                    count,
                );
                new_stmts.push(Stmt::Critical(opt_body));
            }
            Stmt::Match {
                mut expr,
                cases,
                default,
            } => {
                *count += optimize_expr_recursive(&mut expr, consts, escaped);

                let mut new_cases = Vec::new();
                for (case_expr, body) in cases {
                    let mut ce = case_expr;
                    *count += optimize_expr_recursive(&mut ce, consts, escaped);
                    let opt_body =
                        optimize_statements(body, consts, escaped, reads, output_ptrs, count);
                    new_cases.push((ce, opt_body));
                }

                let new_default = default
                    .map(|d| optimize_statements(d, consts, escaped, reads, output_ptrs, count));

                new_stmts.push(Stmt::Match {
                    expr,
                    cases: new_cases,
                    default: new_default,
                });
            }
            other => {
                new_stmts.push(other);
            }
        }
    }
    new_stmts
}
