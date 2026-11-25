# Wand support for Zed

This directory is a self-contained Zed dev extension for the Wand language.

It registers `.w` and `.wh` files as **Wand**, including:

- syntax highlighting for Wand keywords, types, literals, comments, imports,
  pointer modifiers, builtins, functions, and fields
- bracket matching and auto-closing
- indentation rules
- symbol outline and Vim text objects

## Install

In Zed:

1. Open the Extensions view.
2. Run `zed: install dev extension`.
3. Select this `zed-wand` directory.

Files with these suffixes will then open as Wand:

- `.w` — Wand implementation
- `.wh` — Wand declarations

A first line of `sc.true` or `sc.false` also identifies a file as Wand if you
open one without a suffix. `.wh` files use the same rules as `.w` files; no
separate declaration-file mode is needed.

## What is supported

- Keywords, Wand types, environment token, `#import`, `//` comments, string and
  numeric literals, `*adr` / `*i` / `*o` / `*io`, reflection and atomic builtins.
- Bracket matching, auto-closing, 4-space indentation and `else` alignment.
- Outline of functions (including `extern fn` declarations in `.wh` files),
  struct/union/enum definitions and file-scope variables. Local variables are
  intentionally omitted from the outline.
- Vim text objects for functions, structs, unions, enums and comments.

Not supported: diagnostics, formatting, completion, `::nasm::{}` body highlighting,
and perfect parse trees (Wand-only syntax makes the C grammar recover with errors,
which is why the queries match many words as identifiers). Real-world `# ...`
comment lines are dimmed as comments in the editor only; the compiler accepts `//`.

## Git attributes and GitHub

This extension is local to Zed. GitHub does not read it. To influence GitHub's
language label in a repository today, add:

```gitattributes
*.w  linguist-language=Wand
*.wh linguist-language=Wand
```

That override does not give GitHub Wand highlighting or language stats until Wand
is registered in github-linguist; see `../linguist/README.md` for details.

## Why it uses the built-in C grammar

Wand's token and block structure is close enough to C that Zed's built-in
Tree-sitter C grammar can provide the parse tree. The queries in
`languages/wand/` override the shared tokens with Wand names and match
Wand-only words that C reports as identifiers.

This keeps the extension usable immediately, with no separate grammar
repository, pinned commit, or new commit in this repository.

If a dedicated `tree-sitter-wand` grammar is added later, change
`languages/wand/config.toml`:

```toml
grammar = "wand"
```

and add a `[grammars.wand]` section to `extension.toml` pointing at that
grammar's repository and revision.

## Validation performed

Zed itself was not available in the development environment. All five `.scm`
files were compiled with tree-sitter 0.26 against the upstream tree-sitter-c
parser, and several real `.w`/`.wh` files from the benchmark corpus and UTMS7
were parsed with that grammar; no query errors were reported.
