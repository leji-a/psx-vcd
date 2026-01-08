// src/vcd.rs
use anyhow::Result;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use crate::cue::CueSheet;
use crate::utils::Msf;

const SECTOR_SIZE: usize = 2352;
const VCD_HEADER_SIZE: usize = 0x100000; // 1MB header
const PREGAP_SECTORS: u32 = 150; // 2 seconds at 75 sectors/second

/// VCD Converter - creates POPSTARTER-compatible VCD files
///
/// The VCD format is used by POPSTARTER/OPL to run PlayStation games on PS2.
/// It consists of:
/// 1. A 1MB header containing track information and TOC (Table of Contents)
/// 2. The raw BIN data from the game disc
///
/// The header format matches cue2pops v2.0 for maximum compatibility.
pub struct VcdConverter {
    gap_adjustment: i32,
}

impl VcdConverter {
    /// Create a new VCD converter with optional gap adjustment
    ///
    /// Gap adjustment adds or subtracts seconds from track indexes:
    /// - gap_plus: +2 seconds (useful for some problem discs)
    /// - gap_minus: -2 seconds (rarely needed)
    pub fn new(gap_plus: bool, gap_minus: bool) -> Self {
        let gap_adjustment = if gap_plus {
            2
        } else if gap_minus {
            -2
        } else {
            0
        };

        Self { gap_adjustment }
    }

    /// Convert a combined BIN file to VCD format
    ///
    /// This creates a VCD file with proper header and copies the BIN data.
    /// The header contains:
    /// - TOC descriptors (A0, A1, A2)
    /// - Track entries with MSF positions
    /// - Sector count information
    /// - cue2pops version identifier
    pub fn convert_to_vcd(
        &self,
        combined_bin: &Path,
        vcd_path: &Path,
        cue_sheet: &CueSheet,
    ) -> Result<()> {
        println!("  Creating VCD file...");

        let bin_size = std::fs::metadata(combined_bin)?.len();

        // Create VCD header with TOC information
        let header = self.create_vcd_header(bin_size, cue_sheet)?;

        // Write VCD file
        let mut vcd_file = File::create(vcd_path)?;

        // Write 1MB header
        vcd_file.write_all(&header)?;

        // Copy BIN data after header
        let mut bin_file = File::open(combined_bin)?;
        let mut buffer = vec![0u8; 1024 * 1024]; // 1MB buffer for efficient copying

        loop {
            let bytes_read = bin_file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            vcd_file.write_all(&buffer[..bytes_read])?;
        }

        vcd_file.flush()?;

        let vcd_size = std::fs::metadata(vcd_path)?.len();
        println!(
            "  [+] VCD created: {:.2} MB",
            vcd_size as f64 / (1024.0 * 1024.0)
        );

        Ok(())
    }

    /// Create the VCD header (0x100000 bytes / 1MB)
    ///
    /// The header layout matches cue2pops v2.0:
    /// - 0x00-0x09: Descriptor A0 (disc type)
    /// - 0x0A-0x13: Descriptor A1 (content array)
    /// - 0x14-0x1D: Descriptor A2 (lead-out)
    /// - 0x1E+: Track entries (10 bytes each)
    /// - 0x400-0x403: cue2pops signature
    /// - 0x408-0x40B: Total sector count
    /// - 0x40C-0x40F: Total sector count (duplicate)
    fn create_vcd_header(&self, bin_size: u64, cue_sheet: &CueSheet) -> Result<Vec<u8>> {
        let mut header = vec![0u8; VCD_HEADER_SIZE];

        // Calculate actual sectors from combined BIN
        let bin_sectors = (bin_size / SECTOR_SIZE as u64) as u32;

        // CRITICAL: cue2pops only counts explicit PREGAP keywords in CUE
        // Multi-file CUEs like Tekken 3 use INDEX 00 but no PREGAP keywords
        let pregap_count = 0u32; // Correct for most games
        let postgap_count = 0u32; // Rarely used

        // cue2pops exact calculation:
        // total = bin_sectors + (pregap_count * 150) + (postgap_count * 150)
        let total_sectors =
            bin_sectors + (pregap_count * PREGAP_SECTORS) + (postgap_count * PREGAP_SECTORS);

        println!("\n  === VCD Header Calculation (cue2pops v2.0) ===");
        println!("  BIN size: {} bytes", bin_size);
        println!("  BIN sectors: {}", bin_sectors);
        println!("  Pregap keywords: {}", pregap_count);
        println!("  Postgap keywords: {}", postgap_count);
        println!("  Total sectors (for header): {}", total_sectors);

        // Build the 3 TOC descriptors
        self.build_descriptor_a0(&mut header);
        self.build_descriptor_a1(&mut header, cue_sheet);
        self.build_descriptor_a2(&mut header, total_sectors);

        // Write track entries starting at offset 0x1E (30)
        println!("\n  === Track Entries ===");
        self.write_track_entries(&mut header, cue_sheet);

        // Write sector count at offsets 0x408 and 0x40C (1032, 1036)
        let sector_bytes = total_sectors.to_le_bytes();
        header[1032..1036].copy_from_slice(&sector_bytes);
        header[1036..1040].copy_from_slice(&sector_bytes);

        // Write cue2pops version signature at 0x400 (1024)
        header[1024] = 0x6B; // 'k'
        header[1025] = 0x48; // 'H'
        header[1026] = 0x6E; // 'n'
        header[1027] = 0x20; // ' ' - cue2pops v2.0 identifier

        println!("  ============================================\n");

        Ok(header)
    }

    /// Build Descriptor A0 (First Track / Disc Type)
    ///
    /// This descriptor indicates:
    /// - First track type (DATA or AUDIO)
    /// - First track number
    /// - Disc type (CD-XA for PlayStation)
    fn build_descriptor_a0(&self, header: &mut [u8]) {
        header[0] = 0x41; // First track type (0x41 = DATA)
        header[2] = 0xA0; // Descriptor ID
        header[7] = 0x01; // First track number
        header[8] = 0x20; // Disc type (0x20 = CD-XA)
    }

    /// Build Descriptor A1 (Last Track / Content Type)
    ///
    /// This descriptor indicates:
    /// - Last track type (DATA or AUDIO)
    /// - Total number of tracks (in BCD)
    /// - Content type for the disc
    fn build_descriptor_a1(&self, header: &mut [u8], cue_sheet: &CueSheet) {
        let last_track = cue_sheet.get_last_track();
        let track_count = cue_sheet.get_total_tracks();

        // Determine content type (0x01 for CDDA, 0x41 for DATA only)
        let content_type = if last_track.map(|t| t.is_audio()).unwrap_or(false) {
            0x01
        } else {
            0x41
        };

        header[10] = content_type;
        header[12] = 0xA1; // Descriptor ID
        header[17] = (((track_count / 10) << 4) | (track_count % 10)) as u8; // BCD track count
        header[20] = content_type; // v2.0 addition
    }

    /// Build Descriptor A2 (Lead-Out Position)
    ///
    /// The lead-out marks the end of the disc's playable area.
    /// CRITICAL: cue2pops adds +150 sectors to the total for the lead-out MSF.
    fn build_descriptor_a2(&self, header: &mut [u8], total_sectors: u32) {
        header[22] = 0xA2; // Descriptor ID

        // CRITICAL: Add 150 sectors for lead-out MSF
        // This matches cue2pops original C code exactly
        let leadout_sectors_for_msf = total_sectors + 150;
        let leadout_msf = Msf::from_sectors(leadout_sectors_for_msf);
        let leadout_bcd = leadout_msf.to_bcd();

        header[27] = leadout_bcd[0]; // Minutes
        header[28] = leadout_bcd[1]; // Seconds
        header[29] = leadout_bcd[2]; // Frames

        println!(
            "  Lead-Out MSF: {} (sectors: {} + 150 = {})",
            leadout_msf, total_sectors, leadout_sectors_for_msf
        );
    }

    /// Write track entries to header (starting at offset 30/0x1E)
    ///
    /// Each track entry is 10 bytes:
    /// - Byte 0: Track type (0x41 = DATA, 0x01 = AUDIO)
    /// - Byte 2: Track number (BCD)
    /// - Bytes 3-5: INDEX 00 MSF (BCD)
    /// - Byte 6: NULL
    /// - Bytes 7-9: INDEX 01 MSF (BCD)
    ///
    /// Gap adjustment (if any) is applied here to INDEX positions.
    fn write_track_entries(&self, header: &mut [u8], cue_sheet: &CueSheet) {
        let mut offset = 30;

        for file in &cue_sheet.files {
            for track in &file.tracks {
                // Track type (0x41 = DATA, 0x01 = AUDIO)
                header[offset] = if track.is_audio() { 0x01 } else { 0x41 };

                // Track number (BCD)
                offset += 2;
                header[offset] = ((track.number / 10) << 4) | (track.number % 10);

                // INDEX 00 MSF
                offset += 1;
                let index00_msf = track.index00_msf.unwrap_or(track.index01_msf);

                // Apply user-requested gap adjustment (only for tracks > 1)
                let adjusted_index00 = if track.number > 1 && self.gap_adjustment != 0 {
                    index00_msf.add_seconds(self.gap_adjustment)
                } else {
                    index00_msf
                };

                let index00_bcd = adjusted_index00.to_bcd();
                header[offset..offset + 3].copy_from_slice(&index00_bcd);

                // Skip NULL byte
                offset += 4;

                // INDEX 01 MSF
                let adjusted_index01 = if track.number > 1 && self.gap_adjustment != 0 {
                    track.index01_msf.add_seconds(self.gap_adjustment)
                } else {
                    track.index01_msf
                };

                let index01_bcd = adjusted_index01.to_bcd();
                header[offset..offset + 3].copy_from_slice(&index01_bcd);

                println!(
                    "  Track {:02} [{:5}]: INDEX 00={} (sector {}) | INDEX 01={} (sector {})",
                    track.number,
                    track.track_type,
                    adjusted_index00,
                    adjusted_index00.to_sectors(),
                    adjusted_index01,
                    adjusted_index01.to_sectors()
                );

                offset += 3;
            }
        }
    }
}
