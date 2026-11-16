//! MTProto obfuscated transport: the 64-byte init packet, the four stream
//! ciphers of a proxied connection and the packet splitter.
//!
//! Ported from Flowseal/tg-ws-proxy (MIT), `proxy/tg_ws_proxy.py` and
//! `proxy/bridge.py`; see `licenses/THIRD-PARTY.md`.
use aes::Aes256;
use ctr::cipher::{KeyIvInit, StreamCipher};
use sha2::{Digest, Sha256};

pub type Cipher = ctr::Ctr128BE<Aes256>;

pub const INIT_LEN: usize = 64;
pub const SECRET_LEN: usize = 16;
/// Key (32 bytes) and IV (16 bytes) sit at 8..56 of the init packet.
const KEY_AT: usize = 8;
const IV_AT: usize = 40;
const TAG_AT: usize = 56;
const DC_AT: usize = 60;
/// Test datacenters are numbered from here in the client's init packet.
const TEST_DC: u16 = 10000;
/// Largest transport packet kept whole for one WebSocket frame. A longer
/// length is not a real packet: the stream is passed through unsplit instead
/// of being buffered without limit.
const MAX_PACKET: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Abridged,
    Intermediate,
    /// Padded intermediate, what a `dd` secret asks the client to use.
    Padded,
}
impl Transport {
    fn tag(self) -> [u8; 4] {
        match self {
            Self::Abridged => [0xef; 4],
            Self::Intermediate => [0xee; 4],
            Self::Padded => [0xdd; 4],
        }
    }
    fn from_tag(tag: &[u8]) -> Option<Self> {
        [Self::Abridged, Self::Intermediate, Self::Padded]
            .into_iter()
            .find(|t| t.tag() == tag)
    }
}

/// What the client's init packet says once decrypted with the proxy secret.
#[derive(Debug, Clone)]
pub struct ClientInit {
    pub dc: u16,
    pub media: bool,
    pub test: bool,
    pub transport: Transport,
    key_iv: [u8; 48],
}
impl ClientInit {
    /// The datacenter as Telegram reads it from an init packet: negative for
    /// media connections.
    pub fn dc_index(&self) -> i16 {
        let dc = self.dc.min(i16::MAX as u16) as i16;
        if self.media { -dc } else { dc }
    }
}

fn cipher(key: &[u8], iv: &[u8]) -> Cipher {
    Cipher::new_from_slices(key, iv).expect("32-byte key and 16-byte IV")
}

/// The client side of a proxied connection mixes the proxy secret into the key.
fn secret_cipher(key_iv: &[u8; 48], secret: &[u8; SECRET_LEN]) -> Cipher {
    let key = Sha256::new()
        .chain_update(&key_iv[..32])
        .chain_update(secret)
        .finalize();
    cipher(&key, &key_iv[32..])
}

fn reversed(key_iv: &[u8]) -> [u8; 48] {
    let mut out: [u8; 48] = key_iv.try_into().expect("48 bytes of key and IV");
    out.reverse();
    out
}

/// `None` for a wrong secret or an unknown transport.
pub fn parse_client_init(init: &[u8; INIT_LEN], secret: &[u8; SECRET_LEN]) -> Option<ClientInit> {
    let key_iv: [u8; 48] = init[KEY_AT..TAG_AT].try_into().ok()?;
    let mut plain = *init;
    secret_cipher(&key_iv, secret).apply_keystream(&mut plain);
    let transport = Transport::from_tag(&plain[TAG_AT..DC_AT])?;
    let index = i16::from_le_bytes([plain[DC_AT], plain[DC_AT + 1]]);
    let mut dc = index.unsigned_abs();
    let test = dc >= TEST_DC;
    if test {
        dc -= TEST_DC;
    }
    Some(ClientInit {
        dc,
        media: index < 0,
        test,
        transport,
        key_iv,
    })
}

/// Random bytes that Telegram will not take for another transport.
fn random_init() -> [u8; INIT_LEN] {
    const RESERVED_STARTS: [&[u8; 4]; 7] = [
        b"HEAD",
        b"POST",
        b"GET ",
        b"OPTI",
        &[0xee; 4],
        &[0xdd; 4],
        &[0x16, 0x03, 0x01, 0x02],
    ];
    loop {
        let mut init = [0u8; INIT_LEN];
        getrandom::fill(&mut init).expect("system random source");
        let start: &[u8; 4] = init[..4].try_into().expect("four bytes");
        if init[0] != 0xef && !RESERVED_STARTS.contains(&start) && init[4..8] != [0; 4] {
            return init;
        }
    }
}

/// The proxy's own init packet for Telegram: plain obfuscation, no secret.
pub fn relay_init(transport: Transport, dc_index: i16) -> [u8; INIT_LEN] {
    let mut init = random_init();
    let mut plain = init;
    plain[TAG_AT..DC_AT].copy_from_slice(&transport.tag());
    plain[DC_AT..DC_AT + 2].copy_from_slice(&dc_index.to_le_bytes());
    cipher(&init[KEY_AT..IV_AT], &init[IV_AT..TAG_AT]).apply_keystream(&mut plain);
    // Only the tail is sent encrypted; the key and IV stay readable.
    init[TAG_AT..].copy_from_slice(&plain[TAG_AT..]);
    init
}

/// Client ciphertext is decrypted and encrypted again for Telegram, and back.
pub struct Ciphers {
    pub from_client: Cipher,
    pub to_client: Cipher,
    pub to_telegram: Cipher,
    pub from_telegram: Cipher,
}

pub fn ciphers(client: &ClientInit, secret: &[u8; SECRET_LEN], relay: &[u8; INIT_LEN]) -> Ciphers {
    let skip_init = |mut cipher: Cipher| {
        cipher.apply_keystream(&mut [0u8; INIT_LEN]);
        cipher
    };
    let relay_back = reversed(&relay[KEY_AT..TAG_AT]);
    Ciphers {
        from_client: skip_init(secret_cipher(&client.key_iv, secret)),
        to_client: secret_cipher(&reversed(&client.key_iv), secret),
        to_telegram: skip_init(cipher(&relay[KEY_AT..IV_AT], &relay[IV_AT..TAG_AT])),
        from_telegram: cipher(&relay_back[..32], &relay_back[32..]),
    }
}

/// Splits the client's stream into MTProto transport packets, so that each
/// goes to the gateway as one WebSocket frame.
pub struct Splitter {
    transport: Transport,
    cipher: Vec<u8>,
    plain: Vec<u8>,
    /// A packet did not parse: the rest of the stream goes through as it comes.
    disabled: bool,
}
impl Splitter {
    pub fn new(transport: Transport) -> Self {
        Self {
            transport,
            cipher: Vec::new(),
            plain: Vec::new(),
            disabled: false,
        }
    }

    /// `cipher` is what goes to Telegram, `plain` the same bytes decrypted;
    /// lengths are read from `plain`, whole packets are cut from `cipher`.
    pub fn split(&mut self, cipher: &[u8], plain: &[u8]) -> Vec<Vec<u8>> {
        if cipher.is_empty() {
            return vec![];
        }
        if self.disabled {
            return vec![cipher.to_vec()];
        }
        self.cipher.extend_from_slice(cipher);
        self.plain.extend_from_slice(plain);
        let mut parts = Vec::new();
        let mut offset = 0;
        while offset < self.cipher.len() {
            match self.packet_len(offset) {
                Packet::Incomplete => break,
                Packet::Unknown => {
                    parts.push(self.cipher[offset..].to_vec());
                    offset = self.cipher.len();
                    self.disabled = true;
                }
                Packet::Whole(len) => {
                    parts.push(self.cipher[offset..offset + len].to_vec());
                    offset += len;
                }
            }
        }
        self.cipher.drain(..offset);
        self.plain.drain(..offset);
        parts
    }

    /// What is left when the client closes mid-packet.
    pub fn flush(&mut self) -> Option<Vec<u8>> {
        self.plain.clear();
        (!self.cipher.is_empty()).then(|| std::mem::take(&mut self.cipher))
    }

    fn packet_len(&self, offset: usize) -> Packet {
        let plain = &self.plain[offset..];
        let (header, payload) = match self.transport {
            Transport::Abridged => match plain[0] {
                0x7f | 0xff => {
                    if plain.len() < 4 {
                        return Packet::Incomplete;
                    }
                    let words = u32::from_le_bytes([plain[1], plain[2], plain[3], 0]);
                    (4, words as usize * 4)
                }
                first => (1, (first & 0x7f) as usize * 4),
            },
            Transport::Intermediate | Transport::Padded => {
                if plain.len() < 4 {
                    return Packet::Incomplete;
                }
                let len = u32::from_le_bytes([plain[0], plain[1], plain[2], plain[3]]);
                (4, (len & 0x7fff_ffff) as usize)
            }
        };
        if payload == 0 || payload > MAX_PACKET {
            Packet::Unknown
        } else if plain.len() < header + payload {
            Packet::Incomplete
        } else {
            Packet::Whole(header + payload)
        }
    }
}

enum Packet {
    Whole(usize),
    Incomplete,
    Unknown,
}

/// The client's half of the handshake, as Telegram Desktop does it: the init
/// packet, then the ciphers for sending and receiving. For tests and the live
/// check; the proxy itself never plays the client.
pub fn client_init(
    secret: &[u8; SECRET_LEN],
    transport: Transport,
    dc_index: i16,
) -> ([u8; INIT_LEN], Cipher, Cipher) {
    let mut init = random_init();
    init[TAG_AT..DC_AT].copy_from_slice(&transport.tag());
    init[DC_AT..DC_AT + 2].copy_from_slice(&dc_index.to_le_bytes());
    let key_iv: [u8; 48] = init[KEY_AT..TAG_AT].try_into().expect("48 bytes");
    let mut send = secret_cipher(&key_iv, secret);
    let mut encrypted = init;
    send.apply_keystream(&mut encrypted);
    init[TAG_AT..].copy_from_slice(&encrypted[TAG_AT..]);
    (init, send, secret_cipher(&reversed(&key_iv), secret))
}

/// Telegram's half: what a relay init packet asks for, then the ciphers for
/// receiving and sending. For tests.
pub fn server_side(init: &[u8; INIT_LEN]) -> Option<(Transport, i16, Cipher, Cipher)> {
    let mut receive = cipher(&init[KEY_AT..IV_AT], &init[IV_AT..TAG_AT]);
    let mut plain = *init;
    receive.apply_keystream(&mut plain);
    let transport = Transport::from_tag(&plain[TAG_AT..DC_AT])?;
    let index = i16::from_le_bytes([plain[DC_AT], plain[DC_AT + 1]]);
    let back = reversed(&init[KEY_AT..TAG_AT]);
    Some((transport, index, receive, cipher(&back[..32], &back[32..])))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: [u8; SECRET_LEN] = [7; SECRET_LEN];

    #[test]
    fn init_packet_gives_datacenter_media_and_transport() {
        let (init, ..) = client_init(&SECRET, Transport::Padded, -4);
        let client = parse_client_init(&init, &SECRET).unwrap();
        assert_eq!((client.dc, client.media, client.test), (4, true, false));
        assert_eq!(client.transport, Transport::Padded);
        assert_eq!(client.dc_index(), -4);

        let (init, ..) = client_init(&SECRET, Transport::Abridged, 10002);
        let client = parse_client_init(&init, &SECRET).unwrap();
        assert_eq!((client.dc, client.media, client.test), (2, false, true));
        assert_eq!(client.dc_index(), 2);
    }

    #[test]
    fn wrong_secret_is_rejected() {
        // A wrong key gives a random tag: 3 valid tags out of 2^32.
        let (init, ..) = client_init(&SECRET, Transport::Intermediate, 2);
        assert!(parse_client_init(&init, &[8; SECRET_LEN]).is_none());
    }

    #[test]
    fn data_survives_both_re_encryptions() {
        let (init, mut client_send, mut client_receive) =
            client_init(&SECRET, Transport::Intermediate, 2);
        let client = parse_client_init(&init, &SECRET).unwrap();
        let relay = relay_init(client.transport, client.dc_index());
        let mut proxy = ciphers(&client, &SECRET, &relay);
        let (transport, index, mut telegram_receive, mut telegram_send) =
            server_side(&relay).unwrap();
        assert_eq!((transport, index), (Transport::Intermediate, 2));

        for message in [&b"first message"[..], &b"second"[..]] {
            let mut data = message.to_vec();
            client_send.apply_keystream(&mut data);
            proxy.from_client.apply_keystream(&mut data);
            assert_eq!(data, message);
            proxy.to_telegram.apply_keystream(&mut data);
            telegram_receive.apply_keystream(&mut data);
            assert_eq!(data, message);

            telegram_send.apply_keystream(&mut data);
            proxy.from_telegram.apply_keystream(&mut data);
            assert_eq!(data, message);
            proxy.to_client.apply_keystream(&mut data);
            client_receive.apply_keystream(&mut data);
            assert_eq!(data, message);
        }
    }

    fn intermediate(payload: &[u8]) -> Vec<u8> {
        let mut packet = (payload.len() as u32).to_le_bytes().to_vec();
        packet.extend_from_slice(payload);
        packet
    }

    #[test]
    fn splitter_cuts_whole_packets_across_reads() {
        let mut splitter = Splitter::new(Transport::Padded);
        let first = intermediate(&[1; 12]);
        let second = intermediate(&[2; 40]);
        let stream = [first.clone(), second.clone()].concat();
        // The same bytes stand for plain and cipher: only lengths matter here.
        let (a, b) = stream.split_at(20);
        assert_eq!(splitter.split(a, a), vec![first]);
        assert_eq!(splitter.split(b, b), vec![second]);
        assert_eq!(splitter.flush(), None);

        let (a, b) = stream.split_at(2);
        assert!(splitter.split(a, a).is_empty());
        assert_eq!(splitter.split(b, b).len(), 2);
    }

    #[test]
    fn splitter_reads_abridged_lengths() {
        let mut splitter = Splitter::new(Transport::Abridged);
        let short = [vec![2u8], vec![9; 8]].concat();
        let long = [vec![0x7f, 0x80, 0, 0], vec![5; 0x80 * 4]].concat();
        let stream = [short.clone(), long.clone()].concat();
        assert_eq!(splitter.split(&stream, &stream), vec![short, long]);
    }

    #[test]
    fn splitter_passes_a_broken_stream_through() {
        let mut splitter = Splitter::new(Transport::Intermediate);
        let broken = 0x7fff_ffffu32.to_le_bytes();
        assert_eq!(splitter.split(&broken, &broken), vec![broken.to_vec()]);
        assert_eq!(splitter.split(b"rest", b"rest"), vec![b"rest".to_vec()]);

        let mut splitter = Splitter::new(Transport::Intermediate);
        let partial = &intermediate(&[1; 12])[..9];
        assert!(splitter.split(partial, partial).is_empty());
        assert_eq!(splitter.flush(), Some(partial.to_vec()));
    }
}
