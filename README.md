# psx-vcd

A command-line tool to convert PlayStation 1 game images (BIN/CUE format) to VCD format compatible with POPSTARTER and Open PS2 Loader (OPL) on PlayStation 2.

## Features

- **Automatic conversion**: Combines multi-track BIN files and converts them to VCD format in one step
- **Game ID detection**: Automatically detects and includes PlayStation game serial numbers
- **cue2pops v2.0 compatible**: Generates VCD files matching the original cue2pops behavior
- **Multi-file support**: Handles both single-file and multi-file CUE sheets
- **Gap adjustment**: Optional gap+/gap- flags for problematic discs
- **Clean filenames**: Automatically removes regional tags and metadata from output files

## Installation

### From crates.io

```bash
cargo install psx-vcd
```

### Build from source

```bash
git clone https://github.com/yourusername/psx-vcd
cd psx-vcd
cargo build --release
```

The binary will be available at `target/release/psx-vcd`

## Usage

### Auto mode (recommended)

Convert a CUE file to VCD format automatically:

```bash
psx-vcd auto game.cue
```

With custom output directory:

```bash
psx-vcd auto game.cue -o /path/to/output
```

With verbose output:

```bash
psx-vcd auto game.cue -v
```

### Combine mode

Only combine multiple BIN files into one:

```bash
psx-vcd combine game.cue
```

This creates a combined BIN and a new CUE file that can be used with other tools.

### Convert mode

Convert an already combined BIN to VCD:

```bash
psx-vcd convert game.bin --cue game.cue
```

### Detect mode

Detect the Game ID from a BIN or CUE file:

```bash
psx-vcd detect game.cue
```

With verbose information:

```bash
psx-vcd detect game.cue -v
```

Debug mode to see all potential IDs found:

```bash
psx-vcd detect game.cue --debug
```

## Gap Adjustment

Some games may require gap adjustment for proper operation:

- `--gap-plus`: Add 2 seconds to track indexes (useful for some problem discs)
- `--gap-minus`: Subtract 2 seconds from track indexes (rarely needed)

Example:

```bash
psx-vcd auto game.cue --gap-plus
```

## Output

The tool generates VCD files with the following naming format:

```
SLUS_XXX.XX.GameName.VCD
```

Where:
- `SLUS_XXX.XX` is the detected Game ID
- `GameName` is the cleaned game name

## Supported Formats

- **Input**: CUE/BIN files (MODE2/2352 tracks required)
- **Output**: VCD files compatible with POPSTARTER/OPL

## Technical Details

This tool implements the cue2pops v2.0 VCD format specification:
- 1MB VCD header with TOC (Table of Contents) information
- Proper MSF (Minutes:Seconds:Frames) calculation for multi-track games
- BCD (Binary-Coded Decimal) encoding for time values
- Support for pregap and postgap handling

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Acknowledgments

- **tallero** - [cue2pops](https://github.com/tallero/cue2pops-linux)
- **ADBeta** - [psx-combine](https://github.com/ADBeta/psx-comBINe)
- The POPSTARTER and OPL communities for their continued support
