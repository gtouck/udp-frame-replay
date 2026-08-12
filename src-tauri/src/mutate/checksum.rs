//! 校验和算法。
//!
//! CRC 用 `crc` crate 的标准参数表，并用公开测试向量钉死 ——
//! 校验和算错了对端不会告诉你「算法选错了」，只会静默丢包，
//! 所以这里必须有可对照的标准答案。

use crate::config::ChecksumAlgo;

/// 对 `data` 求校验和。返回值按算法宽度截断。
pub fn compute(algo: ChecksumAlgo, data: &[u8]) -> u64 {
    match algo {
        ChecksumAlgo::Sum8 => data.iter().fold(0u8, |a, b| a.wrapping_add(*b)) as u64,

        ChecksumAlgo::Sum16 => data
            .iter()
            .fold(0u16, |a, b| a.wrapping_add(*b as u16)) as u64,

        ChecksumAlgo::Xor8 => data.iter().fold(0u8, |a, b| a ^ b) as u64,

        ChecksumAlgo::Crc16Ccitt => {
            const C: crc::Crc<u16> = crc::Crc::<u16>::new(&crc::CRC_16_IBM_3740);
            C.checksum(data) as u64
        }

        ChecksumAlgo::Crc16Modbus => {
            const C: crc::Crc<u16> = crc::Crc::<u16>::new(&crc::CRC_16_MODBUS);
            C.checksum(data) as u64
        }

        ChecksumAlgo::Crc16Xmodem => {
            const C: crc::Crc<u16> = crc::Crc::<u16>::new(&crc::CRC_16_XMODEM);
            C.checksum(data) as u64
        }

        ChecksumAlgo::Crc32 => {
            const C: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
            C.checksum(data) as u64
        }
    }
}

/// 该算法天然的结果字节数，用作界面上的默认宽度
pub fn natural_width(algo: ChecksumAlgo) -> usize {
    match algo {
        ChecksumAlgo::Sum8 | ChecksumAlgo::Xor8 => 1,
        ChecksumAlgo::Sum16
        | ChecksumAlgo::Crc16Ccitt
        | ChecksumAlgo::Crc16Modbus
        | ChecksumAlgo::Crc16Xmodem => 2,
        ChecksumAlgo::Crc32 => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 各标准算法对 "123456789" 的公开校验值。
    /// 这些数字是算法身份证 —— 对不上就说明参数表选错了。
    const CHECK: &[u8] = b"123456789";

    #[test]
    fn crc16_ccitt_matches_published_check_value() {
        assert_eq!(compute(ChecksumAlgo::Crc16Ccitt, CHECK), 0x29B1);
    }

    #[test]
    fn crc16_modbus_matches_published_check_value() {
        assert_eq!(compute(ChecksumAlgo::Crc16Modbus, CHECK), 0x4B37);
    }

    #[test]
    fn crc16_xmodem_matches_published_check_value() {
        assert_eq!(compute(ChecksumAlgo::Crc16Xmodem, CHECK), 0x31C3);
    }

    #[test]
    fn crc32_matches_published_check_value() {
        assert_eq!(compute(ChecksumAlgo::Crc32, CHECK), 0xCBF4_3926);
    }

    #[test]
    fn sum8_wraps_at_one_byte() {
        assert_eq!(compute(ChecksumAlgo::Sum8, &[0x01, 0x02, 0x03]), 0x06);
        assert_eq!(compute(ChecksumAlgo::Sum8, &[0xFF, 0x02]), 0x01);
    }

    #[test]
    fn sum16_keeps_two_bytes() {
        assert_eq!(compute(ChecksumAlgo::Sum16, &[0xFF, 0x02]), 0x0101);
        assert_eq!(compute(ChecksumAlgo::Sum16, &[0xFF; 4]), 0x03FC);
    }

    #[test]
    fn xor8_folds_all_bytes() {
        assert_eq!(compute(ChecksumAlgo::Xor8, &[0x0F, 0xF0]), 0xFF);
        assert_eq!(compute(ChecksumAlgo::Xor8, &[0xAA, 0xAA]), 0x00);
    }

    #[test]
    fn empty_input_is_defined_for_every_algorithm() {
        for algo in [
            ChecksumAlgo::Sum8,
            ChecksumAlgo::Sum16,
            ChecksumAlgo::Xor8,
            ChecksumAlgo::Crc16Ccitt,
            ChecksumAlgo::Crc16Modbus,
            ChecksumAlgo::Crc16Xmodem,
            ChecksumAlgo::Crc32,
        ] {
            // 不 panic 即可，具体值由各算法的初值决定
            let _ = compute(algo, &[]);
        }
    }

    #[test]
    fn natural_widths_are_sane() {
        assert_eq!(natural_width(ChecksumAlgo::Xor8), 1);
        assert_eq!(natural_width(ChecksumAlgo::Crc16Ccitt), 2);
        assert_eq!(natural_width(ChecksumAlgo::Crc32), 4);
    }
}
