//! `sf2-to-sf3` — convert a SoundFont bank to the Ogg/Vorbis-compressed format.

use std::process::ExitCode;

use rameau_soundfont_convert::{Quality, convert_file};

const USAGE: &str = "\
usage: sf2-to-sf3 <input.sf2> <output.sf3> [-q <quality>]

  -q, --quality <q>   Ogg/Vorbis VBR quality, -0.2 (smallest) to 1.0 (best).
                      Defaults to 0.5.
  -h, --help          Show this message.
";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("sf2-to-sf3: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut positional = Vec::new();
    let mut quality = Quality::default();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            "-q" | "--quality" => {
                let value = args.next().ok_or("-q needs a value")?;
                let parsed: f32 = value
                    .parse()
                    .map_err(|_| format!("not a number: {value}"))?;
                quality = Quality::new(parsed);
            }
            other => positional.push(other.to_string()),
        }
    }

    let [input, output] = positional.as_slice() else {
        return Err(format!("expected an input and an output path\n\n{USAGE}"));
    };

    convert_file(input, output, quality).map_err(|e| format!("{input}: {e}"))?;

    let before = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
    let after = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    if before > 0 && after > 0 {
        let mib = |n: u64| n as f64 / (1024.0 * 1024.0);
        println!(
            "{input} -> {output}  ({:.1} MiB -> {:.1} MiB, {:.0}% smaller, quality {})",
            mib(before),
            mib(after),
            100.0 - (after as f64 / before as f64 * 100.0),
            quality.get(),
        );
    }
    Ok(())
}
