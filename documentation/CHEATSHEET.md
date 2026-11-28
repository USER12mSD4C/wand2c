# Quick Reference

This page is an index, not a second syntax specification. Use [SYNTAX.md](SYNTAX.md) for complete rules and [LEARN.md](LEARN.md) for explanation.

## Program skeleton

```wand
sc.true
#import <io>

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    return(0);
}
```

`sc.false` replaces `sc.true` for freestanding code. Raw and kernel formats use `main` as the default entry; pass `--entry kmain` to select `kmain`.

## Commands

```bash
wand2c FILE.w -o OUT -fp                    # hosted program
wand2c FILE.w -o OUT.o -fo                  # relocatable object
wand2c FILE.w -o OUT.bin -fr --entry NAME   # flat binary
wand2c FILE.w -o OUT.img -fk --entry NAME   # kernel image
wand2c FILE.w -o OUT.wexp -fw               # dynamic module
wand2c -il libw/llibw.86                    # install standard library
```

## Documentation index

| Topic | Section |
|---|---|
| Source files and imports | [SYNTAX 1-3](SYNTAX.md#1-source-model) |
| Comments, identifiers, literals | [SYNTAX 4-6](SYNTAX.md#4-comments) |
| Types and pointers | [SYNTAX 7-9](SYNTAX.md#7-primitive-types) |
| Constants and user types | [SYNTAX 10-14](SYNTAX.md#10-constants) |
| Variables and pointer modifiers | [SYNTAX 15-17](SYNTAX.md#15-variables) |
| Operators and conversions | [SYNTAX 18-19](SYNTAX.md#18-operators) |
| Control flow and functions | [SYNTAX 20-22](SYNTAX.md#20-control-flow) |
| Sections and concurrency | [SYNTAX 23-25](SYNTAX.md#23-global-sections) |
| Hardware features | [SYNTAX 26-28](SYNTAX.md#26-critical-sections) |
| Dynamic modules and reflection | [SYNTAX 29-30](SYNTAX.md#29-jmpto-statement) |
| Safety and optimizer | [SYNTAX 31-32](SYNTAX.md#31-memory-safety-analysis) |
| Output formats and restrictions | [SYNTAX 33-34](SYNTAX.md#33-compiler-output-formats) |
| Compact full reference for agents | [AGENT_REFERENCE.md](AGENT_REFERENCE.md) |

## Sharp edges

- Use `=` for assignment and `==` for comparison.
- System imports use logical names without extensions or path separators. Quoted local imports may contain subdirectories but no file extension.
- Multi-level pointers are not allowed.
- `critical` requires `sc.false`.
- Use explicit casts at narrowing boundaries.

Current implementation limitations are listed in [TROUBLESHOOTING.md](TROUBLESHOOTING.md).
