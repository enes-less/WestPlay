//! CityHash64, version 1.0 (Geoff Pike and Jyrki Alakuijala, Google; MIT licence, see
//! THIRD_PARTY_NOTICES.md): how the game's archives name their entries (a path's hash). Version 1.1
//! changed the results, so it has to be this one.

const K0: u64 = 0xc3a5_c85c_97cb_3127;
const K1: u64 = 0xb492_b66f_be98_f273;
const K2: u64 = 0x9ae1_6a3b_2f90_404f;
const K3: u64 = 0xc949_d7c7_509e_6557;

pub fn city_hash64(s: &[u8]) -> u64 {
    let len = s.len();
    if len <= 16 {
        return hash_len_0_to_16(s);
    }
    if len <= 32 {
        return hash_len_17_to_32(s);
    }
    if len <= 64 {
        return hash_len_33_to_64(s);
    }

    // Longer: the end first, then 64-byte chunks from the start, keeping 56 bytes of state.
    let n = len as u64;
    let mut x = fetch64(s, len - 40);
    let mut y = fetch64(s, len - 16).wrapping_add(fetch64(s, len - 56));
    let mut z = hash_len_16(fetch64(s, len - 48).wrapping_add(n), fetch64(s, len - 24));
    let mut v = weak_hash_len_32_with_seeds(s, len - 64, n, z);
    let mut w = weak_hash_len_32_with_seeds(s, len - 32, y.wrapping_add(K1), x);
    x = x.wrapping_mul(K1).wrapping_add(fetch64(s, 0));

    // The chunks before the last part: as many whole chunks as fit in len - 1.
    let chunks = (len - 1) / 64;
    for chunk in 0..chunks {
        let pos = chunk * 64;
        x = rotate(x.wrapping_add(y).wrapping_add(v.0).wrapping_add(fetch64(s, pos + 8)), 37).wrapping_mul(K1);
        y = rotate(y.wrapping_add(v.1).wrapping_add(fetch64(s, pos + 48)), 42).wrapping_mul(K1);
        x ^= w.1;
        y = y.wrapping_add(v.0).wrapping_add(fetch64(s, pos + 40));
        z = rotate(z.wrapping_add(w.0), 33).wrapping_mul(K1);
        v = weak_hash_len_32_with_seeds(s, pos, v.1.wrapping_mul(K1), x.wrapping_add(w.0));
        w = weak_hash_len_32_with_seeds(s, pos + 32, z.wrapping_add(w.1), y.wrapping_add(fetch64(s, pos + 16)));
        std::mem::swap(&mut z, &mut x);
    }
    hash_len_16(
        hash_len_16(v.0, w.0).wrapping_add(shift_mix(y).wrapping_mul(K1)).wrapping_add(z),
        hash_len_16(v.1, w.1).wrapping_add(x),
    )
}

fn fetch64(s: &[u8], pos: usize) -> u64 {
    u64::from_le_bytes(s[pos..pos + 8].try_into().unwrap())
}

fn fetch32(s: &[u8], pos: usize) -> u64 {
    u32::from_le_bytes(s[pos..pos + 4].try_into().unwrap()).into()
}

fn rotate(value: u64, shift: u32) -> u64 {
    value.rotate_right(shift)
}

fn shift_mix(value: u64) -> u64 {
    value ^ (value >> 47)
}

/// Murmur-inspired: 128 bits down to 64.
fn hash_len_16(u: u64, v: u64) -> u64 {
    const MUL: u64 = 0x9ddf_ea08_eb38_2d69;
    let mut a = (u ^ v).wrapping_mul(MUL);
    a ^= a >> 47;
    let mut b = (v ^ a).wrapping_mul(MUL);
    b ^= b >> 47;
    b.wrapping_mul(MUL)
}

fn hash_len_0_to_16(s: &[u8]) -> u64 {
    let len = s.len();
    let n = len as u64;
    if len > 8 {
        let a = fetch64(s, 0);
        let b = fetch64(s, len - 8);
        // Rotating by the length: 9 to 16, never 0.
        return hash_len_16(a, rotate(b.wrapping_add(n), len as u32)) ^ b;
    }
    if len >= 4 {
        let a = fetch32(s, 0);
        return hash_len_16(n.wrapping_add(a << 3), fetch32(s, len - 4));
    }
    if len > 0 {
        let (a, b, c) = (u64::from(s[0]), u64::from(s[len >> 1]), u64::from(s[len - 1]));
        let y = a.wrapping_add(b << 8);
        let z = n.wrapping_add(c << 2);
        return shift_mix(y.wrapping_mul(K2) ^ z.wrapping_mul(K3)).wrapping_mul(K2);
    }
    K2
}

fn hash_len_17_to_32(s: &[u8]) -> u64 {
    let len = s.len();
    let a = fetch64(s, 0).wrapping_mul(K1);
    let b = fetch64(s, 8);
    let c = fetch64(s, len - 8).wrapping_mul(K2);
    let d = fetch64(s, len - 16).wrapping_mul(K0);
    hash_len_16(
        rotate(a.wrapping_sub(b), 43).wrapping_add(rotate(c, 30)).wrapping_add(d),
        a.wrapping_add(rotate(b ^ K3, 20)).wrapping_sub(c).wrapping_add(len as u64),
    )
}

fn hash_len_33_to_64(s: &[u8]) -> u64 {
    let len = s.len();
    let mut z = fetch64(s, 24);
    let mut a = fetch64(s, 0).wrapping_add((len as u64).wrapping_add(fetch64(s, len - 16)).wrapping_mul(K0));
    let mut b = rotate(a.wrapping_add(z), 52);
    let mut c = rotate(a, 37);
    a = a.wrapping_add(fetch64(s, 8));
    c = c.wrapping_add(rotate(a, 7));
    a = a.wrapping_add(fetch64(s, 16));
    let vf = a.wrapping_add(z);
    let vs = b.wrapping_add(rotate(a, 31)).wrapping_add(c);
    a = fetch64(s, 16).wrapping_add(fetch64(s, len - 32));
    z = fetch64(s, len - 8);
    b = rotate(a.wrapping_add(z), 52);
    c = rotate(a, 37);
    a = a.wrapping_add(fetch64(s, len - 24));
    c = c.wrapping_add(rotate(a, 7));
    a = a.wrapping_add(fetch64(s, len - 16));
    let wf = a.wrapping_add(z);
    let ws = b.wrapping_add(rotate(a, 31)).wrapping_add(c);
    let r = shift_mix(vf.wrapping_add(ws).wrapping_mul(K2).wrapping_add(wf.wrapping_add(vs).wrapping_mul(K0)));
    shift_mix(r.wrapping_mul(K0).wrapping_add(vs)).wrapping_mul(K2)
}

/// A 16-byte hash of `s[pos..pos + 32]` with seeds `a` and `b`.
fn weak_hash_len_32_with_seeds(s: &[u8], pos: usize, a: u64, b: u64) -> (u64, u64) {
    let (w, x, y, z) = (fetch64(s, pos), fetch64(s, pos + 8), fetch64(s, pos + 16), fetch64(s, pos + 24));
    let mut a = a.wrapping_add(w);
    let mut b = rotate(b.wrapping_add(a).wrapping_add(z), 21);
    let c = a;
    a = a.wrapping_add(x).wrapping_add(y);
    b = b.wrapping_add(rotate(a, 44));
    (a.wrapping_add(z), b.wrapping_add(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_the_empty_path_as_the_root() {
        assert_eq!(city_hash64(b""), K2);
    }

    /// Paths of every length band, as the game's own archives have them (found there by these hashes).
    #[test]
    fn hashes_paths_as_the_game_does() {
        for (path, hash) in [
            ("def", 0x2c6f_469e_fb31_c45a),
            ("def/city.sii", 0x9a5c_b1eb_aac9_8229),
            ("map/europe/sec+0000+0000.base", 0xc6c0_3649_95a4_439a),
            ("def/world/sign_board_model.mod_switzerland_reskin.sii", 0xff57_0adb_89db_74c0),
            (
                concat!(
                    "effect/eut2/dif_spec_weight_weight_dif_spec_weight/eut2.dif.spec.weight.weight.dif.spec.weight",
                    ".fmtsnmapcalc.shadow.tg0.tg1.a.heightmap.jittered.vcol2.clodblend.rfx",
                ),
                0xa2dc_9dd5_414d_e604,
            ),
        ] {
            assert_eq!(city_hash64(path.as_bytes()), hash, "{path}");
        }
    }
}
