#![allow(dead_code)]

use crate::ast::*;

pub struct Optimizer;

impl Optimizer {
    pub fn optimize_program(program: &mut Program) -> usize {
        let mut count = 0;
        for func in &mut program.functions {
            if let Some(ref mut body) = func.body {
                count += Self::optimize_block(body);
            }
        }
        count
    }

    /// Рекурсивная оптимизация вектора инструкций (блока).
    /// Позволяет удалять мертвый код и встраивать ветки с константными условиями.
    fn optimize_block(stmts: &mut Vec<Stmt>) -> usize {
        let mut count = 0;
        let mut optimized_stmts = Vec::new();
        let mut unreachable = false;

        // Забираем старый список инструкций для обработки
        for mut stmt in std::mem::take(stmts) {
            if unreachable {
                // Все инструкции после return или безусловного перехода удаляются
                count += 1;
                continue;
            }

            // Предварительно оптимизируем выражения во вложенных конструкциях
            match &mut stmt {
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
                Stmt::Return(values) => {
                    for (_, expr) in values {
                        count += Self::optimize_expr(expr);
                    }
                }
                Stmt::Expr(expr) => {
                    count += Self::optimize_expr(expr);
                }
                Stmt::Jmpto { args, .. } => {
                    for arg in args {
                        count += Self::optimize_stmt(arg);
                    }
                }
                _ => {}
            }

            // Проверяем, прерывает ли текущая инструкция выполнение блока
            if let Stmt::Return(_) = stmt {
                unreachable = true;
            }

            // Высокоуровневые оптимизации управляющих конструкций
            match stmt {
                Stmt::If {
                    mut cond,
                    mut then_branch,
                    mut else_branch,
                } => {
                    count += Self::optimize_expr(&mut cond);
                    if let Expr::Number(n) = cond {
                        count += 1; // Убираем сам заголовок ветвления
                        if n != 0 {
                            // Ветка всегда истинна, переносим инструкции в основной блок
                            count += Self::optimize_block(&mut then_branch);
                            optimized_stmts.extend(then_branch);
                        } else if let Some(mut else_stmts) = else_branch {
                            // Ветка всегда ложна, переносим else-ветку в основной блок
                            count += Self::optimize_block(&mut else_stmts);
                            optimized_stmts.extend(else_stmts);
                        }
                    } else {
                        count += Self::optimize_block(&mut then_branch);
                        if let Some(ref mut else_stmts) = else_branch {
                            count += Self::optimize_block(else_stmts);
                        }
                        optimized_stmts.push(Stmt::If {
                            cond,
                            then_branch,
                            else_branch,
                        });
                    }
                }
                Stmt::While { mut cond, mut body } => {
                    count += Self::optimize_expr(&mut cond);
                    if let Expr::Number(0) = cond {
                        // Цикл с ложным условием никогда не выполнится
                        count += 1 + body.len();
                    } else {
                        count += Self::optimize_block(&mut body);
                        optimized_stmts.push(Stmt::While { cond, body });
                    }
                }
                Stmt::For {
                    mut init,
                    mut cond,
                    mut post,
                    mut body,
                } => {
                    count += Self::optimize_expr(&mut cond);
                    if let Expr::Number(0) = cond {
                        // Тело цикла не выполнится, но инициализатор должен отработать один раз
                        count += 1 + body.len();
                        if let Some(init_stmt) = init {
                            let mut wrapper = vec![*init_stmt];
                            count += Self::optimize_block(&mut wrapper);
                            optimized_stmts.extend(wrapper);
                        }
                    } else {
                        if let Some(ref mut init_stmt) = init {
                            count += Self::optimize_stmt(init_stmt);
                        }
                        if let Some(ref mut post_stmt) = post {
                            count += Self::optimize_stmt(post_stmt);
                        }
                        count += Self::optimize_block(&mut body);
                        optimized_stmts.push(Stmt::For {
                            init,
                            cond,
                            post,
                            body,
                        });
                    }
                }
                _ => {
                    optimized_stmts.push(stmt);
                }
            }
        }

        *stmts = optimized_stmts;
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
                count += Self::optimize_block(then_branch);
                if let Some(ref mut else_stmts) = else_branch {
                    count += Self::optimize_block(else_stmts);
                }
            }
            Stmt::While { cond, body } => {
                count += Self::optimize_expr(cond);
                count += Self::optimize_block(body);
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
                count += Self::optimize_block(body);
            }
            Stmt::Return(values) => {
                for (_, expr) in values {
                    count += Self::optimize_expr(expr);
                }
            }
            Stmt::Jmpto { args, .. } => {
                for arg in args {
                    count += Self::optimize_stmt(arg);
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

                // 1. Полноценное сворачивание числовых констант
                if let (Expr::Number(a), Expr::Number(b)) = (&**left, &**right) {
                    let val_a = *a;
                    let val_b = *b;
                    let folded = match op.as_str() {
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
                        "OpAnd" => Some(if val_a != 0 && val_b != 0 { 1 } else { 0 }),
                        "OpOr" => Some(if val_a != 0 || val_b != 0 { 1 } else { 0 }),
                        _ => None,
                    };
                    if let Some(f_val) = folded {
                        *expr = Expr::Number(f_val);
                        count += 1;
                        return count;
                    }
                }

                // Вспомогательные переменные для анализа алгебраических свойств
                let is_left_zero = matches!(&**left, Expr::Number(0));
                let is_right_zero = matches!(&**right, Expr::Number(0));
                let is_left_one = matches!(&**left, Expr::Number(1));
                let is_right_one = matches!(&**right, Expr::Number(1));

                // 2. Алгебраические упрощения
                if op == "OpAdd" {
                    if is_left_zero {
                        *expr = *right.clone();
                        return count + 1;
                    }
                    if is_right_zero {
                        *expr = *left.clone();
                        return count + 1;
                    }
                }
                if op == "OpSub" {
                    if is_right_zero {
                        *expr = *left.clone();
                        return count + 1;
                    }
                }
                if op == "OpMul" {
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

                    // Оптимизация умножения на степени двойки (замена на сдвиг влево)
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
                if op == "OpDiv" {
                    if is_right_one {
                        *expr = *left.clone();
                        return count + 1;
                    }

                    // Оптимизация деления на степени двойки (замена на сдвиг вправо)
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
                if op == "OpBitAnd" {
                    if is_left_zero || is_right_zero {
                        *expr = Expr::Number(0);
                        return count + 1;
                    }
                }
                if op == "OpBitOr" {
                    if is_left_zero {
                        *expr = *right.clone();
                        return count + 1;
                    }
                    if is_right_zero {
                        *expr = *left.clone();
                        return count + 1;
                    }
                }
                if op == "OpBitXor" {
                    if is_left_zero {
                        *expr = *right.clone();
                        return count + 1;
                    }
                    if is_right_zero {
                        *expr = *left.clone();
                        return count + 1;
                    }
                }
                if op == "OpShl" || op == "OpShr" {
                    if is_right_zero {
                        *expr = *left.clone();
                        return count + 1;
                    }
                }

                // 3. Оптимизация короткого замыкания логики
                if op == "OpAnd" || op == "&&" {
                    if is_left_zero {
                        *expr = Expr::Number(0);
                        return count + 1;
                    }
                    if is_left_one {
                        *expr = *right.clone();
                        return count + 1;
                    }
                }
                if op == "OpOr" || op == "||" {
                    if is_left_one {
                        *expr = Expr::Number(1);
                        return count + 1;
                    }
                    if is_left_zero {
                        *expr = *right.clone();
                        return count + 1;
                    }
                }

                // 4. Оптимизация структурно идентичных выражений (x - x => 0)
                if is_same_expr(left, right) {
                    match op.as_str() {
                        "OpSub" | "OpBitXor" => {
                            *expr = Expr::Number(0);
                            return count + 1;
                        }
                        "OpEq" | "OpEqEq" | "==" | "OpLtEq" | "OpLe" | "<=" | "OpGtEq" | "OpGe"
                        | ">=" => {
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
                    count += Self::optimize_expr(arg);
                }
            }
            _ => {}
        }
        count
    }
}

/// Проверка, является ли число степенью двойки
fn is_power_of_two(val: u64) -> bool {
    val > 0 && (val & (val - 1)) == 0
}

/// Вспомогательное глубокое сравнение двух выражений без предположения о реализации PartialEq
fn is_same_expr(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        (Expr::Number(n1), Expr::Number(n2)) => n1 == n2,
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
        _ => false,
    }
}
