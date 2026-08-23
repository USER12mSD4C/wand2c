mod abi;
use abi::generate_elf64_binary;
mod ast;
mod checker;
mod codegen;
mod lexer;
mod optimizer;
mod parser;
mod safety;
mod token;

use ast::Span;
use checker::TypeChecker;
use codegen::{NativeGenerator, OutputFormat};
use lexer::Lexer;
use optimizer::Optimizer;
use parser::Parser;
use std::collections::HashMap;
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(0);
    }

    if args[1] == "--install-library" || args[1] == "-il" {
        if args.len() < 3 {
            eprintln!("\x1b[31;1merror\x1b[0m: --install-library requires a library path (e.g. 'libw' or 'libw/io').");
            std::process::exit(1);
        }
        install_library(&args[2]);
        std::process::exit(0);
    }

    let mut input_files = Vec::new();
    let mut output_file = None;
    let mut output_format: Option<OutputFormat> = None;
    let mut entry_name: Option<String> = None;
    let mut i = 1;

    while i < args.len() {
        if args[i] == "-o" {
            if i + 1 < args.len() {
                output_file = Some(args[i + 1].clone());
                i += 2;
            } else {
                eprintln!("\x1b[31;1merror\x1b[0m: -o option requires an argument.");
                std::process::exit(1);
            }
        } else if args[i] == "--format" {
            if i + 1 < args.len() {
                let parsed = parse_format_name(&args[i + 1]);
                match parsed {
                    Some(fmt) => select_format(&mut output_format, fmt),
                    None => {
                        eprintln!(
                            "\x1b[31;1merror\x1b[0m: unknown format '{}'",
                            args[i + 1]
                        );
                        std::process::exit(1);
                    }
                }
                i += 2;
            } else {
                eprintln!("\x1b[31;1merror\x1b[0m: --format requires an argument.");
                std::process::exit(1);
            }
        } else if args[i].starts_with("--format=") {
            let value = &args[i][9..];
            match parse_format_name(value) {
                Some(fmt) => select_format(&mut output_format, fmt),
                None => {
                    eprintln!("\x1b[31;1merror\x1b[0m: unknown format '{}'", value);
                    std::process::exit(1);
                }
            }
            i += 1;
        } else if args[i] == "-f" {
            if i + 1 < args.len() {
                let parsed = parse_format_name(&args[i + 1]);
                match parsed {
                    Some(fmt) => select_format(&mut output_format, fmt),
                    None => {
                        eprintln!(
                            "\x1b[31;1merror\x1b[0m: unknown format '{}'",
                            args[i + 1]
                        );
                        std::process::exit(1);
                    }
                }
                i += 2;
            } else {
                eprintln!("\x1b[31;1merror\x1b[0m: -f requires an argument.");
                std::process::exit(1);
            }
        } else if args[i].starts_with("-f=") {
            let value = &args[i][3..];
            match parse_format_name(value) {
                Some(fmt) => select_format(&mut output_format, fmt),
                None => {
                    eprintln!("\x1b[31;1merror\x1b[0m: unknown format '{}'", value);
                    std::process::exit(1);
                }
            }
            i += 1;
        } else if args[i] == "-fp" {
            select_format(&mut output_format, OutputFormat::Program);
            i += 1;
        } else if args[i] == "-fo" {
            select_format(&mut output_format, OutputFormat::Object);
            i += 1;
        } else if args[i] == "-fr" {
            select_format(&mut output_format, OutputFormat::Raw);
            i += 1;
        } else if args[i] == "-fk" {
            select_format(&mut output_format, OutputFormat::Kernel);
            i += 1;
        } else if args[i] == "-fw" {
            select_format(&mut output_format, OutputFormat::Wexp);
            i += 1;
        } else if args[i] == "--entry" {
            if i + 1 < args.len() {
                entry_name = Some(args[i + 1].clone());
                i += 2;
            } else {
                eprintln!("\x1b[31;1merror\x1b[0m: --entry requires an argument.");
                std::process::exit(1);
            }
        } else if args[i].starts_with('-') {
            eprintln!("\x1b[31;1merror\x1b[0m: unknown option '{}'", args[i]);
            print_usage();
            std::process::exit(1);
        } else {
            input_files.push(args[i].clone());
            i += 1;
        }
    }

    if input_files.is_empty() {
        eprintln!("\x1b[31;1merror\x1b[0m: no input files specified.");
        print_usage();
        std::process::exit(1);
    }

    let output_format = match output_format {
        Some(fmt) => fmt,
        None => {
            if input_files[0].ends_with(".wexp") {
                OutputFormat::Wexp
            } else {
                OutputFormat::Program
            }
        }
    };

    if input_files.iter().any(|f| f.ends_with(".wexp"))
        && output_format != OutputFormat::Wexp
        && output_format != OutputFormat::Object
    {
        eprintln!("\x1b[31;1merror\x1b[0m: .wexp source files require --format=wexp.");
        std::process::exit(1);
    }

    if (output_format == OutputFormat::Program || output_format == OutputFormat::Wexp)
        && entry_name.is_some()
    {
        eprintln!("\x1b[31;1merror\x1b[0m: --entry is not allowed for program or wexp format.");
        std::process::exit(1);
    }

    let had_explicit_output = output_file.is_some();

    let mut final_output = match output_file {
        Some(out) => out,
        None => {
            let first_file = &input_files[0];
            if let Some(pos) = first_file.rfind('.') {
                first_file[..pos].to_string()
            } else {
                format!("{}.bin", first_file)
            }
        }
    };

    if !had_explicit_output && output_format == OutputFormat::Wexp {
        let first_file = &input_files[0];
        final_output = if let Some(pos) = first_file.rfind('.') {
            format!("{}.wexp", &first_file[..pos])
        } else {
            format!("{}.wexp", first_file)
        };
    }

    if !had_explicit_output && output_format == OutputFormat::Object {
        let first_file = &input_files[0];
        final_output = if let Some(pos) = first_file.rfind('.') {
            format!("{}.o", &first_file[..pos])
        } else {
            format!("{}.o", first_file)
        };
    }

    println!("\x1b[32;1m[wand2c]\x1b[0m Starting multi-file compilation pipeline...");
    println!("  \x1b[34;1mStage 1:\x1b[0m Lexing and Parsing");

    let mut program = ast::Program {
        use_os: true,
        imports: Vec::new(),
        typedefs: Vec::new(),
        structs: Vec::new(),
        enums: Vec::new(),
        constants: Vec::new(),
        sections: Vec::new(),
        functions: Vec::new(),
    };

    let mut function_sources = HashMap::new();

    for filename in &input_files {
        println!(
            "    \x1b[37;1mSource:\x1b[0m Processing file: '{}'",
            filename
        );
        let source_code = match fs::read_to_string(filename) {
            Ok(content) => content,
            Err(e) => {
                eprintln!(
                    "\x1b[31;1merror\x1b[0m: failed to read file '{}': {}",
                    filename, e
                );
                std::process::exit(1);
            }
        };

        let lexer = Lexer::new(&source_code);
        let mut parser = Parser::new(lexer);

        match parser.parse_program() {
            Ok(parsed) => {
                if program.functions.is_empty() && program.structs.is_empty() {
                    program.use_os = parsed.use_os;
                } else if program.use_os != parsed.use_os {
                    eprintln!("\x1b[33;1mwarning\x1b[0m: conflicting sc.true/sc.false settings across files");
                }
                for func in &parsed.functions {
                    if !func.is_extern {
                        function_sources
                            .insert(func.name.clone(), (filename.clone(), source_code.clone()));
                    }
                }
                program.imports.extend(parsed.imports);
                program.typedefs.extend(parsed.typedefs);
                program.structs.extend(parsed.structs);
                program.sections.extend(parsed.sections);
                program.functions.extend(parsed.functions);
            }
            Err(err) => {
                report_parse_error(filename, &source_code, &err.message, &err.span);
                std::process::exit(1);
            }
        }
    }

    let mut resolved_imports = std::collections::HashSet::new();
    let mut imports_to_resolve = program.imports.clone();

    while let Some(imp_name) = imports_to_resolve.pop() {
        if resolved_imports.contains(&imp_name) {
            continue;
        }
        resolved_imports.insert(imp_name.clone());

        let mut resolved = false;
        let (wh_filename, w_filename) = resolve_import_path(&imp_name);

        if std::path::Path::new(&wh_filename).exists() {
            resolved = true;
            println!(
                "    \x1b[36mHeader Dependency:\x1b[0m Loaded and parsed '{}'",
                wh_filename
            );
            let wh_source = fs::read_to_string(&wh_filename).expect("Failed to read header file");
            let wh_lexer = Lexer::new(&wh_source);
            let mut wh_parser = Parser::new(wh_lexer);

            match wh_parser.parse_program() {
                Ok(wh_program) => {
                    program.typedefs.extend(wh_program.typedefs);
                    program.structs.extend(wh_program.structs);
                    program.functions.extend(wh_program.functions);
                    program.sections.extend(wh_program.sections);

                    for sub_imp in wh_program.imports {
                        if !resolved_imports.contains(&sub_imp) {
                            imports_to_resolve.push(sub_imp);
                        }
                    }
                }
                Err(err) => {
                    report_parse_error(&wh_filename, &wh_source, &err.message, &err.span);
                    std::process::exit(1);
                }
            }
        }

        if std::path::Path::new(&w_filename).exists() {
            resolved = true;
            println!(
                "    \x1b[36mSource Dependency (Auto-Load):\x1b[0m Loaded and parsed '{}'",
                w_filename
            );
            let w_source =
                fs::read_to_string(&w_filename).expect("Failed to read implementation file");
            let w_lexer = Lexer::new(&w_source);
            let mut w_parser = Parser::new(w_lexer);
            match w_parser.parse_program() {
                Ok(w_program) => {
                    for sub_imp in w_program.imports {
                        if !resolved_imports.contains(&sub_imp) {
                            imports_to_resolve.push(sub_imp);
                        }
                    }
                    for func in &w_program.functions {
                        if !func.is_extern {
                            function_sources
                                .insert(func.name.clone(), (w_filename.clone(), w_source.clone()));
                        }
                        let existing_pos = program
                            .functions
                            .iter()
                            .position(|f| f.name == func.name);
                        if let Some(pos) = existing_pos {
                            if program.functions[pos].is_extern && !func.is_extern {
                                program.functions[pos] = func.clone();
                            }
                        } else {
                            program.functions.push(func.clone());
                        }
                    }
                    program.structs.extend(w_program.structs);
                    program.sections.extend(w_program.sections);
                    program.typedefs.extend(w_program.typedefs);
                }
                Err(err) => {
                    report_parse_error(&w_filename, &w_source, &err.message, &err.span);
                    std::process::exit(1);
                }
            }
        }

        if !resolved {
            eprintln!(
                "\x1b[31;1merror\x1b[0m: failed to resolve import '{}'. \
                Neither '{}' nor '{}' exists.",
                imp_name, wh_filename, w_filename
            );
            std::process::exit(1);
        }
    }

    for func in &mut program.functions {
        if func.is_extern {
            continue;
        }

        if let Some((func_filename, func_source)) = function_sources.get(&func.name) {
            let local_lexer = Lexer::new(func_source);
            let mut local_parser = Parser::new(local_lexer);
            if let Err(err) = local_parser.seek_to_function(&func.name) {
                report_parse_error(func_filename, func_source, &err.message, &err.span);
                std::process::exit(1);
            }
            match local_parser.parse_function_body() {
                Ok(body) => {
                    func.body = Some(body);
                }
                Err(err) => {
                    report_parse_error(func_filename, func_source, &err.message, &err.span);
                    std::process::exit(1);
                }
            }
        }
    }

    println!(
        "    \x1b[32mSymbols Merged:\x1b[0m structs={}, sections={}, functions={}, typedefs={}",
        program.structs.len(),
        program.sections.len(),
        program.functions.len(),
        program.typedefs.len()
    );

    match output_format {
        OutputFormat::Program => {
            if !program.use_os {
                eprintln!("\x1b[31;1merror\x1b[0m: program format requires sc.true.");
                std::process::exit(1);
            }
        }
        OutputFormat::Kernel => {
            if program.use_os {
                eprintln!("\x1b[31;1merror\x1b[0m: kernel format requires sc.false.");
                std::process::exit(1);
            }
        }
        _ => {}
    }

    if output_format != OutputFormat::Object {
        let required_entry = match output_format {
            OutputFormat::Program | OutputFormat::Wexp => "main".to_string(),
            _ => entry_name.clone().unwrap_or_else(|| "main".to_string()),
        };

        if !program.functions.iter().any(|f| f.name == required_entry) {
            eprintln!(
                "\x1b[31;1merror\x1b[0m: entry function '{}' not found.",
                required_entry
            );
            std::process::exit(1);
        }
    }

    println!("  \x1b[34;1mStage 2:\x1b[0m AST Optimization Pass");
    let folded_count = Optimizer::optimize_program(&mut program);
    println!(
        "    \x1b[32mOptimized:\x1b[0m Folded {} binary expressions into constant literals",
        folded_count
    );

    println!("  \x1b[34;1mStage 3:\x1b[0m Type Checking & Safety Analysis");
    let mut checker = TypeChecker::new();
    checker.populate_symbols(&program);
    if let Err(e) = checker.verify_calls(&program) {
        eprintln!("\n  \x1b[31;1m[checker error]\x1b[0m {}", e);
        std::process::exit(1);
    }

    for s in &program.structs {
        if let Err(e) = checker.calculate_struct_layout(&s.name) {
            eprintln!("    [checker error] {}", e);
            std::process::exit(1);
        }
    }

    let mut structs_map = HashMap::new();
    for s in &program.structs {
        structs_map.insert(s.name.clone(), s.clone());
    }

    let mut safety_analyzer = safety::MemorySafetyAnalyzer::new();
    let mut safety_errors = 0;

    for func in &program.functions {
        let (source, base_line) =
            if let Some((_filename, func_source)) = function_sources.get(&func.name) {
                let mut line = 1usize;
                for (i, l) in func_source.lines().enumerate() {
                    if l.contains(&format!("fn {}", func.name))
                        || l.contains(&format!("export fn {}", func.name))
                    {
                        line = i + 1;
                        break;
                    }
                }
                (Some(func_source.as_str()), line)
            } else {
                (None, 0)
            };
        if let Err(errors) =
            safety_analyzer.analyze_function(func, &structs_map, source, base_line)
        {
            for err in errors {
                eprintln!("{}", err);
                if err.contains("error") {
                    safety_errors += 1;
                }
            }
        }
    }

    if safety_errors > 0 {
        eprintln!(
            "\n\x1b[31;1merror\x1b[0m: compilation aborted due to {} safety violation(s).",
            safety_errors
        );
        std::process::exit(1);
    }

    println!("  \x1b[34;1mStage 4:\x1b[0m Direct x86_64 Code Generation");
    let mut generator = NativeGenerator::new();
    generator.output_format = output_format;
    generator.entry_name = entry_name.clone();
    generator.use_os = program.use_os;
    let raw_machine_code = generator.compile_program(&program);

    println!("  \x1b[34;1mStage 5:\x1b[0m Binary Packaging & ABI Linking");

    println!("    \x1b[37;1mLinking functions from source files:\x1b[0m");
    for (func_name, offset) in &generator.function_offsets {
        let origin = if let Some((file, _)) = function_sources.get(func_name) {
            file.as_str()
        } else {
            "stdlib"
        };
        println!(
            "      {}: '{}' -> offset: 0x{:x} (virtual addr: 0x{:x})",
            origin,
            func_name,
            offset,
            0x400078 + offset
        );
    }

    let mut unresolved_calls = Vec::new();
    for (_, target_name) in &generator.call_patches {
        if !generator.function_offsets.contains_key(target_name) {
            if !unresolved_calls.contains(target_name) {
                unresolved_calls.push(target_name.clone());
            }
        }
    }
    if !unresolved_calls.is_empty() {
        println!("    \x1b[37;1mLinking unresolved symbols as external imports:\x1b[0m");
        for unresolved in &unresolved_calls {
            println!(
                "      Import: '{}' -> resolved via Standard 4/6 ABI loader",
                unresolved
            );
        }
    }

    let executable_image = if output_format == OutputFormat::Program
        || output_format == OutputFormat::Wexp
    {
        generate_elf64_binary(&raw_machine_code, &program, &generator)
    } else {
        raw_machine_code.clone()
    };

    println!("    \x1b[37;1mLinking ELF SHT Sections:\x1b[0m");
    println!("      .text          (0x400078) -> Executive payload");
    println!("      .p46_header    (ABI Magic + Metadata)");
    println!("      .p46_types     (TLV structures declarations)");
    println!("      .p46_exports   (Exported symbols & signatures)");
    println!("      .p46_deps      (Standard 4/6 dependencies)");
    println!("      .p46_strtab    (Symbol names String Table)");

    if let Err(e) = fs::write(&final_output, &executable_image) {
        eprintln!(
            "\x1b[31;1merror\x1b[0m: writing binary to '{}': {}",
            final_output, e
        );
        std::process::exit(1);
    }

    if output_format == OutputFormat::Program {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            if let Ok(metadata) = fs::metadata(&final_output) {
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o755);

                if let Err(e) = fs::set_permissions(&final_output, permissions) {
                    eprintln!(
                        "\x1b[33;1mwarning\x1b[0m: failed to set executable permissions: {}",
                        e
                    );
                }
            }
        }
    }

    println!(
        "\x1b[32;1m[wand2c]\x1b[0m \x1b[32;1mSuccess:\x1b[0m Built '{}' ({} bytes).",
        final_output,
        executable_image.len()
    );
}

fn resolve_import_path(imp_name: &str) -> (String, String) {
    if imp_name.starts_with('<') && imp_name.ends_with('>') {
        let lib_name = &imp_name[1..imp_name.len() - 1];
        let system_dir = std::env::var("WAND_LIB_PATH").unwrap_or_else(|_| {
            if let Ok(home) = std::env::var("HOME") {
                format!("{}/.local/lib/libw", home)
            } else {
                "/usr/local/lib/libw".to_string()
            }
        });
        (
            format!("{}/{}.wh", system_dir, lib_name),
            format!("{}/{}.w", system_dir, lib_name),
        )
    } else {
        (format!("{}.wh", imp_name), format!("{}.w", imp_name))
    }
}

fn validate_file(filename: &str) -> Result<(), String> {
    if !std::path::Path::new(filename).exists() {
        return Ok(());
    }
    let source_code =
        std::fs::read_to_string(filename).map_err(|e| format!("failed to read file: {}", e))?;

    let lexer = Lexer::new(&source_code);
    let mut parser = Parser::new(lexer);

    let program = parser.parse_program().map_err(|e| {
        format!(
            "parse error on line {}, col {}: {}",
            e.span.line, e.span.col, e.message
        )
    })?;

    if filename.ends_with(".wh") {
        return Ok(());
    }

    for func in &program.functions {
        let local_lexer = Lexer::new(&source_code);
        let mut local_parser = Parser::new(local_lexer);
        if local_parser.seek_to_function(&func.name).is_ok() {
            local_parser.parse_function_body().map_err(|e| {
                format!(
                    "in function '{}' on line {}, col {}: {}",
                    func.name, e.span.line, e.span.col, e.message
                )
            })?;
        }
    }

    Ok(())
}

fn install_library(lib_path: &str) {
    let path = std::path::Path::new(lib_path);

    if path.is_dir() {
        println!(
            "    \x1b[36mMulti-Install:\x1b[0m Scanning directory '{}'...",
            lib_path
        );
        let entries = match std::fs::read_dir(path) {
            Ok(e) => e,
            Err(e) => {
                eprintln!(
                    "\x1b[31;1merror\x1b[0m: failed to read directory '{}': {}",
                    lib_path, e
                );
                std::process::exit(1);
            }
        };

        let mut installed_any = false;
        for entry in entries {
            if let Ok(entry) = entry {
                let fpath = entry.path();
                if fpath.is_file() {
                    let ext = fpath.extension().and_then(|s| s.to_str()).unwrap_or("");
                    if ext == "w" {
                        let file_stem_path = fpath.with_extension("");
                        let stem_str = file_stem_path.to_str().unwrap_or("");
                        if !stem_str.is_empty() {
                            install_single_file(stem_str);
                            installed_any = true;
                        }
                    }
                }
            }
        }

        if installed_any {
            println!(
                "\x1b[32;1m[wand2c]\x1b[0m \x1b[32;1mSuccess:\x1b[0m Installed all libraries from directory '{}'!",
                lib_path
            );
        } else {
            eprintln!(
                "\x1b[31;1merror\x1b[0m: no library files (.w/.wh) found in directory '{}'",
                lib_path
            );
        }
        return;
    }

    install_single_file(lib_path);
}

fn install_single_file(lib_path: &str) {
    let path = std::path::Path::new(lib_path);
    let stem = match path.file_stem() {
        Some(s) => s.to_str().unwrap_or(""),
        None => {
            eprintln!("\x1b[31;1merror\x1b[0m: invalid library path");
            std::process::exit(1);
        }
    };

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let target_dir =
        std::env::var("WAND_LIB_PATH").unwrap_or_else(|_| format!("{}/.local/lib/libw", home));

    let source_w = format!("{}.w", lib_path);
    let source_wh = format!("{}.wh", lib_path);

    if std::path::Path::new(&source_w).exists() {
        println!("    Validating library code '{}'...", source_w);
        if let Err(err) = validate_file(&source_w) {
            eprintln!(
                "\x1b[31;1merror\x1b[0m: validation failed for library file '{}':\n  {}",
                source_w, err
            );
            std::process::exit(1);
        }
    }
    if std::path::Path::new(&source_wh).exists() {
        println!("    Validating library header '{}'...", source_wh);
        if let Err(err) = validate_file(&source_wh) {
            eprintln!(
                "\x1b[31;1merror\x1b[0m: validation failed for library file '{}':\n  {}",
                source_wh, err
            );
            std::process::exit(1);
        }
    }

    if let Err(e) = std::fs::create_dir_all(&target_dir) {
        eprintln!(
            "\x1b[31;1merror\x1b[0m: failed to create directory '{}': {}",
            target_dir, e
        );
        std::process::exit(1);
    }

    let target_w = format!("{}/{}.w", target_dir, stem);
    let target_wh = format!("{}/{}.wh", target_dir, stem);

    let mut copied = false;

    if std::path::Path::new(&source_w).exists() {
        if let Err(e) = std::fs::copy(&source_w, &target_w) {
            eprintln!(
                "\x1b[31;1merror copying\x1b[0m '{}' -> '{}': {}",
                source_w, target_w, e
            );
            std::process::exit(1);
        }
        println!("    Installed: '{}' -> '{}'", source_w, target_w);
        copied = true;
    }

    if std::path::Path::new(&source_wh).exists() {
        if let Err(e) = std::fs::copy(&source_wh, &target_wh) {
            eprintln!(
                "\x1b[31;1merror copying\x1b[0m '{}' -> '{}': {}",
                source_wh, target_wh, e
            );
            std::process::exit(1);
        }
        println!("    Installed: '{}' -> '{}'", source_wh, target_wh);
        copied = true;
    }

    if copied {
        println!(
            "\x1b[32;1m[wand2c]\x1b[0m \x1b[32;1mSuccess:\x1b[0m Installed single library '{}'!",
            stem
        );
    } else {
        eprintln!(
            "\x1b[31;1merror\x1b[0m: no source files found at '{}'",
            lib_path
        );
    }
}

fn report_parse_error(filename: &str, source: &str, message: &str, span: &Span) {
    eprintln!("\x1b[31;1merror\x1b[0m: {}", message);
    eprintln!(
        "  \x1b[34;1m-->\x1b[0m {}:{}:{}",
        filename, span.line, span.col
    );

    let lines: Vec<&str> = source.lines().collect();
    if span.line > 0 && span.line <= lines.len() {
        let line_content = lines[span.line - 1];

        let line_num_str = format!("{} | ", span.line);
        let padding = " ".repeat(line_num_str.len() - 3);

        eprintln!("{}|", padding);
        eprintln!("{}{}", line_num_str, line_content);

        let caret_padding = " ".repeat(span.col - 1);
        let highlight_len = if span.end > span.start {
            span.end - span.start
        } else {
            1
        };
        let carets = "^".repeat(highlight_len);

        eprintln!("{}| \x1b[31;1m{}{}\x1b[0m", padding, caret_padding, carets);
        eprintln!("{}|", padding);
    }
    eprintln!();
}

fn print_usage() {
    println!("wand2c - Wand Version 2 Compiler");
    println!("Usage:");
    println!("  wand2c <input_files.w> [options]");
    println!("  wand2c --install-library <lib_path>");
    println!("  wand2c -il <lib_path>");
    println!();
    println!("Options:");
    println!("  -o <filename>        Output file path");
    println!("  --format=<name>      Set output format");
    println!("  -f <name>            Set output format");
    println!("  -fp                  Same as --format=program");
    println!("  -fo                  Same as --format=object");
    println!("  -fr                  Same as --format=raw");
    println!("  -fk                  Same as --format=kernel");
    println!("  -fw                  Same as --format=wexp");
    println!("  --entry <function>   Set entry function for raw or kernel format");
    println!();
    println!("Formats:");
    println!("  program              Hosted executable file");
    println!("  object               Relocatable ELF object");
    println!("  raw                  Flat binary image");
    println!("  kernel               Freestanding kernel or kernel module image");
    println!("  wexp                 Dynamic execution module");
}

fn parse_format_name(name: &str) -> Option<OutputFormat> {
    match name {
        "program" => Some(OutputFormat::Program),
        "object" => Some(OutputFormat::Object),
        "raw" => Some(OutputFormat::Raw),
        "kernel" => Some(OutputFormat::Kernel),
        "wexp" => Some(OutputFormat::Wexp),
        _ => None,
    }
}

fn select_format(current: &mut Option<OutputFormat>, new_format: OutputFormat) {
    if let Some(existing) = current {
        if *existing != new_format {
            eprintln!("\x1b[31;1merror\x1b[0m: conflicting output formats.");
            std::process::exit(1);
        }
    } else {
        *current = Some(new_format);
    }
}
