//! Architecture-specific optimizations and constants
//! 
//! This module provides architecture-specific functionality like cache line size
//! and CPU pause instructions that are needed for optimal performance.

/// Cache line size in bytes for the target architecture
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub const CACHE_LINE_SIZE: usize = 64;

#[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
pub const CACHE_LINE_SIZE: usize = 64;

#[cfg(any(target_arch = "powerpc", target_arch = "powerpc64"))]
pub const CACHE_LINE_SIZE: usize = 128;

#[cfg(target_arch = "s390x")]
pub const CACHE_LINE_SIZE: usize = 256;

#[cfg(target_arch = "riscv64")]
pub const CACHE_LINE_SIZE: usize = 64;

#[cfg(not(any(
    target_arch = "x86",
    target_arch = "x86_64",
    target_arch = "arm",
    target_arch = "aarch64",
    target_arch = "powerpc",
    target_arch = "powerpc64",
    target_arch = "s390x",
    target_arch = "riscv64"
)))]
pub const CACHE_LINE_SIZE: usize = 64;

/// Executes a CPU-specific instruction to indicate a spin-wait loop to the CPU
/// 
/// This helps improve performance in busy-wait loops by:
/// - Potentially reducing power consumption
/// - Avoiding pipeline flushes
/// - Giving priority to other hyper-threads
#[inline(always)]
pub fn spin_loop_pause() {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        #[cfg(target_arch = "x86")]
        std::arch::x86::_mm_pause();
        #[cfg(target_arch = "x86_64")]
        std::arch::x86_64::_mm_pause();
    }

    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    unsafe {
        #[cfg(target_feature = "v6")]
        std::arch::asm!("yield");
        #[cfg(not(target_feature = "v6"))]
        std::arch::asm!("nop");
    }

    #[cfg(any(target_arch = "powerpc", target_arch = "powerpc64"))]
    unsafe {
        // Lower priority of current thread
        std::arch::asm!("or 31,31,31");
    }

    #[cfg(target_arch = "riscv64")]
    unsafe {
        std::arch::asm!(".insn i 0x0F, 0, x0, x0, 0x010");
    }

    // For unsupported architectures, we do nothing (no-op)
    #[cfg(not(any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "aarch64",
        target_arch = "powerpc",
        target_arch = "powerpc64",
        target_arch = "riscv64"
    )))]
    {
        std::hint::spin_loop();
    }
}

/// Returns the equivalent of next power of 2 for the given value
/// 
/// This is a compile-time function that calculates the next power of 2
/// greater than or equal to the input value.
pub const fn round_up_to_power_of_2(mut n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    
    n -= 1;
    n |= n >> 1;
    n |= n >> 2;
    n |= n >> 4;
    n |= n >> 8;
    n |= n >> 16;
    #[cfg(target_pointer_width = "64")]
    {
        n |= n >> 32;
    }
    n + 1
}

/// Calculates the number of bits needed for cache line index
/// 
/// Used for index shuffling to minimize false sharing.
pub const fn get_cache_line_index_bits(elements_per_cache_line: usize) -> usize {
    match elements_per_cache_line {
        256 => 8,
        128 => 7,
        64 => 6,
        32 => 5,
        16 => 4,
        8 => 3,
        4 => 2,
        2 => 1,
        _ => 0,
    }
}

/// Computes whether to use index shuffling and how many bits to shuffle
pub const fn get_index_shuffle_bits(
    minimize_contention: bool, 
    array_size: usize,
    elements_per_cache_line: usize
) -> usize {
    if !minimize_contention {
        return 0;
    }
    
    let bits = get_cache_line_index_bits(elements_per_cache_line);
    let min_size = 1 << (bits * 2);
    
    if array_size < min_size {
        0
    } else {
        bits
    }
}

/// Remaps an index to minimize cache line contention
/// 
/// This function swaps the lowest BITS with the next BITS in the index,
/// which helps spread out contention across different cache lines.
#[inline(always)]
pub const fn remap_index(index: usize, bits: usize) -> usize {
    if bits == 0 {
        return index;
    }
    
    let mix_mask = (1 << bits) - 1;
    let mix = (index ^ (index >> bits)) & mix_mask;
    index ^ mix ^ (mix << bits)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_round_up_to_power_of_2() {
        assert_eq!(round_up_to_power_of_2(0), 1);
        assert_eq!(round_up_to_power_of_2(1), 1);
        assert_eq!(round_up_to_power_of_2(2), 2);
        assert_eq!(round_up_to_power_of_2(3), 4);
        assert_eq!(round_up_to_power_of_2(4), 4);
        assert_eq!(round_up_to_power_of_2(5), 8);
        assert_eq!(round_up_to_power_of_2(7), 8);
        assert_eq!(round_up_to_power_of_2(8), 8);
        assert_eq!(round_up_to_power_of_2(9), 16);
        assert_eq!(round_up_to_power_of_2(1023), 1024);
        assert_eq!(round_up_to_power_of_2(1024), 1024);
        assert_eq!(round_up_to_power_of_2(1025), 2048);
    }
    
    #[test]
    fn test_remap_index() {
        // With 0 bits, index should remain unchanged
        assert_eq!(remap_index(42, 0), 42);
        
        // With 3 bits (typical for 8 elements per cache line)
        // It should swap the lowest 3 bits with the next 3 bits
        assert_eq!(remap_index(0b000_000, 3), 0b000_000);
        assert_eq!(remap_index(0b000_001, 3), 0b001_000);
        assert_eq!(remap_index(0b000_010, 3), 0b010_000);
        assert_eq!(remap_index(0b001_000, 3), 0b000_001);
        assert_eq!(remap_index(0b010_000, 3), 0b000_010);
        assert_eq!(remap_index(0b001_010, 3), 0b010_001);
    }
}
