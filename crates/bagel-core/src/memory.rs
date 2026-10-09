//! Picks how much memory to give the game or a server, based on how many
//! mods it runs and how much RAM the computer has.

use std::path::Path;

/// Never go below this, even on small computers.
const MIN_MB: u32 = 2048;

/// Memory for something running `mods` mods on a computer with `total_mb` of
/// RAM (`None` when unknown). Leaves at least half the RAM for everything else.
pub fn recommended_mb(mods: usize, total_mb: Option<u64>) -> u32 {
    let want = match mods {
        0 => 3072,
        1..=49 => 4096,
        50..=149 => 6144,
        _ => 8192,
    };
    match total_mb {
        Some(total) => {
            let half = u32::try_from(total / 2).unwrap_or(u32::MAX) / 512 * 512;
            want.min(half.max(MIN_MB))
        }
        None => want,
    }
}

/// The computer's total RAM in MB.
pub fn total_ram_mb() -> Option<u64> {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    Some(sys.total_memory() / (1024 * 1024)).filter(|&mb| mb > 0)
}

/// Enabled mod files in `dir/mods`.
pub async fn count_mods(dir: &Path) -> usize {
    let Ok(mut entries) = tokio::fs::read_dir(dir.join("mods")).await else {
        return 0;
    };
    let mut n = 0;
    while let Ok(Some(entry)) = entries.next_entry().await {
        if entry.file_name().to_string_lossy().to_ascii_lowercase().ends_with(".jar") {
            n += 1;
        }
    }
    n
}

/// The recommended memory for the game or server folder `dir`.
pub async fn recommended_for(dir: &Path) -> u32 {
    recommended_mb(count_mods(dir).await, total_ram_mb())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn more_mods_get_more_memory() {
        let big = Some(32 * 1024);
        assert_eq!(recommended_mb(0, big), 3072);
        assert_eq!(recommended_mb(20, big), 4096);
        assert_eq!(recommended_mb(80, big), 6144);
        assert_eq!(recommended_mb(300, big), 8192);
    }

    #[test]
    fn small_computers_keep_half_their_ram() {
        assert_eq!(recommended_mb(300, Some(8 * 1024)), 4096);
        assert_eq!(recommended_mb(300, Some(16 * 1024 - 300)), 7680);
        assert_eq!(recommended_mb(300, Some(2048)), MIN_MB);
        assert_eq!(recommended_mb(300, None), 8192);
    }

    #[tokio::test]
    async fn counts_only_jars() {
        let dir = tempfile::tempdir().unwrap();
        let mods = dir.path().join("mods");
        std::fs::create_dir(&mods).unwrap();
        for f in ["a.jar", "B.JAR", "c.jar.disabled", "notes.txt"] {
            std::fs::write(mods.join(f), b"").unwrap();
        }
        assert_eq!(count_mods(dir.path()).await, 2);
        assert_eq!(count_mods(&dir.path().join("missing")).await, 0);
    }
}
