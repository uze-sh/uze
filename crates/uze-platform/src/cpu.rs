//! What the processor this runs on can do, where a program is published in
//! builds that differ by it.

/// Whether this processor lacks AVX2, for which x86-64 programs are often
/// published in a slower `baseline` build. Never on another architecture.
pub fn lacks_avx2() -> bool {
    imp::lacks_avx2()
}

#[cfg(target_arch = "x86_64")]
mod imp {
    pub(super) fn lacks_avx2() -> bool {
        !std::arch::is_x86_feature_detected!("avx2")
    }
}

#[cfg(not(target_arch = "x86_64"))]
mod imp {
    pub(super) fn lacks_avx2() -> bool {
        false
    }
}
