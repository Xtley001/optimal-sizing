//! Multicall3 batching. See `ROADMAP.md`'s "Multicall batching for
//! `sizing-integration`" entry — this module is that feature.
//!
//! Multicall3 is deployed at the same address on essentially every EVM
//! chain via a deterministic CREATE2 deployment (public, well-known
//! infrastructure: <https://www.multicall3.com/deployments>) — same
//! treatment this crate already gives Uniswap v2's fixed fee or Curve's
//! fee precision: cited public protocol constant, not an invented value.
//!
//! The `aggregate3` selector below was computed via actual keccak256 of
//! the canonical signature `aggregate3((address,bool,bytes)[])`, not
//! transcribed from memory — same discipline `main.rs`'s existing
//! selectors (`SEL_GET_RESERVES` etc.) already follow.
//!
//! `aggregate3(Call3[] calls) returns (Result[] returnData)`, where
//! `Call3 = (address target, bool allowFailure, bytes callData)` and
//! `Result = (bool success, bytes returnData)`. Every call submitted by
//! this module sets `allowFailure = false`: if any sub-call reverts,
//! Multicall3 itself bubbles that revert up through `aggregate3`, so it
//! surfaces as an ordinary `eth_call` RPC error via the existing
//! `eth_call` error handling in `main.rs` — no new error path needed,
//! and this preserves the crate's existing "fail loudly, no silent
//! partial results" behavior (a stated project discipline) rather than quietly
//! returning `success = false` entries for a caller to miss.

/// Multicall3's deployed address — identical across EVM chains.
pub const MULTICALL3_ADDRESS: &str = "0xcA11bde05977b3631167028862bE2a173976CA11";

// aggregate3((address,bool,bytes)[]) -> verified via keccak256, see
// module doc comment above.
const SEL_AGGREGATE3: &str = "82ad56cb";

/// One read call to batch: the target contract and its raw calldata
/// (selector + ABI-encoded args, no `0x` prefix, as hex bytes already
/// decoded — callers pass the same bytes they'd otherwise hand directly
/// to a single `eth_call`).
pub struct Call3 {
    /// Target contract address, `"0x..."`.
    pub target: String,
    /// Raw calldata bytes for this call (selector + encoded args).
    pub call_data: Vec<u8>,
}

impl Call3 {
    /// Builds a `Call3` from a target address and a hex calldata string
    /// (no `0x` prefix), matching the selector-string style already used
    /// throughout `main.rs` (e.g. `SEL_BALANCES` + an encoded arg).
    pub fn new(target: &str, call_data_hex: &str) -> Self {
        Self {
            target: target.to_string(),
            call_data: from_hex_bytes(call_data_hex),
        }
    }
}

fn from_hex_bytes(s: &str) -> Vec<u8> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("invalid hex in call_data"))
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut acc, b| {
            write!(acc, "{b:02x}").unwrap();
            acc
        })
}

fn word_uint256(v: u64) -> [u8; 32] {
    let mut word = [0u8; 32];
    word[24..32].copy_from_slice(&v.to_be_bytes());
    word
}

fn word_bool(b: bool) -> [u8; 32] {
    let mut word = [0u8; 32];
    if b {
        word[31] = 1;
    }
    word
}

fn word_address(addr: &str) -> [u8; 32] {
    let raw = from_hex_bytes(addr);
    assert_eq!(
        raw.len(),
        20,
        "address must decode to exactly 20 bytes, got {}",
        raw.len()
    );
    let mut word = [0u8; 32];
    word[12..32].copy_from_slice(&raw);
    word
}

/// ABI-encodes a dynamic `bytes` value: a length word followed by the
/// data, right-padded with zero bytes to a multiple of 32.
fn encode_bytes_value(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 + data.len() + 32);
    out.extend_from_slice(&word_uint256(data.len() as u64));
    out.extend_from_slice(data);
    let pad = (32 - (data.len() % 32)) % 32;
    out.resize(out.len() + pad, 0u8);
    out
}

/// ABI-encodes the full calldata for `aggregate3(calls)`, selector
/// included, ready to pass straight to `eth_call` as the `data` field.
///
/// Encoding shape (standard Solidity ABI head/tail encoding for a
/// `(address,bool,bytes)[]` argument, since each tuple element is itself
/// dynamic because of its `bytes` field):
///
/// ```text
/// [selector]
/// [offset to array data]                = 0x20 (the only argument)
/// -- array data (starts at the offset above) --
/// [array length N]
/// [N tuple offsets]  (relative to the start of this elements region,
///                     i.e. right after the length word)
/// [tuple 0 body][tuple 1 body]...        each body:
///     [address target]
///     [bool allowFailure]
///     [offset to bytes, relative to the start of this tuple's body]
///     [bytes length][bytes data, padded]
/// ```
pub fn encode_aggregate3(calls: &[Call3]) -> String {
    let n = calls.len();

    let mut tuple_bodies: Vec<Vec<u8>> = Vec::with_capacity(n);
    for c in calls {
        let mut body = Vec::new();
        body.extend_from_slice(&word_address(&c.target));
        body.extend_from_slice(&word_bool(false)); // allowFailure = false
        body.extend_from_slice(&word_uint256(96)); // offset to bytes: 3 head words
        body.extend_from_slice(&encode_bytes_value(&c.call_data));
        tuple_bodies.push(body);
    }

    let elements_region_head_len = 32 * n as u64;
    let mut tuple_offsets: Vec<[u8; 32]> = Vec::with_capacity(n);
    let mut running = elements_region_head_len;
    for body in &tuple_bodies {
        tuple_offsets.push(word_uint256(running));
        running += body.len() as u64;
    }

    let mut array_data = Vec::new();
    array_data.extend_from_slice(&word_uint256(n as u64));
    for off in &tuple_offsets {
        array_data.extend_from_slice(off);
    }
    for body in &tuple_bodies {
        array_data.extend_from_slice(body);
    }

    let mut out = String::with_capacity(8 + 64 + array_data.len() * 2);
    out.push_str(SEL_AGGREGATE3);
    out.push_str(&to_hex(&word_uint256(32))); // offset to the sole argument's data
    out.push_str(&to_hex(&array_data));
    out
}

fn read_uint256(word: &[u8]) -> u64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&word[24..32]);
    u64::from_be_bytes(buf)
}

/// Decodes `aggregate3`'s `Result[] returnData` back into each call's raw
/// `returnData` bytes, in the same order `calls` was submitted in.
///
/// Panics if any entry reports `success = false` — unreachable in
/// practice given every call above is submitted with
/// `allowFailure = false` (see module doc comment), so this is a
/// defensive assertion, not a real error path.
pub fn decode_aggregate3_result(data: &[u8]) -> Vec<Vec<u8>> {
    let array_data_offset = read_uint256(&data[0..32]) as usize;
    let array_base = array_data_offset;
    let n = read_uint256(&data[array_base..array_base + 32]) as usize;
    let elements_region_start = array_base + 32;

    let mut results = Vec::with_capacity(n);
    for i in 0..n {
        let offset_word =
            &data[elements_region_start + i * 32..elements_region_start + i * 32 + 32];
        let tuple_offset = read_uint256(offset_word) as usize;
        let tuple_start = elements_region_start + tuple_offset;

        let success = data[tuple_start + 31] != 0;
        assert!(
            success,
            "Multicall3 aggregate3 reported success=false for call index {i} despite allowFailure=false — should be unreachable"
        );

        let bytes_offset = read_uint256(&data[tuple_start + 32..tuple_start + 64]) as usize;
        let bytes_start = tuple_start + bytes_offset;
        let len = read_uint256(&data[bytes_start..bytes_start + 32]) as usize;
        let payload = data[bytes_start + 32..bytes_start + 32 + len].to_vec();
        results.push(payload);
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trips a synthetic aggregate3 response through the decoder
    /// to check the offset/length arithmetic independently of any real
    /// RPC call — this is the part with no test coverage before this
    /// module existed, and the part most likely to have an off-by-32 bug.
    #[test]
    fn decode_round_trips_two_results_of_different_lengths() {
        // Build a synthetic Result[] by hand: two entries, (true, 0x1122)
        // and (true, 0xaabbccdd), mirroring exactly what a real
        // aggregate3 response looks like.
        let payloads: Vec<Vec<u8>> = vec![vec![0x11, 0x22], vec![0xaa, 0xbb, 0xcc, 0xdd]];

        let mut tuple_bodies = Vec::new();
        for p in &payloads {
            let mut body = Vec::new();
            body.extend_from_slice(&word_bool(true));
            body.extend_from_slice(&word_uint256(64)); // offset to bytes: 2 head words
            body.extend_from_slice(&encode_bytes_value(p));
            tuple_bodies.push(body);
        }
        let n = payloads.len() as u64;
        let head_len = 32 * n;
        let mut offsets = Vec::new();
        let mut running = head_len;
        for body in &tuple_bodies {
            offsets.push(word_uint256(running));
            running += body.len() as u64;
        }
        let mut array_data = Vec::new();
        array_data.extend_from_slice(&word_uint256(n));
        for o in &offsets {
            array_data.extend_from_slice(o);
        }
        for b in &tuple_bodies {
            array_data.extend_from_slice(b);
        }

        let mut full = Vec::new();
        full.extend_from_slice(&word_uint256(32));
        full.extend_from_slice(&array_data);

        let decoded = decode_aggregate3_result(&full);
        assert_eq!(decoded, payloads);
    }

    #[test]
    fn encode_aggregate3_starts_with_verified_selector() {
        let calls = vec![Call3::new(
            "0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc",
            "0902f1ac",
        )];
        let encoded = encode_aggregate3(&calls);
        assert!(encoded.starts_with(SEL_AGGREGATE3));
    }

    #[test]
    fn encode_then_decode_shape_is_internally_consistent() {
        // Not a real RPC round-trip (no network here), but checks that
        // encode_aggregate3 produces calldata with the write number of
        // calls represented in its own array-length word, which is the
        // one invariant checkable without live state.
        let calls = vec![
            Call3::new("0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc", "0902f1ac"),
            Call3::new("0xbEbc44782C7dB0a1A60Cb6fe97d0b483032FF1C7", "f446c1d0"),
            Call3::new("0xbEbc44782C7dB0a1A60Cb6fe97d0b483032FF1C7", "ddca3f43"),
        ];
        let encoded = encode_aggregate3(&calls);
        let bytes = from_hex_bytes(&encoded[8..]); // strip selector
        let array_offset = read_uint256(&bytes[0..32]) as usize;
        let n = read_uint256(&bytes[array_offset..array_offset + 32]);
        assert_eq!(n, 3);
    }
}
