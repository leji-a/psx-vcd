// src/cue.rs
use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::utils::Msf;

/// CD-ROM track type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackType {
    Audio,
    Mode1_2048,
    Mode1_2352,
    Mode2_2336,
    Mode2_2352,
}

impl TrackType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "AUDIO" => Some(TrackType::Audio),
            "MODE1/2048" => Some(TrackType::Mode1_2048),
            "MODE1/2352" => Some(TrackType::Mode1_2352),
            "MODE2/2336" => Some(TrackType::Mode2_2336),
            "MODE2/2352" => Some(TrackType::Mode2_2352),
            _ => None,
        }
    }

    pub fn sector_size(&self) -> usize {
        match self {
            TrackType::Audio => 2352,
            TrackType::Mode1_2048 => 2048,
            TrackType::Mode1_2352 => 2352,
            TrackType::Mode2_2336 => 2336,
            TrackType::Mode2_2352 => 2352,
        }
    }
}

impl std::fmt::Display for TrackType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TrackType::Audio => "AUDIO",
            TrackType::Mode1_2048 => "MODE1/2048",
            TrackType::Mode1_2352 => "MODE1/2352",
            TrackType::Mode2_2336 => "MODE2/2336",
            TrackType::Mode2_2352 => "MODE2/2352",
        };
        write!(f, "{}", s)
    }
}

/// CUE track structure
#[derive(Debug, Clone)]
pub struct Track {
    pub number: u8,
    pub track_type: TrackType,
    pub index00_msf: Option<Msf>,
    pub index01_msf: Msf,
}

impl Track {
    pub fn new(number: u8, track_type: TrackType, index01_msf: Msf) -> Self {
        Self {
            number,
            track_type,
            index00_msf: None,
            index01_msf,
        }
    }

    pub fn is_audio(&self) -> bool {
        self.track_type == TrackType::Audio
    }

    pub fn sector_size(&self) -> usize {
        self.track_type.sector_size()
    }
}

/// CUE file entry (FILE directive)
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub filename: String,
    pub file_type: String,
    pub tracks: Vec<Track>,
    pub file_size: u64,
}

impl FileEntry {
    pub fn new(filename: String, file_type: String) -> Self {
        Self {
            filename,
            file_type,
            tracks: Vec::new(),
            file_size: 0,
        }
    }
}

/// Complete CUE sheet structure
#[derive(Debug, Clone)]
pub struct CueSheet {
    pub files: Vec<FileEntry>,
}

impl CueSheet {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    /// Parse a CUE file and validate its structure
    pub fn parse(cue_path: &Path) -> Result<Self> {
        let file = File::open(cue_path)
            .with_context(|| format!("Failed to open CUE file: {}", cue_path.display()))?;
        let reader = BufReader::new(file);

        let mut cue_sheet = CueSheet::new();
        let mut current_file: Option<FileEntry> = None;
        let mut current_track: Option<Track> = None;

        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with("REM") {
                continue;
            }

            if trimmed.starts_with("FILE ") {
                Self::handle_file_directive(
                    &mut cue_sheet,
                    &mut current_file,
                    &mut current_track,
                    trimmed,
                )?;
            } else if trimmed.starts_with("TRACK ") {
                Self::handle_track_directive(&mut current_file, &mut current_track, trimmed)?;
            } else if trimmed.starts_with("INDEX ") {
                Self::handle_index_directive(&mut current_track, trimmed)?;
            }
        }

        // Save last file and track
        if let Some(mut file) = current_file {
            if let Some(track) = current_track {
                file.tracks.push(track);
            }
            cue_sheet.files.push(file);
        }

        cue_sheet.validate()?;
        Ok(cue_sheet)
    }

    /// Handle FILE directive
    fn handle_file_directive(
        cue_sheet: &mut CueSheet,
        current_file: &mut Option<FileEntry>,
        current_track: &mut Option<Track>,
        trimmed: &str,
    ) -> Result<()> {
        // Save previous file if exists
        if let Some(mut file) = current_file.take() {
            if let Some(track) = current_track.take() {
                file.tracks.push(track);
            }
            cue_sheet.files.push(file);
        }

        // Parse new FILE
        let parts: Vec<&str> = trimmed.splitn(2, '"').collect();
        if parts.len() < 2 {
            bail!("Invalid FILE line: {}", trimmed);
        }
        let rest: Vec<&str> = parts[1].splitn(2, '"').collect();
        if rest.len() < 2 {
            bail!("Invalid FILE line: {}", trimmed);
        }
        let filename = rest[0].to_string();
        let file_type = rest[1].trim().to_string();

        *current_file = Some(FileEntry::new(filename, file_type));
        Ok(())
    }

    /// Handle TRACK directive
    fn handle_track_directive(
        current_file: &mut Option<FileEntry>,
        current_track: &mut Option<Track>,
        trimmed: &str,
    ) -> Result<()> {
        // Save previous track if exists
        if let Some(ref mut file) = current_file {
            if let Some(track) = current_track.take() {
                file.tracks.push(track);
            }
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 3 {
            bail!("Invalid TRACK line: {}", trimmed);
        }

        let track_num: u8 = parts[1]
            .parse()
            .with_context(|| format!("Invalid track number: {}", parts[1]))?;
        let track_type = TrackType::from_str(parts[2])
            .ok_or_else(|| anyhow::anyhow!("Unknown track type: {}", parts[2]))?;

        *current_track = Some(Track::new(track_num, track_type, Msf::new(0, 0, 0)));
        Ok(())
    }

    /// Handle INDEX directive
    fn handle_index_directive(current_track: &mut Option<Track>, trimmed: &str) -> Result<()> {
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 3 {
            bail!("Invalid INDEX line: {}", trimmed);
        }

        let index_num: u8 = parts[1]
            .parse()
            .with_context(|| format!("Invalid index number: {}", parts[1]))?;
        let msf = Msf::from_str(parts[2]).with_context(|| format!("Invalid MSF: {}", parts[2]))?;

        if let Some(ref mut track) = current_track {
            match index_num {
                0 => track.index00_msf = Some(msf),
                1 => track.index01_msf = msf,
                _ => {} // Ignore other indexes
            }
        }
        Ok(())
    }

    /// Validate CUE sheet structure
    fn validate(&self) -> Result<()> {
        if self.files.is_empty() {
            bail!("CUE file contains no FILE entries");
        }

        // Validate first track starts at 00:00:00
        if let Some(first_file) = self.files.first() {
            if let Some(first_track) = first_file.tracks.first() {
                if first_track.index01_msf.to_sectors() != 0 {
                    bail!("First track INDEX 01 must be 00:00:00");
                }
            }
        }

        Ok(())
    }

    /// Load file sizes for all BIN files referenced in CUE
    pub fn load_file_sizes(&mut self, cue_dir: &Path) -> Result<()> {
        for file in &mut self.files {
            let file_path = cue_dir.join(&file.filename);
            file.file_size = std::fs::metadata(&file_path)
                .with_context(|| format!("Failed to get size of: {}", file.filename))?
                .len();
        }
        Ok(())
    }

    /// Get total number of tracks across all files
    pub fn get_total_tracks(&self) -> usize {
        self.files.iter().map(|f| f.tracks.len()).sum()
    }

    /// Get the last track in the CUE sheet
    pub fn get_last_track(&self) -> Option<&Track> {
        self.files.last()?.tracks.last()
    }

    /// Validate that all tracks are MODE2/2352
    pub fn validate_mode2(&self) -> Result<()> {
        for file in &self.files {
            for track in &file.tracks {
                if track.number == 1 && track.track_type != TrackType::Mode2_2352 {
                    bail!(
                        "First track must be MODE2/2352, found: {}",
                        track.track_type
                    );
                }
            }
        }
        Ok(())
    }

    /// Print detailed CUE sheet information
    pub fn print_info(&self) {
        println!("\n=== CUE Sheet Information ===");
        for (file_idx, file) in self.files.iter().enumerate() {
            println!(
                "FILE #{}: \"{}\" {}",
                file_idx + 1,
                file.filename,
                file.file_type
            );
            println!(
                "  Size: {:.2} MB",
                file.file_size as f64 / (1024.0 * 1024.0)
            );
            for track in &file.tracks {
                println!("  TRACK {:02} {}", track.number, track.track_type);
                if let Some(idx00) = track.index00_msf {
                    println!("    INDEX 00 {}", idx00);
                }
                println!("    INDEX 01 {}", track.index01_msf);
            }
        }
        println!("Total tracks: {}", self.get_total_tracks());
        println!("=============================\n");
    }

    /// Recalculate MSF positions for a combined BIN file
    ///
    /// This implements cue2pops v2.0 MSF recalculation logic:
    /// - Track 01: Always INDEX 00=00:00:00, INDEX 01=00:02:00
    /// - Track 02+: Applies +150 sector adjustment for pregaps
    ///
    /// The logic matches the original cue2pops behavior exactly for
    /// proper compatibility with POPSTARTER/OPL.
    pub fn recalculate_msf_for_combined(&mut self) {
        let mut accumulated_sectors = 0u32;

        println!("  === Recalculating MSF (cue2pops v2.0 logic) ===");

        for file in &mut self.files {
            let physical_sectors = (file.file_size / file.tracks[0].sector_size() as u64) as u32;

            for track in &mut file.tracks {
                if track.number == 1 {
                    // Track 01: Always starts at 00:00:00 for INDEX 00
                    // INDEX 01 is always at 00:02:00 (150 sectors pregap)
                    track.index00_msf = Some(Msf::from_sectors(0));
                    track.index01_msf = Msf::from_sectors(150);

                    println!(
                        "    Track {:02}: INDEX 00={} INDEX 01={} | Physical: {} sectors",
                        track.number,
                        track.index00_msf.unwrap(),
                        track.index01_msf,
                        physical_sectors
                    );

                    // Track 01 adds its physical size including the 150 sector pregap
                    accumulated_sectors += physical_sectors;
                } else {
                    // CRITICAL: cue2pops applies +150 sectors (2 seconds) adjustment
                    // This happens TWICE for tracks with explicit INDEX 00:
                    // 1. Once for the physical pregap
                    // 2. Once for the "unconditional" adjustment

                    if track.index00_msf.is_some() {
                        // Track has explicit pregap (INDEX 00 in original CUE)
                        // INDEX 00 = accumulated + 150 (physical) + 150 (unconditional)
                        // INDEX 01 = INDEX 00 + 150 (pregap length)
                        let index00_sector = accumulated_sectors + 150 + 150;
                        let index01_sector = index00_sector + 150;

                        track.index00_msf = Some(Msf::from_sectors(index00_sector));
                        track.index01_msf = Msf::from_sectors(index01_sector);

                        println!(
                            "    Track {:02}: INDEX 00={} (sector {}) | INDEX 01={} (sector {}) | Physical: {} sectors",
                            track.number,
                            track.index00_msf.unwrap(),
                            index00_sector,
                            track.index01_msf,
                            index01_sector,
                            physical_sectors
                        );
                    } else {
                        // Track without explicit pregap
                        // Apply +150 unconditional adjustment
                        let adjusted_sector = accumulated_sectors + 150;
                        track.index00_msf = Some(Msf::from_sectors(adjusted_sector));
                        track.index01_msf = Msf::from_sectors(adjusted_sector);

                        println!(
                            "    Track {:02}: INDEX 00={} INDEX 01={} (sector {}) | Physical: {} sectors",
                            track.number,
                            track.index00_msf.unwrap(),
                            track.index01_msf,
                            adjusted_sector,
                            physical_sectors
                        );
                    }

                    // Add this file's physical sectors (which includes the pregap)
                    accumulated_sectors += physical_sectors;
                }

                println!("    -> Accumulated: {} sectors", accumulated_sectors);
            }
        }

        println!("  ===============================================\n");
    }
}
