use std::io::{self,Read,Seek,SeekFrom};

pub struct BitReader<'a, R: Read + Seek + 'a> {
    byte: u8,
    n_bits_remaining: u8,
    n_bits_per_chunk: u8,
    source: &'a mut R,
}

impl<'a, R: Read + Seek + 'a> BitReader<'a, R> {
    pub fn new(source: &'a mut R, n_bits_per_chunk: u8) -> BitReader<'a, R> {
        assert!(n_bits_per_chunk != 0 && n_bits_per_chunk <= 8 && 8 % n_bits_per_chunk == 0);

        BitReader {
            byte: 0,
            n_bits_remaining: 0,
            n_bits_per_chunk,
            source,
        }
    }

    pub fn read_bits(&mut self) -> Result<u8, io::Error> {
        if self.n_bits_per_chunk == 8 {
            let mut byte = [0];
            self.source.read(&mut byte)?;
            Ok(byte[0])
        } else {
            if self.n_bits_remaining == 0 {
                let mut byte = [0];
                self.source.read(&mut byte)?;
                self.byte = byte[0];
                self.n_bits_remaining = 8;
            }

            let result = self.byte & ((!0u8) >> (8 - self.n_bits_per_chunk));
            self.byte = self.byte >> self.n_bits_per_chunk;
            self.n_bits_remaining -= self.n_bits_per_chunk;

            Ok(result)
        }
    }

    pub fn seek_to_byte_boundary(&mut self, align: u64) -> Result<(), io::Error> {
        let position = self.source.seek(SeekFrom::Current(0))?;

        if position % align != 0 {
            self.source.seek(SeekFrom::Current((align - (position % align)) as i64))?;
        }

        self.byte = 0;
        self.n_bits_remaining = 0;

        return Ok(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn test_constructor_one<R: Read + Seek>(bytes: &mut R, n_bits: u8, expected: u8) {
            let mut bitreader = BitReader::new(bytes, n_bits);

            assert!(bitreader.read_bits().unwrap() == expected);
            assert!(bitreader.read_bits().unwrap() == expected);
    }

    #[test]
    fn test_constructor() {
        let mut buff = Cursor::new(vec![!0; 128]);

        test_constructor_one(&mut buff, 1, 1);
        test_constructor_one(&mut buff, 2, 3);
        test_constructor_one(&mut buff, 4, 15);
        test_constructor_one(&mut buff, 8, 255);
    }
}
