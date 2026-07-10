#![allow(dead_code)]

use crate::ast::*;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
enum AllocationState {
    Allocated { line: usize },
    Freed,
}

pub struct MemoryLeakDetector {
    tracked_allocations: HashMap<String, AllocationState>,
}

impl MemoryLeakDetector {
    pub fn new() -> Self {
        Self {
            tracked_allocations: HashMap::new(),
        }
    }

    pub fn analyze_function(&mut self, func: &FuncDecl) {
        self.tracked_allocations.clear();
        if let Some(body) = &func.body {
            self.analyze_statements(body);
            self.report_leaks(&func.name);
        }
    }

    fn analyze_statements(&mut self, stmts: &[Stmt]) {
        for (idx, stmt) in stmts.iter().enumerate() {
            match stmt {
                Stmt::VarDefinition(decl) => {
                    if let Some(init_expr) = &decl.initial_value {
                        if self.is_allocation_call(init_expr) {
                            self.tracked_allocations.insert(
                                decl.name.clone(),
                                AllocationState::Allocated { line: idx + 1 },
                            );
                        }
                    }
                }
                Stmt::Assignment { targets, value } => {
                    if self.is_allocation_call(value) {
                        for target in targets {
                            if let Expr::Variable(name) = target {
                                self.tracked_allocations.insert(
                                    name.clone(),
                                    AllocationState::Allocated { line: idx + 1 },
                                );
                            }
                        }
                    }
                }
                Stmt::Expr(Expr::Call { name, args }) => {
                    if name == "mfree" {
                        if let Some(Expr::Variable(ptr_name)) = args.first() {
                            if self.tracked_allocations.contains_key(ptr_name) {
                                self.tracked_allocations
                                    .insert(ptr_name.clone(), AllocationState::Freed);
                            }
                        }
                    }
                }
                Stmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    self.analyze_statements(then_branch);
                    if let Some(else_b) = else_branch {
                        self.analyze_statements(else_b);
                    }
                }
                Stmt::While { body, .. } => {
                    self.analyze_statements(body);
                }
                Stmt::For { body, .. } => {
                    self.analyze_statements(body);
                }
                Stmt::Jmpto { args, .. } => {
                    self.analyze_statements(args);
                }
                Stmt::Return(values) => {
                    for (_, expr) in values {
                        if let Expr::Variable(ptr_name) = expr {
                            if let Some(AllocationState::Allocated { .. }) =
                                self.tracked_allocations.get(ptr_name)
                            {
                                self.tracked_allocations.remove(ptr_name);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn is_allocation_call(&self, expr: &Expr) -> bool {
        if let Expr::Call { name, .. } = expr {
            name == "mloc" || name == "bmloc"
        } else {
            false
        }
    }

    fn report_leaks(&self, func_name: &str) {
        for (var_name, state) in &self.tracked_allocations {
            if let AllocationState::Allocated { line } = state {
                eprintln!(
                    "\x1b[33mwarning\x1b[0m: potential memory leak in function '{}'. \
                    Pointer '{}' allocated on line {} is not freed via 'mfree()'.",
                    func_name, var_name, line
                );
            }
        }
    }
}
