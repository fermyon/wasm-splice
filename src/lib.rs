use std::io::Write;

use anyhow::{ensure, Result};
use wasm_encoder::{Encode, SectionId};
use wasmparser::{BinaryReader, CustomSectionReader};

pub struct ExternalSection<'a> {
    pub section_id: u8,
    pub prefix: &'a [u8],
    pub external_size: u32,
    pub digest_algo: &'a str,
    pub digest_data: &'a [u8],
}

impl<'a> ExternalSection<'a> {
    pub const CUSTOM_SECTION_NAME: &str = "@external-section";

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(512);

        data.push(self.section_id);
        self.prefix.encode(&mut data);
        self.external_size.encode(&mut data);
        self.digest_algo.encode(&mut data);
        self.digest_data.encode(&mut data);

        data
    }

    pub fn write_custom_section(&self, writer: impl Write) -> std::io::Result<usize> {
        let bytes = self.to_bytes();
        write_custom_section(writer, Self::CUSTOM_SECTION_NAME, &bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<ExternalSection> {
        let mut reader = BinaryReader::new(bytes);

        let section_id = reader.read_u8()?;
        let prefix = read_u32_len_bytes(&mut reader)?;
        let external_size = reader.read_u32()?;
        let digest_algo = reader.read_string()?;
        let digest_data = read_u32_len_bytes(&mut reader)?;

        ensure!(reader.eof(), "unexpected trailing data");

        Ok(ExternalSection {
            section_id,
            prefix,
            external_size,
            digest_algo,
            digest_data,
        })
    }

    pub fn from_custom_section(section: CustomSectionReader) -> Result<ExternalSection> {
        ensure!(
            section.name() == Self::CUSTOM_SECTION_NAME,
            "not an external section!"
        );
        Self::from_bytes(section.data())
    }
}

fn read_u32_len_bytes<'a>(reader: &mut BinaryReader<'a>) -> Result<&'a [u8]> {
    let len = reader.read_u32()?;
    Ok(reader.read_bytes(len as usize)?)
}

fn write_custom_section(mut writer: impl Write, name: &str, data: &[u8]) -> std::io::Result<usize> {
    let mut name_bytes = vec![];
    name.encode(&mut name_bytes);

    let mut section_header = vec![SectionId::Custom as u8];
    let section_size = name_bytes.len() + data.len();
    section_size.encode(&mut section_header);

    writer.write_all(&section_header)?;
    writer.write_all(&name_bytes)?;
    writer.write_all(data)?;
    Ok(section_header.len() + name_bytes.len() + data.len())
}
