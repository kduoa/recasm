use bitvec::field::BitField;
use crossterm::execute;
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use deku::prelude::*;
use std::collections::HashMap;
use std::fs;

/// ReCOP Instruction definition
#[derive(PartialEq, Debug, DekuRead, DekuWrite)]
#[deku(endian = "big")]
struct Inst {
    #[deku(bits = 2)]
    addr_mode: u8,
    #[deku(bits = 6)]
    opcode: u8,
    #[deku(bits = 4)]
    reg_z: u8,
    #[deku(bits = 4)]
    reg_x: u8,
    operand: i16,
}

/// Opcode as specified in the Instruction file
#[derive(Debug, serde::Deserialize)]
struct Opcode {
    opcode: u8,
    inh: Option<bool>,
    imm: Option<bool>,
    reg: Option<bool>,
    dir: Option<bool>,
}

/// Enums for each ReCOP addressing mode
#[derive(Debug, PartialEq, Clone, Copy)]
enum AddrMode {
    Inh = 0,
    Imm = 1,
    Reg = 2,
    Dir = 3,
}

/// Overly complicated generic function to import toml files
pub fn import_toml<T: serde::de::DeserializeOwned>(
    path: &str,
) -> anyhow::Result<HashMap<String, T>> {
    let mut tiles: HashMap<String, T> = HashMap::new();
    let table: toml::Table = std::fs::read_to_string(path)?.parse::<toml::Table>()?;
    let tile_names = table.keys().collect::<Vec<_>>();
    for tile_name in tile_names.into_iter() {
        let tile: T = toml::from_str(&toml::to_string(&table[tile_name])?)?;
        tiles.insert(tile_name.clone(), tile);
    }
    Ok(tiles)
}

/// Possible assembler errors
#[derive(Debug)]
enum Error {
    Read,
    Opcode,
    AddrMode,
    Arg2NotReg,
    InvalidReg,
    OperandParse,
}

/// Error with line and contents of line
struct AsmError<'a> {
    err: Error,
    line: u32,
    contents: &'a str,
}

impl<'a> AsmError<'a> {
    /// Creates a new `AsmError`
    fn new(err: Error, line: u32, contents: &'a str) -> Self {
        Self {
            err,
            line,
            contents,
        }
    }
}

impl std::fmt::Display for Error {
    /// Error message output
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Read => write!(f, "failed to read file"),
            Error::Opcode => write!(f, "opcode cannot be recognised"),
            Error::AddrMode => write!(f, "addressing mode cannot be inferred"),
            Error::Arg2NotReg => write!(f, "first argument must be a register"),
            Error::InvalidReg => write!(f, "invalid register"),
            Error::OperandParse => write!(f, "operand could not be parsed"),
        }
    }
}

/// CLI args
use clap::Parser;
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    output: Option<String>,
    #[arg(short, long)]
    instructions: String,
    input: String,
}

fn main() {
    let args = Args::parse();

    let opcode_map = match import_toml::<Opcode>(&args.instructions) {
        Err(_) => {
            println!("Could not parse instructions in {}", args.instructions);
            return;
        }
        Ok(value) => value,
    };

    let input = match fs::read_to_string(&args.input) {
        Ok(value) => value,
        Err(_) => {
            println!("Input file {} could not be read", args.input);
            return;
        }
    };

    let input = input.to_lowercase();
    let input: Vec<_> = input.trim().split("\n").collect();

    let instructions = match assemble(input, opcode_map) {
        Ok(value) => value,
        Err(error) => {
            let _ = execute!(
                std::io::stdout(),
                SetForegroundColor(Color::Red),
                SetAttribute(Attribute::Bold),
                Print("error"),
                SetForegroundColor(Color::Reset),
                Print(": "),
                Print(format!("{}\n", error.err)),
                SetAttribute(Attribute::Reset),
                Print(format!(" {:>2} │ {}\n", error.line, error.contents))
            );
            return;
        }
    };

    let inst: Vec<u32> = instructions
        .iter()
        .map(|x| x.to_bits().unwrap().load_be())
        .collect();
    let inst: String = inst.iter().map(|x| format!("{x:08x}\n")).collect();

    let output_path = args.output.unwrap_or("out.txt".to_string());
    match fs::write(&output_path, inst) {
        Err(_) => println!("Could not write to file {}", output_path),
        Ok(_) => {}
    };
}

fn assemble(input: Vec<&str>, opcodes: HashMap<String, Opcode>) -> Result<Vec<Inst>, AsmError> {
    let mut instructions: Vec<Inst> = Vec::new();
    for (i, line) in input.iter().enumerate() {
        let symbols: Vec<_> = line.trim().split(" ").collect();

        let opcode = match opcodes.get(symbols[0]) {
            Some(value) => value,
            None => {
                return Err(AsmError::new(Error::Opcode, i as u32, line));
            }
        };

        // decipher
        let addr_mode = if symbols.len() == 1 && opcode.inh.unwrap_or(false) {
            // inherent
            AddrMode::Inh
        } else if symbols.len() == 3 {
            let arg3: Vec<_> = symbols[2].to_lowercase().chars().collect();
            let arg2: Vec<_> = symbols[1].to_lowercase().chars().collect();
            if arg2[0] != 'r' {
                return Err(AsmError::new(Error::Arg2NotReg, i as u32, line));
            }
            match arg3[0] {
                'r' => AddrMode::Reg,
                '#' => AddrMode::Imm,
                '$' => AddrMode::Dir,
                _ => {
                    return Err(AsmError::new(Error::AddrMode, i as u32, line));
                }
            }
        } else {
            return Err(AsmError::new(Error::AddrMode, i as u32, line));
        };

        let (reg_z, op2): (u8, i16) = if addr_mode != AddrMode::Inh {
            let arg3: Vec<_> = symbols[2].to_lowercase().chars().collect();
            let arg2: Vec<_> = symbols[1].to_lowercase().chars().collect();
            let reg_z: String = arg2[1..].iter().collect();
            let reg_z = if let Ok(value) = reg_z.parse() {
                value
            } else {
                return Err(AsmError::new(Error::InvalidReg, i as u32, line));
            };
            let op2: String = arg3[1..].iter().collect();
            let op2 = if let Ok(value) = op2.parse() {
                value
            } else {
                return Err(AsmError::new(Error::OperandParse, i as u32, line));
            };
            (reg_z, op2)
        } else {
            (0, 0)
        };

        let mut inst = Inst {
            addr_mode: addr_mode as u8,
            opcode: opcode.opcode,
            reg_z,
            reg_x: 0,
            operand: 0,
        };

        match addr_mode {
            AddrMode::Reg => inst.reg_x = op2 as u8,
            AddrMode::Imm | AddrMode::Dir => inst.operand = op2,
            AddrMode::Inh => {}
        }

        instructions.push(inst);
    }

    Ok(instructions)
}