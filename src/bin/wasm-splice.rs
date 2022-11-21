use std::{
    env,
    fs::File,
    io::{self, Write},
    path::PathBuf,
};

use anyhow::{ensure, Context, Result};
use wasm_splice::{transform_custom_sections, write_section_header, ExternalSection, SpliceConfig};

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

    transform_custom_sections(
        input_path,
        &mut output,
        |name| name == ExternalSection::CUSTOM_SECTION_NAME,
        |reader, mut output| {
            let external =
                ExternalSection::from_custom_section(reader).context("invalid external section")?;
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
            write_section_header(&mut output, external.section_id, payload_size)
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
