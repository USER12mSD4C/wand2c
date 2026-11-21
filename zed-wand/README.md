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
open one without a suffix.

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
