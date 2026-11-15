use crate::url::NetError;

/// Decompresses an RFC 1952 GZIP stream and verifies its CRC32 + ISIZE trailer.
pub fn decompress_gzip(input: &[u8]) -> Result<Vec<u8>, NetError> {
    if input.len() < 18 || input[0] != 0x1F || input[1] != 0x8B || input[2] != 8 {
        return Err(NetError::Decompress("invalid gzip header".into()));
    }
    let flg = input[3];
    let mut pos = 10usize;
    if flg & 0x04 != 0 {
        if pos + 2 > input.len() {
            return Err(NetError::Decompress("truncated gzip FEXTRA".into()));
        }
        let xlen = u16::from_le_bytes([input[pos], input[pos + 1]]) as usize;
        pos += 2 + xlen;
    }
    if flg & 0x08 != 0 {
        while pos < input.len() && input[pos] != 0 {
            pos += 1;
        }
        pos += 1;
    }
    if flg & 0x10 != 0 {
        while pos < input.len() && input[pos] != 0 {
            pos += 1;
        }
        pos += 1;
    }
    if flg & 0x02 != 0 {
        pos += 2;
    }
    if pos + 8 > input.len() {
        return Err(NetError::Decompress("truncated gzip payload".into()));
    }

    let deflate_slice = &input[pos..input.len() - 8];
    let out = decompress_deflate(deflate_slice)?;

    let trailer = &input[input.len() - 8..];
    let expected_crc = u32::from_le_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);
    let expected_isize = u32::from_le_bytes([trailer[4], trailer[5], trailer[6], trailer[7]]);
    if crc32(&out) != expected_crc {
        return Err(NetError::Decompress("gzip CRC32 mismatch".into()));
    }
    if (out.len() as u32) != expected_isize {
        return Err(NetError::Decompress("gzip ISIZE mismatch".into()));
    }
    Ok(out)
}

/// Decompresses an RFC 1950 ZLIB-wrapped DEFLATE stream.
pub fn decompress_zlib(input: &[u8]) -> Result<Vec<u8>, NetError> {
    if input.len() < 6 {
        return Err(NetError::Decompress("truncated zlib stream".into()));
    }
    let cmf = input[0];
    let flg = input[1];
    if (cmf & 0x0F) != 8 || (((cmf as u16) * 256 + flg as u16) % 31 != 0) {
        return Err(NetError::Decompress("invalid zlib header".into()));
    }
    let start = if flg & 0x20 != 0 { 6 } else { 2 };
    if start > input.len() {
        return Err(NetError::Decompress("truncated zlib dict".into()));
    }
    decompress_deflate(&input[start..input.len().saturating_sub(4)])
}

/// Decompresses a raw RFC 1951 DEFLATE stream.
pub fn decompress_deflate(input: &[u8]) -> Result<Vec<u8>, NetError> {
    let mut reader = BitReader::new(input);
    let mut out = Vec::new();

    loop {
        let bfinal = reader.read_bits(1)?;
        let btype = reader.read_bits(2)?;
        match btype {
            0 => {
                reader.align_byte();
                let len = reader.read_u16_le()?;
                let nlen = reader.read_u16_le()?;
                if len ^ 0xFFFF != nlen {
                    return Err(NetError::Decompress("invalid stored block length".into()));
                }
                let bytes = reader.read_bytes(len as usize)?;
                out.extend_from_slice(bytes);
            }
            1 => {
                let (lit_tree, dist_tree) = fixed_trees();
                decode_huffman_block(&mut reader, &lit_tree, &dist_tree, &mut out)?;
            }
            2 => {
                let (lit_tree, dist_tree) = read_dynamic_trees(&mut reader)?;
                decode_huffman_block(&mut reader, &lit_tree, &dist_tree, &mut out)?;
            }
            _ => return Err(NetError::Decompress("invalid DEFLATE BTYPE=3".into())),
        }
        if bfinal == 1 {
            break;
        }
    }

    Ok(out)
}

struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_buf: u32,
    bit_cnt: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_buf: 0,
            bit_cnt: 0,
        }
    }

    fn ensure(&mut self, n: u8) -> Result<(), NetError> {
        while self.bit_cnt < n {
            if self.byte_pos >= self.data.len() {
                return Err(NetError::Decompress("unexpected EOF in DEFLATE stream".into()));
            }
            self.bit_buf |= (self.data[self.byte_pos] as u32) << self.bit_cnt;
            self.byte_pos += 1;
            self.bit_cnt += 8;
        }
        Ok(())
    }

    fn read_bits(&mut self, n: u8) -> Result<u32, NetError> {
        if n == 0 {
            return Ok(0);
        }
        self.ensure(n)?;
        let mask = (1u32 << n) - 1;
        let val = self.bit_buf & mask;
        self.bit_buf >>= n;
        self.bit_cnt -= n;
        Ok(val)
    }

    fn align_byte(&mut self) {
        let drop = self.bit_cnt % 8;
        self.bit_buf >>= drop;
        self.bit_cnt -= drop;
    }

    fn read_u16_le(&mut self) -> Result<u16, NetError> {
        let lo = self.read_bits(8)? as u16;
        let hi = self.read_bits(8)? as u16;
        Ok(lo | (hi << 8))
    }

    fn read_bytes(&mut self, count: usize) -> Result<&'a [u8], NetError> {
        // Consume any buffered whole bytes first (align_byte was called, bit_cnt is multiple of 8)
        let unread_buf_bytes = (self.bit_cnt / 8) as usize;
        self.byte_pos -= unread_buf_bytes;
        self.bit_buf = 0;
        self.bit_cnt = 0;
        if self.byte_pos + count > self.data.len() {
            return Err(NetError::Decompress("truncated stored block".into()));
        }
        let slice = &self.data[self.byte_pos..self.byte_pos + count];
        self.byte_pos += count;
        Ok(slice)
    }
}

struct HuffTree {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl HuffTree {
    fn from_lengths(lengths: &[u8]) -> Result<Self, NetError> {
        let mut counts = [0u16; 16];
        for &len in lengths {
            if len as usize >= counts.len() {
                return Err(NetError::Decompress("huffman code length > 15".into()));
            }
            if len > 0 {
                counts[len as usize] += 1;
            }
        }
        let mut offsets = [0u16; 16];
        let mut total = 0u16;
        for bits in 1..16 {
            offsets[bits] = total;
            total += counts[bits];
        }
        let mut symbols = vec![0u16; total as usize];
        for (sym, &len) in lengths.iter().enumerate() {
            if len > 0 {
                let idx = offsets[len as usize] as usize;
                symbols[idx] = sym as u16;
                offsets[len as usize] += 1;
            }
        }
        Ok(Self { counts, symbols })
    }

    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, NetError> {
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0i32;
        for len in 1..16 {
            code |= reader.read_bits(1)? as i32;
            let count = self.counts[len] as i32;
            if code - count < first {
                return Ok(self.symbols[(index + (code - first)) as usize]);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err(NetError::Decompress("invalid huffman code".into()))
    }
}

fn fixed_trees() -> (HuffTree, HuffTree) {
    let mut lit = [0u8; 288];
    lit[0..144].fill(8);
    lit[144..256].fill(9);
    lit[256..280].fill(7);
    lit[280..288].fill(8);
    let dist = [5u8; 32];
    (
        HuffTree::from_lengths(&lit).unwrap(),
        HuffTree::from_lengths(&dist).unwrap(),
    )
}

fn read_dynamic_trees(reader: &mut BitReader<'_>) -> Result<(HuffTree, HuffTree), NetError> {
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let hlit = reader.read_bits(5)? as usize + 257;
    let hdist = reader.read_bits(5)? as usize + 1;
    let hclen = reader.read_bits(4)? as usize + 4;

    let mut cl_lens = [0u8; 19];
    for i in 0..hclen {
        cl_lens[ORDER[i]] = reader.read_bits(3)? as u8;
    }
    let cl_tree = HuffTree::from_lengths(&cl_lens)?;

    let total = hlit + hdist;
    let mut lengths = Vec::with_capacity(total);
    while lengths.len() < total {
        let sym = cl_tree.decode(reader)?;
        match sym {
            0..=15 => lengths.push(sym as u8),
            16 => {
                let prev = *lengths
                    .last()
                    .ok_or_else(|| NetError::Decompress("repeat with no previous length".into()))?;
                let rep = reader.read_bits(2)? as usize + 3;
                lengths.resize(lengths.len() + rep, prev);
            }
            17 => {
                let rep = reader.read_bits(3)? as usize + 3;
                lengths.resize(lengths.len() + rep, 0);
            }
            18 => {
                let rep = reader.read_bits(7)? as usize + 11;
                lengths.resize(lengths.len() + rep, 0);
            }
            _ => return Err(NetError::Decompress("invalid code-length symbol".into())),
        }
    }
    if lengths.len() != total {
        return Err(NetError::Decompress("code-length overflow".into()));
    }
    let lit_tree = HuffTree::from_lengths(&lengths[..hlit])?;
    let dist_tree = HuffTree::from_lengths(&lengths[hlit..])?;
    Ok((lit_tree, dist_tree))
}

fn decode_huffman_block(
    reader: &mut BitReader<'_>,
    lit_tree: &HuffTree,
    dist_tree: &HuffTree,
    out: &mut Vec<u8>,
) -> Result<(), NetError> {
    const LEN_BASE: [u16; 29] = [
        3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
        131, 163, 195, 227, 258,
    ];
    const LEN_EXTRA: [u8; 29] = [
        0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
    ];
    const DIST_BASE: [u16; 30] = [
        1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
        2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
    ];
    const DIST_EXTRA: [u8; 30] = [
        0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12,
        13, 13,
    ];

    loop {
        let sym = lit_tree.decode(reader)?;
        match sym {
            0..=255 => out.push(sym as u8),
            256 => break,
            257..=285 => {
                let idx = (sym - 257) as usize;
                let len = LEN_BASE[idx] as usize + reader.read_bits(LEN_EXTRA[idx])? as usize;
                let dist_sym = dist_tree.decode(reader)? as usize;
                if dist_sym >= DIST_BASE.len() {
                    return Err(NetError::Decompress("invalid distance symbol".into()));
                }
                let dist =
                    DIST_BASE[dist_sym] as usize + reader.read_bits(DIST_EXTRA[dist_sym])? as usize;
                if dist == 0 || dist > out.len() {
                    return Err(NetError::Decompress("invalid LZ77 back-reference".into()));
                }
                for _ in 0..len {
                    let b = out[out.len() - dist];
                    out.push(b);
                }
            }
            _ => return Err(NetError::Decompress("invalid literal/length symbol".into())),
        }
    }
    Ok(())
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}
