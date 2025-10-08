use core::cmp::min;

pub const PREAMBLE_SIZE: usize = 2;
pub const PREAMBLE: [u8; PREAMBLE_SIZE] = [0b10101011u8, 0b10001111];

pub const DATA_SIZE: usize = 16;

pub const BUFFER_SIZE: usize = PREAMBLE_SIZE + 2 * DATA_SIZE;

pub struct Encoder {
    buffer: [u8; BUFFER_SIZE],
    input_pos: usize,
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Encoder {
    pub fn new() -> Self {
        Encoder {
            buffer: [0u8; BUFFER_SIZE],
            input_pos: 0,
        }
    }

    pub fn get_data_slice(&mut self) -> &mut [u8] {
        &mut self.buffer[PREAMBLE_SIZE..PREAMBLE_SIZE + DATA_SIZE]
    }

    pub fn get_encoded_slice(&self) -> &[u8] {
        &self.buffer
    }

    pub fn encode(&mut self) -> &[u8; BUFFER_SIZE] {
        self.buffer[0..PREAMBLE_SIZE].copy_from_slice(&PREAMBLE);
        self.buffer.copy_within(
            PREAMBLE_SIZE..PREAMBLE_SIZE + DATA_SIZE,
            PREAMBLE_SIZE + DATA_SIZE,
        );
        self.encode_buffer();
        &self.buffer
    }

    fn encode_buffer(&mut self) {
        for i in PREAMBLE_SIZE..PREAMBLE_SIZE + DATA_SIZE {
            self.buffer[i] &= 0b01010101u8;
            self.buffer[i] |= (!(self.buffer[i] << 1)) & 0b10101010u8;
        }
        for i in PREAMBLE_SIZE + DATA_SIZE..PREAMBLE_SIZE + 2 * DATA_SIZE {
            self.buffer[i] &= 0b10101010u8;
            self.buffer[i] |= (!(self.buffer[i] >> 1)) & 0b01010101u8;
        }
    }

    pub fn decode(&mut self) -> &[u8] {
        for i in PREAMBLE_SIZE..PREAMBLE_SIZE + DATA_SIZE {
            self.buffer[i] &= 0b01010101u8;
        }
        for i in PREAMBLE_SIZE + DATA_SIZE..PREAMBLE_SIZE + 2 * DATA_SIZE {
            self.buffer[i - DATA_SIZE] |= self.buffer[i] & 0b10101010u8;
        }
        &self.buffer[PREAMBLE_SIZE..PREAMBLE_SIZE + DATA_SIZE]
    }

    pub fn encoded_input(&mut self, input: &[u8], bytes: usize) -> (usize, bool) {
        let mut n = 0;
        for input in input.iter().take(min(input.len(), bytes)) {
            if self.input_pos >= PREAMBLE_SIZE {
                self.buffer[self.input_pos] = *input;
                self.input_pos += 1;
            } else if *input == PREAMBLE[self.input_pos] {
                self.input_pos += 1;
            } else {
                self.input_pos = 0;
            }
            n += 1;
            if self.input_pos == self.buffer.len() {
                return (n, true);
            }
        }
        (n, false)
    }
}
