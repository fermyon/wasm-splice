use std::{env, fs::File, io::Write, path::PathBuf};

use anyhow::{bail, ensure, Context, Result};
use sha2::{Digest, Sha256};
use wasm_encoder::{Encode, SectionId};
use wasm_splice::ExternalSection;
use wasmparser::{Parser, Payload};

fn main() -> Result<()> {
    let mut args = env::args_os();
    let arg0 = args.next().unwrap();
    ensure!(
        args.len() > 1,
        "invalid arguments\nUsage: {} INPUT custom:NAME...",
        arg0.to_string_lossy()
    );

    // Input file
    let input_path: PathBuf = args.next().unwrap().into();
    let input = std::fs::read(&input_path)
        .with_context(|| format!("Couldn't read input {}", input_path.display()))?;
    let mut consumed = 0;

    // Parse section specs
    let section_specs = args
        .map(|arg| Ok(arg.to_str().context("invalid UTF-8")?.to_string()))
        .collect::<Result<Vec<String>>>()?;
    let custom_section_names: Vec<&str> = section_specs
        .iter()
        .map(|spec| {
            if let Some(name) = spec.strip_prefix("custom:") {
                Ok(name)
            } else {
                bail!("Invalid section spec {spec:?} (only 'custom:NAME' currently supported)");
            }
        })
        .collect::<Result<_>>()?;

    // Output file
    let output_path = input_path.with_extension("wasmx");
    let mut output = File::create(&output_path)
        .with_context(|| format!("Couldn't create output {}", output_path.display()))?;

    let parser = Parser::new(0);
    for payload in parser.parse_all(&input) {
        match payload? {
            Payload::CustomSection(reader) if custom_section_names.contains(&reader.name()) => {
                // Copy up to the beginning of this section
                // FIXME: This is terrible; probably shouldn't use Parser::parse_all
                let section_size_len =
                    leb128::write::unsigned(&mut std::io::sink(), reader.range().len() as u64)
                        .unwrap();
                let section_start = reader.range().start - 1 - section_size_len;
                output.write_all(&input[consumed..section_start])?;
                consumed = reader.range().end;

                // Calculate digest of section content
                let digest = Sha256::digest(reader.data());

                // Write section content to "fragment" file
                let fragment_filename = format!("{digest:x}.ext");
                std::fs::write(&fragment_filename, reader.data()).with_context(|| {
                    format!("Failed to write section content file {fragment_filename:?}")
                })?;

                // Write external section to output
                let mut prefix = vec![];
                reader.name().encode(&mut prefix);
                let external = ExternalSection {
                    section_id: SectionId::Custom as u8,
                    prefix: &prefix,
                    external_size: reader.data().len() as u32,
                    digest_algo: "sha256",
                    digest_data: digest.as_slice(),
                };
                external.write_custom_section(&mut output)?;
            }
            _ => (),
        };
    }

    // Write the remainder
    output.write_all(&input[consumed..])?;

    Ok(())
}
