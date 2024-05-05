# recasm - ReCOP Assembler

Okay, here is an assembler. \ 
However, it is not that good. \
Be nice ;-;

## Building

Install a rust toolchain for your platform by following the instructions 
[here](https://forge.rust-lang.org/infra/other-installation-methods.html).

run `cargo install --path .` in the root directory of the git repo to install
the assembler to `~/.cargo/bin`. If `~/.cargo/bin` is in your `PATH` environment
variable it now should be able to be invoked by typing `recasm` in your
terminal.

## Usage

Basic usage can be shown by running `recasm -h`.

An example is
`recasm -i inst.toml test.asm -o out.txt`
- `-i` specifies the instruction file
- `test.asm` is the input file
- `-o` can be used to specify a output file otherwise it is `out.txt` by default

### Instructions

The assembler needs a toml file that defines the instructions which is passed
using the `-i` flag

An example toml file to specify instructions:
```toml
[and]
opcode = 1
imm = true
reg = true
dir = false
inh = false

[or]
opcode = 2
imm = true
reg = true
```

`and` is the instruction mnemonic.

Immediate (`imm`), Register (`reg`), Direct (`dir`) and Inherent (`inh`) can
be set to true to allow the assembler to generate the instruction with that
addressing mode. If the addressing mode is not specified it is assumed to be
false (and the assembler will throw a error for that addressing mode).

### Assembler Syntax

Instructions are on separate lines. 

Operands are prefixed with:
- Register "`r`"
- Immediate "`#`"
- Direct "`$`"

### Output

The output is a text file containing each instruction in hexadecimal on separate
lines so that you can chuck it in C or VHDL.

Example:
```
nop
add r1 #1
add r1 #1
add r1 r1
or r2 r1
and r1 #0
```
