use fortytwo_lang::lexer::Lexer;
use fortytwo_lang::position_reader::PositionReader;
use miette::NamedSource;
use std::env;
use std::io::{stdin, Read};
use std::sync::Arc;

fn main() -> miette::Result<()> {
    let args: Vec<_> = env::args().collect();
    match args.get(1) {
        Some(_) => show_help(),
        _ => lexer_from_stdin()?,
    }
    Ok(())
}

fn lexer_from_stdin() -> miette::Result<()> {
    let mut sourcecode = String::new();
    stdin()
        .read_to_string(&mut sourcecode)
        .expect("Could not read sourcecode from stdin");
    let named_source = Arc::new(NamedSource::new("stdin", sourcecode.clone()));
    let position_reader = PositionReader::new(sourcecode.chars());
    let lexer = Lexer::new(position_reader, named_source);
    for token in lexer {
        println!("{:?}", token?);
    }
    Ok(())
}

fn show_help() {
    println!(
        r#"FORTYTWO-LANG LEXER DUMP
Dumps the output of the lexer.
Write your ftl sourcecode to stdin and end stdin by pressing CTRL+C.

USAGE:
    ftlld"#
    )
}
