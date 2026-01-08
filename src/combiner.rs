// src/combiner.rs
use crate::cue::CueSheet;
use crate::utils::Msf;
use anyhow::{Context, Result};
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::Path;

const BUFFER_SIZE: usize = 1024 * 1024; // 1MB buffer

/// Information about the combined BIN file
#[derive(Debug)]
pub struct CombinedBinInfo {
    pub total_bytes: u64,
    pub track_count: usize,
}

/// Combines multiple BIN files into a single output file
pub struct BinCombiner;

impl BinCombiner {
    /// Combine multiple BIN files referenced in a CUE sheet into a single BIN file
    ///
    /// This handles two scenarios:
    /// 1. Single-file CUE: Extracts tracks by MSF position from one BIN
    /// 2. Multi-file CUE: Concatenates multiple BIN files in order
    ///
    /// For single-file games, ensures Track 01 has proper pregap indexes:
    /// - INDEX 00 = 00:00:00
    /// - INDEX 01 = 00:02:00 (150 sectors pregap)
    pub fn combine(
        cue_sheet: &mut CueSheet,
        cue_dir: &Path,
        output_path: &Path,
    ) -> Result<CombinedBinInfo> {
        let total_tracks = cue_sheet.get_total_tracks();

        // Special case: single file with single track - just copy it
        if cue_sheet.files.len() == 1 && total_tracks == 1 {
            return Self::handle_single_file(cue_sheet, cue_dir, output_path);
        }

        println!(
            "  Combining {} track(s) from {} file(s)...",
            total_tracks,
            cue_sheet.files.len()
        );

        let mut output_file =
            File::create(output_path).context("Failed to create output BIN file")?;

        let mut total_bytes = 0u64;
        let mut buffer = vec![0u8; BUFFER_SIZE];

        // Process each FILE entry in the CUE
        for file_obj in &cue_sheet.files {
            let input_path = cue_dir.join(&file_obj.filename);

            let mut input_file = File::open(&input_path)
                .with_context(|| format!("Failed to open BIN: {}", file_obj.filename))?;

            if cue_sheet.files.len() > 1 {
                // Multi-file case: each FILE is a complete track
                Self::process_multifile_track(
                    &mut input_file,
                    &mut output_file,
                    file_obj,
                    &mut buffer,
                    &mut total_bytes,
                )?;
            } else {
                // Single-file case: extract tracks by MSF position
                Self::process_singlefile_tracks(
                    &mut input_file,
                    &mut output_file,
                    file_obj,
                    &mut buffer,
                    &mut total_bytes,
                )?;
            }
        }

        output_file.flush()?;

        // Recalculate MSF positions for multi-file CUEs
        if cue_sheet.files.len() > 1 {
            println!("  Recalculating MSF positions for combined BIN...");
            cue_sheet.recalculate_msf_for_combined();
        }

        Ok(CombinedBinInfo {
            total_bytes,
            track_count: total_tracks,
        })
    }

    /// Handle single-file, single-track case with proper pregap setup
    fn handle_single_file(
        cue_sheet: &mut CueSheet,
        cue_dir: &Path,
        output_path: &Path,
    ) -> Result<CombinedBinInfo> {
        let input_path = cue_dir.join(&cue_sheet.files[0].filename);
        println!(
            "  Single track detected, copying: {}",
            cue_sheet.files[0].filename
        );

        std::fs::copy(&input_path, output_path).context("Failed to copy single BIN file")?;

        let file_size = std::fs::metadata(output_path)?.len();

        // CRITICAL: Even for single-file games, Track 01 must have proper pregap
        // INDEX 00 = 00:00:00, INDEX 01 = 00:02:00 (150 sectors pregap)
        println!("  Fixing Track 01 indexes for single-file game...");
        if let Some(file) = cue_sheet.files.get_mut(0) {
            if let Some(track) = file.tracks.get_mut(0) {
                track.index00_msf = Some(Msf::from_sectors(0));
                track.index01_msf = Msf::from_sectors(150);
                println!("    Track 01: INDEX 00=00:00:00 INDEX 01=00:02:00");
            }
        }

        Ok(CombinedBinInfo {
            total_bytes: file_size,
            track_count: 1,
        })
    }

    /// Process multi-file track (each FILE is a complete track)
    fn process_multifile_track(
        input_file: &mut File,
        output_file: &mut File,
        file_obj: &crate::cue::FileEntry,
        buffer: &mut [u8],
        total_bytes: &mut u64,
    ) -> Result<()> {
        println!(
            "  Processing: {} ({} bytes)",
            file_obj.filename, file_obj.file_size
        );

        for track in &file_obj.tracks {
            println!(
                "    Track {:02} [{}]: Complete file",
                track.number, track.track_type
            );
        }

        // Copy entire file
        loop {
            let bytes_read = input_file.read(buffer)?;
            if bytes_read == 0 {
                break;
            }
            output_file.write_all(&buffer[..bytes_read])?;
            *total_bytes += bytes_read as u64;
        }

        Ok(())
    }

    /// Process single-file with multiple tracks (extract by MSF position)
    fn process_singlefile_tracks(
        input_file: &mut File,
        output_file: &mut File,
        file_obj: &crate::cue::FileEntry,
        buffer: &mut [u8],
        total_bytes: &mut u64,
    ) -> Result<()> {
        let file_size = input_file.metadata()?.len();

        for (idx, track) in file_obj.tracks.iter().enumerate() {
            let start_bytes = track.index01_msf.to_sectors() as u64 * track.sector_size() as u64;

            let end_bytes = if idx + 1 < file_obj.tracks.len() {
                file_obj.tracks[idx + 1].index01_msf.to_sectors() as u64
                    * file_obj.tracks[idx + 1].sector_size() as u64
            } else {
                file_size
            };

            let track_bytes = end_bytes - start_bytes;

            println!(
                "    Track {:02} [{}]: MSF {} ({} bytes)",
                track.number, track.track_type, track.index01_msf, track_bytes
            );

            // Seek to track start and copy data
            input_file.seek(std::io::SeekFrom::Start(start_bytes))?;

            let mut remaining = track_bytes;
            while remaining > 0 {
                let to_read = (remaining as usize).min(BUFFER_SIZE);
                let bytes_read = input_file.read(&mut buffer[..to_read])?;
                if bytes_read == 0 {
                    break;
                }
                output_file.write_all(&buffer[..bytes_read])?;
                remaining -= bytes_read as u64;
                *total_bytes += bytes_read as u64;
            }
        }

        Ok(())
    }
}
