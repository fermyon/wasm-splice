use std::{env, fs::File, path::PathBuf};

use anyhow::{bail, ensure, Context, Result};
use sha2::{Digest, Sha256};
use wasm_encoder::{Encode, SectionId};
use wasm_splice::{transform_sections, ExternalSection, SpliceConfig};
use wasmparser::Payload;

fn main() -> Result<()> {
    let mut args = env::args_os();
    let arg0 = args.next().unwrap();
    ensure!(
        args.len() > 1,
        "invalid arguments\nUsage: {} INPUT custom:NAME...",
        arg0.to_string_lossy()
    );

    // Input path
    let input_path: PathBuf = args.next().unwrap().into();

    // Section spec(s)
    let section_specs = args
        .map(|arg| Ok(arg.to_str().context("invalid UTF-8")?.to_string()))
        .collect::<Result<Vec<String>>>()?;
    let custom_section_names: Vec<&str> = section_specs
        .iter()
        .map(|spec| {
            if let Some(name) = spec.strip_prefix("custom:") {
                Ok(name)
            } else {
                bail!("invalid section spec {spec:?} (only 'custom:NAME' currently supported)");
            }
        })
        .collect::<Result<_>>()?;

    // Output file
    let output_path = input_path.with_extension("wasm-split");
    let mut output = File::create(&output_path)
        .with_context(|| format!("couldn't create output {}", output_path.display()))?;

    let config = SpliceConfig::default();

    transform_sections(
        input_path,
        &mut output,
        |payload| match payload {
            Payload::CustomSection(reader) if custom_section_names.contains(&reader.name()) => {
                Some(reader.range())
            }
            _ => None,
        },
        |payload, output| {
            let Payload::CustomSection(reader) = payload else {
                unreachable!("Payload type changed?");
            };

            eprintln!("Matched custom section {:?}", reader.name());

            // Calculate digest of section content
            let digest = Sha256::digest(reader.data());

            // Write section content to external section file
            let path = config.external_section_path(digest)?;
            std::fs::write(&path, reader.data())
                .with_context(|| format!("failed to write section content file {path:?}"))?;
            eprintln!("Wrote external section data to {path:?}");

            // Write external section to output
            let mut prefix = vec![];
            reader.name().encode(&mut prefix);
            let external = ExternalSection {
                external_section_id: SectionId::Custom as u8,
                prefix: &prefix,
                external_size: reader.data().len() as u32,
                digest_algo: "sha256",
                digest_data: digest.as_slice(),
            };
            external.write_section(output)?;
            eprintln!("Split external section: {external:#?}");

            eprintln!();
            Ok(())
        },
    )?;

    eprintln!("Wrote split wasm to {:?}", output_path);
    Ok(())
}
