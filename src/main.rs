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
    operand: u16,
}

/// Opcode as specified in the Instruction file
#[derive(Debug, serde::Deserialize)]
struct Opcode {
    opcode: u8,
    args: u8,
    inh: Option<bool>,
    imm: Option<bool>,
    reg: Option<bool>,
    dir: Option<bool>,
    operand_as_reg: Option<u8>,
}

/// Enums for each ReCOP addressing mode
#[derive(Debug, PartialEq, Clone, Copy)]
enum AddrMode {
    Inh = 0,
    Imm = 1,
    Dir = 2,
    Reg = 3,
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
    OpcodeExpected,
    OpcodeUndefined,
    RegExpected,
    OperandExpected,
    ArgsNumber,
    ImmInvalid,
    RegInvalid,
    DirInvalid,
    InhInvalid,
    ArgParse,
}

/// Error with line and contents of line
struct AsmError {
    err: Error,
    token: Token,
}

impl AsmError {
    /// Creates a new `AsmError`
    fn new(err: Error, token: Token) -> Self {
        Self { err, token }
    }
}

impl std::fmt::Display for Error {
    /// Error message output
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::OpcodeExpected => write!(f, "expected an opcode"),
            Error::OpcodeUndefined => write!(f, "opcode is undefined"),
            Error::RegExpected => write!(f, "expected a register"),
            Error::OperandExpected => write!(f, "expected an operand (register/immediate/direct)"),
            Error::ArgsNumber => write!(f, "opcode expects a different number of arguments"),
            Error::ImmInvalid => write!(f, "opcode does not have immediate addressing mode"),
            Error::RegInvalid => write!(f, "opcode does not have register addressing mode"),
            Error::DirInvalid => write!(f, "opcode does not have direct addressing mode"),
            Error::InhInvalid => write!(f, "opcode does not have inherent addressing mode"),
            Error::ArgParse => write!(f, "failed to parse argument"),
        }
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
enum TokenType {
    Label,
    Opcode,
    Reg,
    Imm,
    Dir,
}

#[derive(Debug, Clone)]
struct Token {
    token: TokenType,
    content: String,
    line_num: u32,
    chars: (u32, u32),
}

impl Token {
    fn new(token: TokenType, content: String, line_num: u32, chars: (u32, u32)) -> Self {
        Self {
            token,
            content,
            line_num,
            chars,
        }
    }
}

/// CLI args
use clap::Parser;
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, value_name = "FILE")]
    output: Option<String>,
    #[arg(short, long, value_name = "FILE")]
    instructions: String,
    input: String,
    #[arg(short, long, value_name = "FILE")]
    mif_output: Option<String>,
    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    debug: bool,
}

fn main() -> Result<(), ()> {
    let args = Args::parse();

    let opcode_map = match import_toml::<Opcode>(&args.instructions) {
        Err(_) => {
            println!("Could not parse instructions in {}", args.instructions);
            return Err(());
        }
        Ok(value) => value,
    };

    if args.debug {
        println!("{opcode_map:?}")
    }

    let input = match fs::read_to_string(&args.input) {
        Ok(value) => value,
        Err(_) => {
            println!("Input file {} could not be read", args.input);
            return Err(());
        }
    };

    let instructions = match assemble(&input, opcode_map, args.debug) {
        Err(error) => {
            let lines: Vec<_> = input.trim().split("\n").map(|x| x.trim()).collect();
            let _ = execute!(
                std::io::stdout(),
                SetForegroundColor(Color::Red),
                SetAttribute(Attribute::Bold),
                Print("error"),
                SetForegroundColor(Color::Reset),
                Print(": "),
                Print(format!("{}\n", error.err)),
                SetAttribute(Attribute::Reset),
                Print(format!(
                    " {:>2} │ {}\n",
                    error.token.line_num, lines[error.token.line_num as usize]
                ))
            );
            return Err(());
        }
        Ok(value) => value,
    };

    let inst: Vec<u32> = instructions
        .iter()
        .map(|x| x.to_bits().unwrap().load_be())
        .collect();
    let inst_hex: String = inst.iter().map(|x| format!("{x:08x}\n")).collect();

    let output_path = args.output.unwrap_or("out.txt".to_string());
    match fs::write(&output_path, inst_hex) {
        Err(_) => println!("Could not write to file {}", output_path),
        Ok(_) => {}
    };

    let mut inst_mif =
        "DEPTH = 32768;\nWIDTH = 32;\nADDRESS_RADIX = HEX;\nDATA_RADIX = HEX;\nCONTENT\nBEGIN\n\n"
            .to_string();
    let inst_mif_hex: String = inst
        .iter()
        .enumerate()
        .map(|(addr, data)| format!("{addr:04x}: {data:08x};\n"))
        .collect();
    inst_mif.push_str(&inst_mif_hex);
    inst_mif.push_str("\nEND;\n");
    let output_path = args.mif_output.unwrap_or("out.mif".to_string());
    match fs::write(&output_path, inst_mif) {
        Err(_) => println!("Could not write to file {}", output_path),
        Ok(_) => {}
    };

    let _ = execute!(
        std::io::stdout(),
        SetForegroundColor(Color::DarkGreen),
        SetAttribute(Attribute::Bold),
        Print("assembled"),
        SetForegroundColor(Color::Reset),
        Print(": "),
        SetAttribute(Attribute::Reset),
        Print(format!("wrote output to {}\n", output_path))
    );

    Ok(())
}

fn assemble(
    input: &String,
    opcodes: HashMap<String, Opcode>,
    debug: bool,
) -> Result<Vec<Inst>, AsmError> {
    let input = input.to_lowercase();
    let (tokens, labels) = lex(input);
    if debug {
        for (line, token) in tokens.iter().enumerate() {
            println!("{line} | {token:?}");
        }
        for label in &labels {
            println!("{} {}", label.0, label.1);
        }
    }
    parse(tokens, opcodes, labels)
}

fn lex(input: String) -> (Vec<Vec<Token>>, HashMap<String, u16>) {
    let lines: Vec<_> = input.trim().split("\n").map(|x| x.trim()).collect();
    let mut tokens: Vec<Vec<Token>> = Vec::new();
    let mut labels: HashMap<String, u16> = HashMap::new();

    'line_loop: for (i, line) in lines.into_iter().enumerate() {
        let mut line_tokens: Vec<Token> = Vec::new();
        if line.len() == 0 {
            continue;
        }
        let symbols: Vec<Vec<char>> = line
            .split(" ")
            .map(|x| x.trim().chars().collect())
            .collect();

        'tokenize_line: for symbol in symbols {
            let (token_type, content) = match symbol[0] {
                ';' => break 'tokenize_line, // comment
                'r' => (TokenType::Reg, symbol[1..].iter().collect()),
                '#' => (TokenType::Imm, symbol[1..].iter().collect()),
                '$' => (TokenType::Dir, symbol[1..].iter().collect()),
                '\'' => (TokenType::Label, symbol[1..].iter().collect()),
                _ => {
                    if symbol[symbol.len() - 1] == ':' {
                        // Token::new(TokenType::Label, symbol.iter().collect())
                        labels.insert(
                            symbol[..symbol.len() - 1].iter().collect(),
                            tokens.len() as u16,
                        );
                        continue 'line_loop;
                    } else {
                        (TokenType::Opcode, symbol.iter().collect())
                    }
                }
            };
            line_tokens.push(Token::new(token_type, content, i as u32, (0, 0)));
        }

        if line_tokens.len() > 0 {
            tokens.push(line_tokens);
        }
    }

    (tokens, labels)
}

fn parse_addr_mode_operand(
    operand_token: &TokenType,
    opcode: &Opcode,
    i: usize,
) -> Result<AddrMode, Error> {
    Ok(match operand_token {
        TokenType::Reg => {
            if opcode.reg.unwrap_or(false) {
                AddrMode::Reg
            } else {
                return Err(Error::RegInvalid);
            }
        }
        TokenType::Imm | TokenType::Label => {
            if opcode.imm.unwrap_or(false) {
                AddrMode::Imm
            } else {
                return Err(Error::ImmInvalid);
            }
        }
        TokenType::Dir => {
            if opcode.dir.unwrap_or(false) {
                AddrMode::Dir
            } else {
                return Err(Error::DirInvalid);
            }
        }
        TokenType::Opcode => {
            if opcode.inh.unwrap_or(false) {
                AddrMode::Inh
            } else {
                return Err(Error::InhInvalid);
            }
        } // _ => return Err(AsmError::new(Error::OperandExpected, i)),
    })
}

fn parse(
    tokens: Vec<Vec<Token>>,
    opcodes: HashMap<String, Opcode>,
    labels: HashMap<String, u16>,
) -> Result<Vec<Inst>, AsmError> {
    let mut instructions: Vec<Inst> = Vec::new();

    for (i, line) in tokens.iter().enumerate() {
        let opcode = if let TokenType::Opcode = &line[0].token {
            if let Some(value) = opcodes.get(&line[0].content) {
                value
            } else {
                return Err(AsmError::new(Error::OpcodeUndefined, line[0].clone()));
            }
        } else {
            return Err(AsmError::new(Error::OpcodeExpected, line[0].clone()));
        };

        // arg check
        if opcode.args as usize != line.len() - 1 {
            return Err(AsmError::new(Error::ArgsNumber, line[0].clone()));
        }

        // addr mode check
        let operand = &line[line.len() - 1];
        let addr_mode = match parse_addr_mode_operand(&operand.token, opcode, i) {
            Err(err) => return Err(AsmError::new(err, operand.clone())),
            Ok(addr_mode) => addr_mode,
        };
        // operand parse
        let mut operand = match addr_mode {
            AddrMode::Inh => 0,
            _ => {
                if operand.token == TokenType::Label {
                    *labels.get(&operand.content).unwrap()
                } else {
                    if let Ok(value) = operand.content.parse() {
                        value
                    } else {
                        return Err(AsmError::new(Error::ArgParse, operand.clone()));
                    }
                }
            }
        };

        // other args must be regs
        let mut reg_args = [0u8; 2];
        for (j, arg) in (1..(line.len() - 1)).enumerate() {
            if let TokenType::Reg = line[arg].token {
                reg_args[j] = match line[arg].content.parse() {
                    Ok(value) => value,
                    Err(_) => return Err(AsmError::new(Error::ArgParse, line[arg].clone())),
                };
            } else {
                return Err(AsmError::new(Error::RegExpected, line[arg].clone()));
            }
        }

        if addr_mode == AddrMode::Reg && opcode.args < 3 {
            if let Some(value) = opcode.operand_as_reg {
                reg_args[value as usize] = operand as u8;
                operand = 0;
            }
        }

        instructions.push(Inst {
            addr_mode: addr_mode as u8,
            opcode: opcode.opcode,
            reg_z: reg_args[0],
            reg_x: reg_args[1],
            operand,
        });
    }
    Ok(instructions)
}
