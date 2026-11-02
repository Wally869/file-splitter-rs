use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use syn::visit::Visit;

struct FunctionInfo {
    name: String,
    parent_impl: Option<String>,
    start_line: usize,
    end_line: usize,
}

impl FunctionInfo {
    fn line_count(&self) -> usize {
        self.end_line.saturating_sub(self.start_line) + 1
    }
}

struct FunctionVisitor {
    functions: Vec<FunctionInfo>,
    current_impl: Option<String>,
}

impl FunctionVisitor {
    fn new() -> Self {
        Self {
            functions: Vec::new(),
            current_impl: None,
        }
    }
}

impl<'ast> Visit<'ast> for FunctionVisitor {
    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let self_ty = &node.self_ty;
        let type_name = quote::quote!(#self_ty).to_string().replace(" ", "");
        self.current_impl = Some(type_name);
        syn::visit::visit_item_impl(self, node);
        self.current_impl = None;
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let name = node.sig.ident.to_string();
        let span = node.sig.ident.span();
        let start = span.start();
        let end_span = node.block.brace_token.span.close();
        let end = end_span.end();

        self.functions.push(FunctionInfo {
            name,
            parent_impl: None,
            start_line: start.line,
            end_line: end.line,
        });
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let name = node.sig.ident.to_string();
        let span = node.sig.ident.span();
        let start = span.start();
        let end_span = node.block.brace_token.span.close();
        let end = end_span.end();

        self.functions.push(FunctionInfo {
            name,
            parent_impl: self.current_impl.clone(),
            start_line: start.line,
            end_line: end.line,
        });
    }
}

pub fn run(file: &Path, names_only: bool) -> Result<()> {
    let source = fs::read_to_string(file)
        .with_context(|| format!("Failed to read {}", file.display()))?;

    let parsed = syn::parse_file(&source)
        .map_err(|e| anyhow::anyhow!("Failed to parse {}: {}", file.display(), e))?;

    let mut visitor = FunctionVisitor::new();
    visitor.visit_file(&parsed);

    if visitor.functions.is_empty() {
        println!("No functions found in {}", file.display());
        return Ok(());
    }

    if names_only {
        for f in &visitor.functions {
            println!("{}", f.name);
        }
        return Ok(());
    }

    println!(
        "{:<40} {:<30} {:<8} {:<8} {}",
        "Function", "Impl", "Start", "End", "Lines"
    );
    println!(
        "{:<40} {:<30} {:<8} {:<8} {}",
        "--------", "----", "-----", "---", "-----"
    );

    for f in &visitor.functions {
        let impl_name = f.parent_impl.as_deref().unwrap_or("-");
        println!(
            "{:<40} {:<30} {:<8} {:<8} {}",
            f.name,
            impl_name,
            f.start_line,
            f.end_line,
            f.line_count()
        );
    }

    let total_lines: usize = visitor.functions.iter().map(|f| f.line_count()).sum();
    println!(
        "\nTotal: {} functions, {} lines of function bodies",
        visitor.functions.len(),
        total_lines
    );

    Ok(())
}
