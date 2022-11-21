# Wasm External Sections Prototype

## Split

Split external sections from the given Wasm binary matching the given names:

```console
$ wasm-split test.wasm custom:.debug_info custom:producers
Matched custom section ".debug_info"
Wrote external section data to "2499bc813ade104035ea2f2974635b18891221231dea6645309ccf1fdd773bfe.ext"
Split external section: ExternalSection {
    section_id: 0,
    prefix: "\\x0b.debug_info",
    external_size: 446921,
    digest_algo: "sha256",
    digest_data: "2499bc813ade104035ea2f2974635b18891221231dea6645309ccf1fdd773bfe",
}

Matched custom section "producers"
Wrote external section data to "925ad151ad28402856af66f293fbbb9c5ef04fc4cb7edf47a2cf97eab5f6bbd6.ext"
Split external section: ExternalSection {
    section_id: 0,
    prefix: "\\tproducers",
    external_size: 88,
    digest_algo: "sha256",
    digest_data: "925ad151ad28402856af66f293fbbb9c5ef04fc4cb7edf47a2cf97eab5f6bbd6",
}

Wrote split wasm to "test.wasm-split"

$ stat --printf "%s\t%n\n" test.wasm* *.ext
4058631 test.wasm
3611744 test.wasm-split
446921  2499bc813ade104035ea2f2974635b18891221231dea6645309ccf1fdd773bfe.ext
88      925ad151ad28402856af66f293fbbb9c5ef04fc4cb7edf47a2cf97eab5f6bbd6.ext

$ strings 925ad151ad28402856af66f293fbbb9c5ef04fc4cb7edf47a2cf97eab5f6bbd6.ext  # "producers"
language
Rust
processed-by
rustc%1.63.0-nightly (b2eed72a6 2022-05-22)
clang
14.0.0
```

## Splice

Splice external sections back into a split wasm file:

```console
$ target/release/wasm-splice test.wasm-split
Found external section: ExternalSection {
    section_id: 0,
    prefix: "\\x0b.debug_info",
    external_size: 446921,
    digest_algo: "sha256",
    digest_data: "2499bc813ade104035ea2f2974635b18891221231dea6645309ccf1fdd773bfe",
}

Found external section: ExternalSection {
    section_id: 0,
    prefix: "\\tproducers",
    external_size: 88,
    digest_algo: "sha256",
    digest_data: "925ad151ad28402856af66f293fbbb9c5ef04fc4cb7edf47a2cf97eab5f6bbd6",
}

Wrote spliced wasm to "test.wasm-spliced"

$ sha1sum test.wasm{,-spliced}
9a3d76bed22285619de0a0c3148ec78b5d1de958  test.wasm
9a3d76bed22285619de0a0c3148ec78b5d1de958  test.wasm-spliced
```
