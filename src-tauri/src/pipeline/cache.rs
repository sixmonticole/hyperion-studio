//! Cache directory management for 16-bit TIFFs.
//!
//! A demosaiced RAW is a single uncompressed 16-bit RGB TIFF. A JSON manifest
//! (`manifest.json`) records the RAW -> TIFF mapping so a session can resume
//! without re-demosaicing.

use crate::model::FrameInfo;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub source_dir: String,
    pub version: u32,
    pub frames: Vec<FrameInfo>,
}

impl Manifest {
    /// Manifest file name inside the cache directory.
    pub const FILE: &'static str = "manifest.json";

    /// Manifest path for a given cache directory.
    pub fn path_in(cache_dir: &Path) -> PathBuf {
        cache_dir.join(Self::FILE)
    }

    /// Loads an existing manifest, if valid.
    pub fn load(cache_dir: &Path) -> Option<Self> {
        let path = Self::path_in(cache_dir);
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Writes the manifest to disk.
    pub fn save(&self, cache_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(cache_dir)
            .with_context(|| format!("creating cache dir {}", cache_dir.display()))?;
        let path = Self::path_in(cache_dir);
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, data)
            .with_context(|| format!("writing manifest {}", path.display()))?;
        Ok(())
    }

    /// Finds the cached entry matching a RAW path, so a changed source can be
    /// detected (same name, different file).
    pub fn find_by_source(&self, source_path: &str) -> Option<&FrameInfo> {
        self.frames.iter().find(|f| f.source_path == source_path)
    }
}

/// Derives a cache TIFF path from a source RAW path and an index.
pub fn cache_path_for(cache_dir: &Path, raw_path: &Path, index: usize) -> PathBuf {
    let stem = raw_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("frame");
    cache_dir.join(format!("{index:05}_{stem}.tif"))
}

/// Recursively sums the size of a directory, in bytes. Returns 0 when the
/// directory does not exist. Used to report cache size and space freed.
pub fn dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => total += dir_size(&path),
            Ok(ft) if ft.is_file() => {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
            _ => {}
        }
    }
    total
}

/// Free space available on the filesystem holding `path`, in bytes.
///
/// Best effort: returns `None` when the platform call fails, so callers can
/// simply skip the disk-space warning rather than fail.
pub fn available_space(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(c.as_ptr(), &mut stat) };
    if rc != 0 {
        return None;
    }
    Some(stat.f_frsize as u64 * stat.f_bavail as u64)
}

/// Reads a 64-bit integer from `sysctlbyname`, or `None` on failure.
fn sysctl_u64(name: &str) -> Option<u64> {
    use std::ffi::CString;

    let cname = CString::new(name).ok()?;
    let mut value: u64 = 0;
    let mut size = std::mem::size_of::<u64>();
    let rc = unsafe {
        libc::sysctlbyname(
            cname.as_ptr(),
            &mut value as *mut u64 as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc == 0 { Some(value) } else { None }
}

/// Total physical memory, in bytes. `None` if the platform call fails.
pub fn total_memory() -> Option<u64> {
    sysctl_u64("hw.memsize")
}

/// Memory currently available for new allocations, in bytes.
///
/// macOS keeps almost no memory "free" by design: it fills RAM with file cache
/// and reclaims on demand. Reading only `vm.page_free_count` therefore reports
/// a misleadingly small number (often under 100 MB) and made the guard refuse
/// perfectly safe runs.
///
/// We report what the kernel can hand out **without swapping**: free, inactive,
/// speculative and purgeable pages. `None` if the platform call fails.
pub fn available_memory() -> Option<u64> {
    let page_size = sysctl_u64("hw.pagesize").unwrap_or(16384);

    // `vm.page_inactive_count` is not exposed on modern macOS, so we go through
    // the Mach host statistics, which is the documented way.
    let mut stats: libc::vm_statistics64 = unsafe { std::mem::zeroed() };
    let mut count = libc::HOST_VM_INFO64_COUNT;
    // `mach_host_self` is marked deprecated in the libc crate in favour of the
    // mach2 crate, but the call itself is correct and stable; pulling in a whole
    // dependency for it is not worth it.
    #[allow(deprecated)]
    let host = unsafe { libc::mach_host_self() };
    let rc = unsafe {
        libc::host_statistics64(
            host,
            libc::HOST_VM_INFO64,
            &mut stats as *mut _ as *mut libc::integer_t,
            &mut count,
        )
    };
    if rc != libc::KERN_SUCCESS {
        // Fall back to the free-page sysctl, with a floor, rather than lying.
        let free = sysctl_u64("vm.page_free_count").unwrap_or(0);
        return Some(free.saturating_mul(page_size));
    }

    let reclaimable = stats.free_count as u64
        + stats.inactive_count as u64
        + stats.speculative_count as u64
        + stats.purgeable_count as u64;
    Some(reclaimable.saturating_mul(page_size))
}

/// Indicates whether a cached TIFF exists and is not empty. A zero-length file
/// is the leftover of an interrupted write, so it is treated as absent.
pub fn is_usable(cache_path: &Path) -> bool {
    std::fs::metadata(cache_path)
        .map(|m| m.len() > 0)
        .unwrap_or(false)
}
