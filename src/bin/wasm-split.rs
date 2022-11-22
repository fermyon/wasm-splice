use std::{env, fs::File, io::Write, path::PathBuf};

use anyhow::{bail, ensure, Context, Result};
use sha2::{Digest, Sha256};
use wasm_encoder::{Encode, SectionId};
use wasm_splice::{transform_sections, ExternalSection, SpliceConfig, EXTERNAL_SECTION_LAYER_BIT};
use wasmparser::Payload;

fn main() -> Result<()> {
    let mut args = env::args_os();
    let arg0 = args.next().unwrap();
    ensure!(
        args.len() > 1,
        "invalid arguments\nUsage: {} INPUT custom:NAME...",
        arg0.to_string_lossy()
    );

    // Input
    let input_path: PathBuf = args.next().unwrap().into();
    let input = std::fs::read(&input_path)
        .with_context(|| format!("Couldn't read input {input_path:?}"))?;

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
        &input,
        &mut output,
        |payload| match payload {
            Payload::Version { range, .. } => range.start == 0,
            Payload::CustomSection(reader) => custom_section_names.contains(&reader.name()),
            _ => false,
        },
        |payload, output| {
            match payload {
                Payload::Version { num, .. } => {
                    // Update the layer field of the version to one of the "uses external sections"
                    // layers by setting the appropriate bit. OK if already set.
                    let new_version = num | EXTERNAL_SECTION_LAYER_BIT;

                    // Write updated preamble to output
                    let mut preamble = b"\0asm".to_vec();
                    preamble.extend(new_version.to_le_bytes());
                    output.write_all(&preamble)?;
                }

                Payload::CustomSection(reader) => {
                    eprintln!("Matched custom section {:?}", reader.name());

                    // Calculate digest of section content
                    let digest = Sha256::digest(reader.data());

                    // Write section content to external section file
                    let path = config.external_section_path(digest)?;
                    std::fs::write(&path, reader.data()).with_context(|| {
                        format!("failed to write section content file {path:?}")
                    })?;
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
                }

                _ => panic!("unexpected payload type: {payload:?}"),
            };
            Ok(())
        },
    )?;

    eprintln!("Wrote split wasm to {:?}", output_path);
    Ok(())
}
