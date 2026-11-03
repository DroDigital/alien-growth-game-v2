//! IEEE 754 half-precision conversion for HDR texel data (procedural
//! environment maps in the game, BC6H input in the texture tools).

/// binary32 -> binary16 bits, round to nearest even. Handles subnormals,
/// overflow to infinity and NaN.
pub fn f32_to_f16_bits(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32;
    let mant = bits & 0x007f_ffff;
    if exp == 0xff {
        return sign | 0x7c00 | if mant != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = mant | 0x0080_0000;
        let shift = (14 - e) as u32;
        let half = 1u32 << (shift - 1);
        let rounded = (m + half - 1 + ((m >> shift) & 1)) >> shift;
        return sign | rounded as u16;
    }
    let half_mant = mant >> 13;
    let rest = mant & 0x1fff;
    let mut out = ((e as u32) << 10) | half_mant;
    if rest > 0x1000 || (rest == 0x1000 && (half_mant & 1) == 1) {
        out += 1; // a carry into the exponent is the correct result
    }
    sign | out as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        assert_eq!(f32_to_f16_bits(0.0), 0x0000);
        assert_eq!(f32_to_f16_bits(1.0), 0x3c00);
        assert_eq!(f32_to_f16_bits(-2.0), 0xc000);
        assert_eq!(f32_to_f16_bits(0.5), 0x3800);
        assert_eq!(f32_to_f16_bits(65504.0), 0x7bff);
        assert_eq!(f32_to_f16_bits(1e9), 0x7c00);
        assert_eq!(f32_to_f16_bits(f32::NAN) & 0x7c00, 0x7c00);
        assert_eq!(f32_to_f16_bits(5.960_464_5e-8), 0x0001);
        // 1 + 2^-11 is exactly halfway between two halves: rounds to even.
        assert_eq!(f32_to_f16_bits(1.0 + 1.0 / 2048.0), 0x3c00);
    }
}
