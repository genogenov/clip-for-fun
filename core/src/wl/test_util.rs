// Wire-format builders for tests that play the compositor side of the socket.

pub(crate) fn msg(object_id: u32, opcode: u16, body: &[u8]) -> Vec<u8> {
    let mut m = Vec::with_capacity(8 + body.len());
    m.extend_from_slice(&object_id.to_ne_bytes());
    m.extend_from_slice(&(((8 + body.len()) as u32) << 16 | u32::from(opcode)).to_ne_bytes());
    m.extend_from_slice(body);
    m
}

pub(crate) fn wl_str(s: &str) -> Vec<u8> {
    let mut out = ((s.len() + 1) as u32).to_ne_bytes().to_vec();
    out.extend_from_slice(s.as_bytes());
    out.push(0);
    out.resize(out.len().next_multiple_of(4), 0);
    out
}
