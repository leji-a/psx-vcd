// src/main.rs
mod combiner;
mod cue;
mod utils;
mod vcd;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::Path;
use std::path::PathBuf;

use combiner::BinCombiner;
use cue::CueSheet;
use utils::detect_game_id;
use vcd::VcdConverter;

/// Automatic PSX BIN/CUE to VCD converter for OPL/POPSTARTER
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Combine and convert to VCD (complete process)
    Auto {
        /// Input CUE file
        #[arg(value_name = "INPUT.cue")]
        input: PathBuf,

        /// Output directory (default: ./psx-vcd-output/)
        #[arg(short, long, value_name = "DIR")]
        output: Option<PathBuf>,

        /// Add 2 seconds to track indexes
        #[arg(long)]
        gap_plus: bool,

        /// Subtract 2 seconds from track indexes
        #[arg(long)]
        gap_minus: bool,

        /// Display detailed CUE information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Combine BIN files only (without VCD conversion)
    Combine {
        /// Input CUE file
        #[arg(value_name = "INPUT.cue")]
        input: PathBuf,

        /// Output directory (default: ./psx-vcd-output/)
        #[arg(short, long, value_name = "DIR")]
        output: Option<PathBuf>,

        /// Output BIN filename
        #[arg(short, long, value_name = "FILE")]
        filename: Option<String>,

        /// Display detailed CUE information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Convert combined BIN to VCD only
    Convert {
        /// Input combined BIN file
        #[arg(value_name = "INPUT.bin")]
        input: PathBuf,

        /// Associated CUE file (for track information)
        #[arg(short, long, value_name = "FILE.cue")]
        cue: PathBuf,

        /// Output directory (default: same as input)
        #[arg(short, long, value_name = "DIR")]
        output: Option<PathBuf>,

        /// Output VCD filename
        #[arg(short, long, value_name = "FILE")]
        filename: Option<String>,

        /// Add 2 seconds to track indexes
        #[arg(long)]
        gap_plus: bool,

        /// Subtract 2 seconds from track indexes
        #[arg(long)]
        gap_minus: bool,
    },

    /// Detect PSX Game ID
    Detect {
        /// Input CUE or BIN file
        #[arg(value_name = "INPUT")]
        input: PathBuf,

        /// Display extended information
        #[arg(short, long)]
        verbose: bool,

        /// Debug mode: show found strings in BIN
        #[arg(short, long)]
        debug: bool,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();

    println!("PSX to VCD Converter");
    println!("====================\n");

    match args.command {
        Commands::Auto {
            input,
            output,
            gap_plus,
            gap_minus,
            verbose,
        } => run_auto_mode(input, output, gap_plus, gap_minus, verbose),
        Commands::Combine {
            input,
            output,
            filename,
            verbose,
        } => run_combine_mode(input, output, filename, verbose),
        Commands::Convert {
            input,
            cue,
            output,
            filename,
            gap_plus,
            gap_minus,
        } => run_convert_mode(input, cue, output, filename, gap_plus, gap_minus),
        Commands::Detect {
            input,
            verbose,
            debug,
        } => run_detect_mode(input, verbose, debug),
    }
}

/// Auto mode: Combine + Convert to VCD (complete process)
fn run_auto_mode(
    input: PathBuf,
    output: Option<PathBuf>,
    gap_plus: bool,
    gap_minus: bool,
    verbose: bool,
) -> Result<()> {
    validate_cue_input(&input)?;
    validate_gap_flags(gap_plus, gap_minus)?;

    println!("[*] Parsing CUE file: {}", input.display());
    let mut cue_sheet = CueSheet::parse(&input)?;

    let cue_dir = input
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine CUE directory"))?;

    cue_sheet.load_file_sizes(cue_dir)?;
    cue_sheet.validate_mode2()?;

    if verbose {
        cue_sheet.print_info();
    } else {
        println!("[+] Found {} track(s)", cue_sheet.get_total_tracks());
    }

    // Detect Game ID before combining (from first BIN)
    let first_bin = cue_dir.join(&cue_sheet.files[0].filename);
    let game_id = detect_and_print_game_id(&first_bin)?;

    // Determine output directory
    let output_dir = output.unwrap_or_else(|| cue_dir.join("psx-vcd-output"));
    std::fs::create_dir_all(&output_dir)?;
    println!("[*] Output directory: {}\n", output_dir.display());

    let game_name = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid input filename"))?;

    let clean_name = clean_game_name(game_name);

    // Step 1: Combine BINs
    println!("[*] Step 1: Combining BIN files");
    let combined_bin = output_dir.join(format!("{}_combined.bin", clean_name));
    let combine_info = BinCombiner::combine(&mut cue_sheet, cue_dir, &combined_bin)?;
    println!(
        "[+] Combined {} track(s) -> {:.2} MB\n",
        combine_info.track_count,
        combine_info.total_bytes as f64 / (1024.0 * 1024.0)
    );

    // Step 2: Convert to VCD
    println!("[*] Step 2: Converting to VCD format");
    let temp_vcd = output_dir.join(format!("{}.VCD", clean_name));
    let converter = VcdConverter::new(gap_plus, gap_minus);
    converter.convert_to_vcd(&combined_bin, &temp_vcd, &cue_sheet)?;

    // Rename with Game ID if detected
    let final_output = if let Some(id) = game_id {
        let renamed_vcd = output_dir.join(format!("{}.{}.VCD", id, clean_name));
        std::fs::rename(&temp_vcd, &renamed_vcd)?;
        renamed_vcd
    } else {
        temp_vcd
    };

    // Clean up temporary file
    let _ = std::fs::remove_file(&combined_bin);

    print_success(&final_output, gap_plus, gap_minus)?;
    Ok(())
}

/// Combine mode: BIN merging only
fn run_combine_mode(
    input: PathBuf,
    output: Option<PathBuf>,
    filename: Option<String>,
    verbose: bool,
) -> Result<()> {
    validate_cue_input(&input)?;

    println!("[*] Parsing CUE file: {}", input.display());
    let mut cue_sheet = CueSheet::parse(&input)?;

    let cue_dir = input
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine CUE directory"))?;

    cue_sheet.load_file_sizes(cue_dir)?;
    cue_sheet.validate_mode2()?;

    if verbose {
        cue_sheet.print_info();
    } else {
        println!("[+] Found {} track(s)", cue_sheet.get_total_tracks());
    }

    let first_bin = cue_dir.join(&cue_sheet.files[0].filename);
    let game_id = detect_and_print_game_id(&first_bin)?;

    let output_dir = output.unwrap_or_else(|| cue_dir.join("psx-vcd-output"));
    std::fs::create_dir_all(&output_dir)?;

    let game_name = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid input filename"))?;

    let clean_name = clean_game_name(game_name);
    let output_filename = filename.unwrap_or_else(|| format!("{}_combined.bin", clean_name));
    let combined_bin = output_dir.join(&output_filename);

    println!("\n[*] Combining BIN files");
    let combine_info = BinCombiner::combine(&mut cue_sheet, cue_dir, &combined_bin)?;

    // Generate new CUE file for the combined BIN
    println!("\n[*] Generating new CUE file...");
    let output_cue = combined_bin.with_extension("cue");

    use std::io::Write;
    let mut cue_file =
        std::fs::File::create(&output_cue).context("Failed to create output CUE file")?;

    writeln!(
        cue_file,
        "FILE \"{}\" BINARY",
        combined_bin.file_name().unwrap().to_string_lossy()
    )?;

    for file in &cue_sheet.files {
        for track in &file.tracks {
            writeln!(cue_file, "  TRACK {:02} {}", track.number, track.track_type)?;

            if let Some(idx00) = track.index00_msf {
                writeln!(cue_file, "    INDEX 00 {}", idx00)?;
            }
            writeln!(cue_file, "    INDEX 01 {}", track.index01_msf)?;
        }
    }

    cue_file.flush()?;

    println!("\n[+] BIN and CUE files created successfully!");
    println!("    BIN: {}", combined_bin.display());
    println!("    CUE: {}", output_cue.display());
    println!(
        "    Size: {:.2} MB",
        combine_info.total_bytes as f64 / (1024.0 * 1024.0)
    );
    println!("    Tracks: {}", combine_info.track_count);
    if let Some(id) = game_id {
        println!("    Game ID: {}", id);
    }
    println!();
    println!("[i] You can now use this CUE with cue2pops:");
    println!("    cue2pops \"{}\"", output_cue.display());
    println!();

    Ok(())
}

/// Convert mode: BIN to VCD conversion only
fn run_convert_mode(
    input: PathBuf,
    cue: PathBuf,
    output: Option<PathBuf>,
    filename: Option<String>,
    gap_plus: bool,
    gap_minus: bool,
) -> Result<()> {
    validate_bin_input(&input)?;
    validate_cue_input(&cue)?;
    validate_gap_flags(gap_plus, gap_minus)?;

    let game_id = detect_and_print_game_id(&input)?;

    println!("\n[*] Parsing CUE file: {}", cue.display());
    let mut cue_sheet = CueSheet::parse(&cue)?;

    let cue_dir = cue
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine CUE directory"))?;

    cue_sheet.load_file_sizes(cue_dir)?;
    cue_sheet.validate_mode2()?;
    println!("[+] Found {} track(s)", cue_sheet.get_total_tracks());

    let output_dir = output.unwrap_or_else(|| {
        input
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf()
    });
    std::fs::create_dir_all(&output_dir)?;

    let game_name = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid input filename"))?;

    let clean_name = clean_game_name(game_name);
    let output_filename = filename.unwrap_or_else(|| format!("{}.VCD", clean_name));
    let temp_vcd = output_dir.join(&output_filename);

    println!("\n[*] Converting to VCD format");
    let converter = VcdConverter::new(gap_plus, gap_minus);
    converter.convert_to_vcd(&input, &temp_vcd, &cue_sheet)?;

    let final_output = if let Some(id) = game_id {
        let renamed_vcd = output_dir.join(format!("{}.{}.VCD", id, clean_name));
        std::fs::rename(&temp_vcd, &renamed_vcd)?;
        renamed_vcd
    } else {
        temp_vcd
    };

    print_success(&final_output, gap_plus, gap_minus)?;
    Ok(())
}

/// Detect mode: Game ID detection only
fn run_detect_mode(input: PathBuf, verbose: bool, debug: bool) -> Result<()> {
    let bin_path = if let Some(ext) = input.extension() {
        let ext_str = ext.to_string_lossy().to_lowercase();

        if ext_str == "cue" {
            println!("[*] Parsing CUE file: {}", input.display());
            let cue_sheet = CueSheet::parse(&input)?;

            let cue_dir = input
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Cannot determine CUE directory"))?;

            if cue_sheet.files.is_empty() {
                bail!("CUE file contains no BIN files");
            }

            let first_bin = cue_dir.join(&cue_sheet.files[0].filename);
            println!("    Reading from: {}\n", cue_sheet.files[0].filename);
            first_bin
        } else if ext_str == "bin" {
            input.clone()
        } else {
            bail!("Input must be a .cue or .bin file");
        }
    } else {
        bail!("Input file has no extension");
    };

    if !bin_path.exists() {
        bail!("BIN file not found: {}", bin_path.display());
    }

    if debug {
        use std::fs::File;
        use std::io::Read;

        println!("[*] Debug mode: Searching for PSX patterns in BIN...\n");

        let mut file = File::open(&bin_path)?;
        let mut buffer = vec![0u8; 150 * 1024];
        let bytes_read = file.read(&mut buffer)?;
        buffer.truncate(bytes_read);

        let search_str = String::from_utf8_lossy(&buffer);

        use regex::Regex;
        let pattern = Regex::new(r"[A-Z]{4}[_\- ]\d{3}[\.\d]{3}")?;

        println!("Found potential IDs:");
        println!("----------------------------");
        let mut found_any = false;
        for caps in pattern.find_iter(&search_str) {
            println!("  {}", caps.as_str());
            found_any = true;
        }

        if !found_any {
            println!("  (none found)");
        }
        println!("----------------------------\n");
    }

    println!("[*] Detecting Game ID...");
    match detect_game_id(&bin_path)? {
        Some(game_id) => {
            if verbose {
                println!("\n[+] Game ID found!");
                println!("----------------------------");
                println!("    Game ID: {}", game_id);
                println!("    Region:  {}", get_region(&game_id));
                println!(
                    "    BIN:     {}",
                    bin_path.file_name().unwrap().to_string_lossy()
                );
                println!("----------------------------\n");
            } else {
                println!("{}", game_id);
            }
        }
        None => {
            if verbose {
                println!("\n[-] No Game ID found");
                println!("    The BIN file may be:");
                println!("    - Corrupted or incomplete");
                println!("    - Not a valid PSX game disc");
                println!("    - Using a non-standard format");
                println!("\n[i] Try running with --debug to see what's in the file:\n");
                println!("    psx-vcd detect {} --debug\n", input.display());
            } else {
                println!("NOT_FOUND");
            }
        }
    }

    Ok(())
}

// Helper functions

fn detect_and_print_game_id(bin_path: &Path) -> Result<Option<String>> {
    println!("\n[*] Detecting Game ID...");
    let game_id = detect_game_id(bin_path)?;

    if let Some(ref id) = game_id {
        println!("[+] Game ID: {} ({})", id, get_region(id));
    } else {
        println!("[!] Game ID not found (non-standard or corrupted)");
    }

    Ok(game_id)
}

fn get_region(game_id: &str) -> &'static str {
    if game_id.starts_with("SLUS") || game_id.starts_with("SCUS") {
        "USA"
    } else if game_id.starts_with("SLES") || game_id.starts_with("SCES") {
        "Europe"
    } else if game_id.starts_with("SLPS")
        || game_id.starts_with("SCPS")
        || game_id.starts_with("SLPM")
    {
        "Japan"
    } else if game_id.starts_with("SCED") || game_id.starts_with("SLED") {
        "Europe (Demo)"
    } else {
        "Unknown"
    }
}

fn validate_cue_input(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("Input file does not exist: {}", path.display());
    }
    if path.extension().and_then(|s| s.to_str()) != Some("cue") {
        bail!("Input must be a .cue file");
    }
    Ok(())
}

fn validate_bin_input(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("Input file does not exist: {}", path.display());
    }
    if path.extension().and_then(|s| s.to_str()) != Some("bin") {
        bail!("Input must be a .bin file");
    }
    Ok(())
}

fn validate_gap_flags(gap_plus: bool, gap_minus: bool) -> Result<()> {
    if gap_plus && gap_minus {
        bail!("Cannot use both --gap-plus and --gap-minus");
    }
    Ok(())
}

fn print_success(output: &PathBuf, gap_plus: bool, gap_minus: bool) -> Result<()> {
    let final_size = std::fs::metadata(output)?.len();
    println!("\n[+] Conversion completed successfully!");
    println!("    Output: {}", output.display());
    println!("    Size: {:.2} MB", final_size as f64 / (1024.0 * 1024.0));

    if gap_plus {
        println!("    Applied: gap++ (+2 seconds adjustment)");
    } else if gap_minus {
        println!("    Applied: gap-- (-2 seconds adjustment)");
    }

    println!("\n[i] Ready for POPSTARTER/OPL!");
    println!(
        "    Copy {} to your POPS folder\n",
        output.file_name().unwrap().to_string_lossy()
    );

    Ok(())
}

fn clean_game_name(name: &str) -> String {
    let mut clean = name.to_string();

    let patterns_to_remove = [
        r"\(USA\)",
        r"\(Europe\)",
        r"\(Japan\)",
        r"\(World\)",
        r"\(En,Fr,De,Es,It\)",
        r"\(En\)",
        r"\(Fr\)",
        r"\(De\)",
        r"\(Es\)",
        r"\(It\)",
        r"\(Ja\)",
        r"\(Disc \d+\)",
        r"\(Disc [A-Z]\)",
        r"\(CD \d+\)",
        r"\(CD [A-Z]\)",
        r"\(Rev \d+\)",
        r"\(v\d+\.\d+\)",
        r"\[!\]",
        r"\[b\]",
        r"\[a\]",
        r"\[h\d*\]",
        r"\[f\d*\]",
        r"\[t\d*\]",
        r"\[o\d*\]",
        r"\[T[+-].*?\]",
        r"\(Track \d+\)",
        r"\(Demo\)",
        r"\(Beta\)",
        r"\(Proto\)",
        r"\(Sample\)",
        r"\(Promo\)",
        r"\(Unl\)",
        r"\[SLUS[-_]\d+\.\d+\]",
        r"\[SLES[-_]\d+\.\d+\]",
        r"\[SCUS[-_]\d+\.\d+\]",
        r"\[SCES[-_]\d+\.\d+\]",
    ];

    for pattern in &patterns_to_remove {
        if let Ok(re) = regex::Regex::new(pattern) {
            clean = re.replace_all(&clean, "").to_string();
        }
    }

    clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    clean = clean.trim().to_string();

    if clean.is_empty() {
        clean = "game".to_string();
    }

    clean
}
