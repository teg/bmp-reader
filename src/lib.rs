extern crate byteorder;

mod bitreader;
mod bmp_header;
mod bmp_pixels;

use bmp_header::{BMPError};
use bmp_pixels::{Pixel,Pixels};
use std::io::{self,Read,Seek};

pub struct BMPReader<'a, R: Read + Seek + 'a> {
    pixels: Pixels<'a, R>,
    bottom_up: bool,
    width: usize,
    height: usize,
    x: usize,
    y: usize,
}

impl<'a, R: Read + Seek + 'a> BMPReader<'a, R> {
    pub fn new(mut source: &mut R) -> Result<BMPReader<R>, BMPError> {
        let (pixels, width, height) = Pixels::new(source)?;

        Ok(BMPReader {
            pixels,
            width: width as usize,
            height: height.abs() as usize,
            bottom_up: height > 0,
            x: 0,
            y: 0,
        })
    }

    pub fn get_width(&self) -> usize {
        self.width
    }

    pub fn get_height(&self) -> usize {
        self.height
    }

    fn get_y(&self) -> usize {
        if self.bottom_up {
            self.y
        } else {
            self.height - self.y
        }
    }
}

impl<'a, R: Read + Seek + 'a> Iterator for BMPReader<'a, R> {
    type Item = (usize, usize, Result<Pixel, io::Error>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.x >= self.width {
            self.x = 0;
            if self.y >= self.height {
                return None;
            } else {
                self.y += 1;
                if let Err(err) = self.pixels.seek_to_byte_boundary(4) {
                    return Some((self.x, self.get_y(), Err(err)));
                }
            }
        } else {
            self.x += 1;
        }

        Some((self.x, self.get_y(), self.pixels.next_pixel()))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}
