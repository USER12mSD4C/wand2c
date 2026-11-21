# Runnable examples

These files are the examples used by [../LEARN.md](../LEARN.md).

Run them from this directory. Local imports are resolved relative to the current working directory.

```bash
cd documentation/examples
../../target/release/wand2c hello.w -o hello -fp
./hello
```

| File | Topic |
|---|---|
| `hello.w` | Program structure |
| `numbers.w` | Variables and arithmetic |
| `control.w` | `if`, `while`, `for`, `match` |
| `functions.w` | Functions and output pointers |
| `arrays.w` | Arrays and `*adr` |
| `structs.w` | `struct` and `enum` |
| `globals.w` | Sections and `EOS` |
| `bare.w` | `sc.false`, `critical`, `::nasm::` |
| `numio.w` | Shared `show()` helper imported by several examples |

Build command by format:

```bash
../../target/release/wand2c FILE.w -o OUTPUT -fp
../../target/release/wand2c FILE.w -o OUTPUT.o -fo
../../target/release/wand2c FILE.w -o OUTPUT.bin -fr --entry ENTRY
../../target/release/wand2c FILE.w -o OUTPUT.img -fk --entry ENTRY
../../target/release/wand2c FILE.w -o OUTPUT.wexp -fw
```
