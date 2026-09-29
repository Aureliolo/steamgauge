//! How much memory the machine's graphics card has, which decides the largest reader it can run.
//!
//! Asked of the platform rather than of the inference runtime, because the runtime only knows
//! a card once a model is open on it, and the question is which model to open.

/// What the platform says about the card with the most memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Card {
    pub bytes: u64,
    /// The processor and the card share one pool, as on Apple silicon. The card can use most of
    /// it, but not all, and nothing else on the machine is counted against it here.
    pub shared: bool,
}

/// The card with the most memory, where the platform says; `None` where it does not, which a
/// caller has to read as "choose as if there is no card".
#[must_use]
pub fn largest() -> Option<Card> {
    platform::largest()
}

#[cfg(windows)]
mod platform {
    use winreg::RegKey;
    use winreg::enums::{HKEY_LOCAL_MACHINE, RegType};

    /// Every display adapter's driver key, under the display device class.
    const DISPLAY_CLASS: &str =
        r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

    pub fn largest() -> Option<super::Card> {
        let class = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey(DISPLAY_CLASS)
            .ok()?;
        class
            .enum_keys()
            .filter_map(Result::ok)
            .filter_map(|adapter| class.open_subkey(adapter).ok())
            .filter_map(|adapter| {
                // The 64-bit value first: WMI's `AdapterRAM` and the older `MemorySize` are 32
                // bits and read 4 GB on every card larger than that.
                [
                    "HardwareInformation.qwMemorySize",
                    "HardwareInformation.MemorySize",
                ]
                .into_iter()
                .find_map(|name| {
                    let value = adapter.get_raw_value(name).ok()?;
                    super::bytes_of(value.vtype == RegType::REG_DWORD, &value.bytes)
                })
            })
            .max()
            .map(|bytes| super::Card {
                bytes,
                shared: false,
            })
    }
}

#[cfg(target_os = "macos")]
mod platform {
    pub fn largest() -> Option<super::Card> {
        let out = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        let bytes = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        Some(super::Card {
            bytes,
            shared: true,
        })
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    pub fn largest() -> Option<super::Card> {
        // NVIDIA through its own tool, AMD through the kernel's count of the card's memory.
        let nvidia = std::process::Command::new("nvidia-smi")
            .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
            .output()
            .ok()
            .and_then(|out| super::largest_mib(&String::from_utf8_lossy(&out.stdout)));
        let amd = std::fs::read_dir("/sys/class/drm")
            .ok()?
            .filter_map(Result::ok)
            .filter_map(|card| {
                std::fs::read_to_string(card.path().join("device/mem_info_vram_total")).ok()
            })
            .filter_map(|bytes| bytes.trim().parse::<u64>().ok())
            .max();
        nvidia.max(amd).map(|bytes| super::Card {
            bytes,
            shared: false,
        })
    }
}

/// A registry value as a size in bytes: a 32-bit one where the value is typed as such, and
/// otherwise the eight bytes of a 64-bit one, which some drivers store untyped as binary.
#[cfg_attr(not(windows), allow(dead_code))]
fn bytes_of(dword: bool, raw: &[u8]) -> Option<u64> {
    if dword {
        return Some(u64::from(u32::from_le_bytes(
            raw.get(..4)?.try_into().ok()?,
        )));
    }
    Some(u64::from_le_bytes(raw.get(..8)?.try_into().ok()?))
}

/// The largest of `nvidia-smi`'s per-card totals, which it prints in mebibytes, one per line.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn largest_mib(listing: &str) -> Option<u64> {
    listing
        .lines()
        .filter_map(|line| line.trim().parse::<u64>().ok())
        .max()
        .map(|mib| mib * 1024 * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_over_four_gigabytes_is_read_from_its_sixty_four_bit_value() {
        let the_4090 = 25_757_220_864_u64;
        assert_eq!(bytes_of(false, &the_4090.to_le_bytes()), Some(the_4090));
        assert_eq!(
            bytes_of(true, &4_293_918_720_u32.to_le_bytes()),
            Some(4_293_918_720)
        );
        assert_eq!(
            bytes_of(false, &[0; 4]),
            None,
            "four bytes are not a 64-bit size"
        );
    }

    #[test]
    fn the_largest_of_several_cards_is_the_one_counted() {
        assert_eq!(largest_mib("8192\n24564\n"), Some(24_564 * 1024 * 1024));
        assert_eq!(largest_mib("No devices were found\n"), None);
    }
}
