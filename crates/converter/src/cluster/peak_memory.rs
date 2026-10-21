//! Process memory figures for the cluster stage's benchmark report.

/// Peak working set of this process in bytes since it started, or `None` where unsupported.
pub fn peak_working_set_bytes() -> Option<u64> {
    imp::peak_working_set_bytes()
}

/// Current working set in bytes (for sampling), or `None` where unsupported.
pub fn working_set_bytes() -> Option<u64> {
    imp::working_set_bytes()
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::mem::size_of;

    /// Layout of the Win32 `PROCESS_MEMORY_COUNTERS` structure.
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCounters,
            size: u32,
        ) -> i32;
    }

    fn counters() -> Option<ProcessMemoryCounters> {
        let mut counters = ProcessMemoryCounters {
            cb: size_of::<ProcessMemoryCounters>() as u32,
            page_fault_count: 0,
            peak_working_set_size: 0,
            working_set_size: 0,
            quota_peak_paged_pool_usage: 0,
            quota_paged_pool_usage: 0,
            quota_peak_non_paged_pool_usage: 0,
            quota_non_paged_pool_usage: 0,
            pagefile_usage: 0,
            peak_pagefile_usage: 0,
        };
        // SAFETY: `GetCurrentProcess` returns a pseudo handle that needs no closing, and
        // `counters` is a live struct with the exact layout of `PROCESS_MEMORY_COUNTERS` whose
        // `cb` field holds its size, so the call writes only inside it.
        let ok = unsafe {
            K32GetProcessMemoryInfo(
                GetCurrentProcess(),
                &mut counters,
                size_of::<ProcessMemoryCounters>() as u32,
            )
        };
        (ok != 0).then_some(counters)
    }

    pub fn peak_working_set_bytes() -> Option<u64> {
        counters().map(|counters| counters.peak_working_set_size as u64)
    }

    pub fn working_set_bytes() -> Option<u64> {
        counters().map(|counters| counters.working_set_size as u64)
    }
}

#[cfg(target_os = "linux")]
mod imp {
    pub fn peak_working_set_bytes() -> Option<u64> {
        status_bytes("VmHWM:")
    }

    pub fn working_set_bytes() -> Option<u64> {
        status_bytes("VmRSS:")
    }

    /// Reads one `Name:   1234 kB` line of /proc/self/status and returns bytes.
    fn status_bytes(key: &str) -> Option<u64> {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let line = status.lines().find_map(|line| line.strip_prefix(key))?;
        let kib: u64 = line.split_whitespace().next()?.parse().ok()?;
        Some(kib * 1024)
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    pub fn peak_working_set_bytes() -> Option<u64> {
        None
    }

    pub fn working_set_bytes() -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn peak_covers_current_after_touching_memory() {
        const SIZE: usize = 64 * 1024 * 1024;
        let before = working_set_bytes().expect("working set is supported");
        let mut block = vec![0u8; SIZE];
        for page in block.chunks_mut(4096) {
            page[0] = 1;
        }
        let block = std::hint::black_box(block);
        let current = working_set_bytes().expect("working set is supported");
        let peak = peak_working_set_bytes().expect("peak working set is supported");
        assert!(peak >= current, "peak {peak} below current {current}");
        assert!(
            current >= before + (SIZE as u64) / 2,
            "working set grew from {before} to {current}"
        );
        assert_eq!(block.len(), SIZE);
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    #[test]
    fn unsupported_platforms_report_none() {
        assert_eq!(peak_working_set_bytes(), None);
        assert_eq!(working_set_bytes(), None);
    }
}
