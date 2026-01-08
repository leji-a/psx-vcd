use anyhow::Result;
use regex::Regex;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Msf (Minutes:Seconds:Frames) timestamp structure
///
/// CD-ROM sectors are addressed using MSF format where:
/// - 1 second = 75 frames
/// - 1 minute = 60 seconds = 4500 frames
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Msf {
    pub minutes: u8,
    pub seconds: u8,
    pub frames: u8,
}

impl Msf {
    pub fn new(minutes: u8, seconds: u8, frames: u8) -> Self {
        Self {
            minutes,
            seconds,
            frames,
        }
    }

    /// Parse Msf from string format "MM:SS:FF"
    pub fn from_str(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 3 {
            anyhow::bail!("Invalid MSF format: {}", s);
        }
        Ok(Self {
            minutes: parts[0].parse()?,
            seconds: parts[1].parse()?,
            frames: parts[2].parse()?,
        })
    }

    /// Convert Msf to sector count (LBA - Logical Block Address)
    pub fn to_sectors(self) -> u32 {
        ((self.minutes as u32 * 60) + self.seconds as u32) * 75 + self.frames as u32
    }

    /// Create Msf from sector count
    pub fn from_sectors(sectors: u32) -> Self {
        let frames = sectors % 75;
        let total_seconds = sectors / 75;
        let seconds = total_seconds % 60;
        let minutes = total_seconds / 60;
        Self::new(minutes as u8, seconds as u8, frames as u8)
    }

    /// Add seconds to Msf (can be negative for subtraction)
    pub fn add_seconds(self, seconds: i32) -> Self {
        let total_sectors = self.to_sectors() as i32 + (seconds * 75);
        if total_sectors < 0 {
            Self::new(0, 0, 0)
        } else {
            Self::from_sectors(total_sectors as u32)
        }
    }

    /// Convert to BCD (Binary-Coded Decimal) format used in VCD header
    pub fn to_bcd(self) -> [u8; 3] {
        [
            ((self.minutes / 10) << 4) | (self.minutes % 10),
            ((self.seconds / 10) << 4) | (self.seconds % 10),
            ((self.frames / 10) << 4) | (self.frames % 10),
        ]
    }
}

impl std::fmt::Display for Msf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:02}:{:02}:{:02}",
            self.minutes, self.seconds, self.frames
        )
    }
}

/// Detect PlayStation Game ID from binary data
pub fn detect_game_id(bin_path: &Path) -> Result<Option<String>> {
    let mut file = File::open(bin_path)?;

    let mut buffer = vec![0u8; 150 * 1024];
    let bytes_read = file.read(&mut buffer)?;
    if bytes_read == 0 {
        return Ok(None);
    }
    buffer.truncate(bytes_read);

    let patterns = vec![
        Regex::new(r"(S[CL][EUA][SD][_]\d{3}\.\d{2})")?, // SLUS_XXX.XX
        Regex::new(r"(S[CL][EUA][SD][-]\d{3}\.\d{2})")?, // SLUS-XXX.XX
        Regex::new(r"(S[CL][EUA][SD][_-]\d{5})")?,       // SLUS_XXXXX
        Regex::new(r"(S[CL][EUA][SD][ ]\d{3}\.\d{2})")?, // SLUS XXX.XX
    ];

    let search_str = String::from_utf8_lossy(&buffer);

    for pattern in &patterns {
        if let Some(caps) = pattern.find(&search_str) {
            let mut game_id = caps.as_str().to_string();
            game_id = game_id.replace('-', "_");
            game_id = game_id.replace(' ', "_");

            if !game_id.contains('.') && game_id.len() == 10 {
                game_id.insert(9, '.');
                game_id.insert(9, '.');
                game_id.truncate(11);
            }

            return Ok(Some(game_id));
        }
    }

    let prefixes = [
        "SLUS", "SCUS", "SLES", "SCES", "SLPS", "SCPS", "SLPM", "SCED", "SLED",
    ];

    let pattern = Regex::new(r"(S[CL][EUA][SD][_]\d{3}\.\d{2})")?; // regex fuera del loop
    for (i, window) in buffer.windows(4).enumerate() {
        let prefix = String::from_utf8_lossy(window);
        if prefixes.contains(&prefix.as_ref()) && i + 11 <= buffer.len() {
            let potential_id = String::from_utf8_lossy(&buffer[i..i + 11]);
            if let Some(caps) = pattern.find(&potential_id) {
                let game_id = caps.as_str().replace('-', "_");
                return Ok(Some(game_id));
            }
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msf_conversion() {
        let msf = Msf::new(1, 30, 50);
        let sectors = msf.to_sectors();
        assert_eq!(sectors, (1 * 60 + 30) * 75 + 50);

        let msf2 = Msf::from_sectors(sectors);
        assert_eq!(msf, msf2);
    }

    #[test]
    fn test_msf_add_seconds() {
        let msf = Msf::new(0, 2, 0);
        let msf2 = msf.add_seconds(2);
        assert_eq!(msf2, Msf::new(0, 4, 0));
    }

    #[test]
    fn test_msf_bcd() {
        let msf = Msf::new(12, 34, 56);
        let bcd = msf.to_bcd();
        assert_eq!(bcd, [0x12, 0x34, 0x56]);
    }

    #[test]
    fn test_msf_from_str() {
        let msf = Msf::from_str("01:30:50").unwrap();
        assert_eq!(msf, Msf::new(1, 30, 50));
    }

    #[test]
    fn test_msf_negative_add() {
        let msf = Msf::new(0, 2, 0);
        let msf2 = msf.add_seconds(-3);
        assert_eq!(msf2, Msf::new(0, 0, 0));
    }
}
