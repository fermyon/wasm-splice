use std::{
    env,
    fs::File,
    io::{self, Write},
    path::PathBuf,
};

use anyhow::{ensure, Context, Result};
use wasm_splice::{transform_sections, write_section_header, ExternalSection, SpliceConfig};
use wasmparser::Payload;

fn main() -> Result<()> {
    let mut args = env::args_os();
    let arg0 = args.next().unwrap();
    ensure!(
        args.len() == 1,
        "invalid arguments\nUsage: {} INPUT",
        arg0.to_string_lossy()
    );

    // Input path
    let input_path: PathBuf = args.next().unwrap().into();

    // Output file
    let output_path = input_path.with_extension("wasm-spliced");
    let mut output = File::create(&output_path)
        .with_context(|| format!("couldn't create output {}", output_path.display()))?;

    let config = SpliceConfig::default();

    transform_sections(
        input_path,
        &mut output,
        |payload| match payload {
            Payload::UnknownSection { id, range, .. } if id == &ExternalSection::SECTION_ID => {
                Some(range.clone())
            }
            _ => None,
        },
        |payload, mut output| {
            let Payload::UnknownSection { contents, .. } = payload else {
                unreachable!("Payload type changed?");
            };

            // Deserialize external section
            let external =
                ExternalSection::from_bytes(contents).context("invalid external section")?;
            eprintln!("Found external section: {external:#?}\n");

            // Check digest algo
            let algo = external.digest_algo;
            ensure!(algo == "sha256", "unknown digest algorithm {algo:?}",);

            // Open external section file (ensuring it exists)
            let path = config.external_section_path(external.digest_data)?;
            let mut file = File::open(&path)
                .with_context(|| format!("couldn't open external section {path:?}"))?;

            // Write section header
            let payload_size = external.prefix.len() + external.external_size as usize;
            write_section_header(&mut output, external.external_section_id, payload_size)
                .context("failed to write section header")?;

            // Write prefix
            output
                .write_all(external.prefix)
                .context("failed tp write prefix")?;

            // Copy external section to output
            let copied =
                io::copy(&mut file, output).context("failed to copy external section data")?;
            let external_size = external.external_size as u64;
            ensure!(
                copied == external_size,
                "external section data size {copied} != expected {external_size}"
            );

            Ok(())
        },
    )?;

    eprintln!("Wrote spliced wasm to {:?}", output_path);
    Ok(())
}
