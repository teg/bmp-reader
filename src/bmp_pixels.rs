use bitreader::BitReader;
use bmp_header::{BMPHeader,BMPError,BMPVersion};
use byteorder::{LittleEndian,ReadBytesExt};
use std::io::{self,Read,Seek,SeekFrom};

pub struct Pixel {
    pub red: u32,
    pub green: u32,
    pub blue: u32,
    pub alpha: u32,
}

fn upscale(from: u32, bits: u8) -> u32 {
    let mut to = from;

    for _ in 1..(32/bits) {
        to = to << bits | from;
    }

    if 32 % bits != 0 {
        to = to << 32 % bits | from >> (32 - 32 % bits);
    }

    to
}

fn mask(px: u32, mask: u32) -> u32 {
    return upscale((px & mask) >> mask.trailing_zeros(), !(mask >> mask.trailing_zeros()).trailing_zeros() as u8);
}

fn mask_or_zeros(px: u32, msk: u32) -> u32 {
    if msk == 0 {
        return 0;
    } else {
        return mask(px, msk);
    }
}

fn mask_or_ones(px: u32, msk: u32) -> u32 {
    if msk == 0 {
        return !0u32;
    } else {
        return mask(px, msk);
    }
}

impl Pixel {
    fn from_pallete_pixel(px: &PalletePixel) -> Pixel {
        Pixel{
            red: upscale(px.red as u32, 8),
            green: upscale(px.green as u32, 8),
            blue: upscale(px.blue as u32, 8),
            alpha: !0u32,
        }
    }

    fn from_bitfields(px: u32, red: u32, green: u32, blue: u32, alpha: u32) -> Pixel {
        Pixel {
            red: mask_or_zeros(px, red),
            green: mask_or_zeros(px, green),
            blue: mask_or_zeros(px, blue),
            alpha: mask_or_ones(px, alpha),
        }
    }
}

pub struct PalletePixel {
    red: u8,
    green: u8,
    blue: u8,
}

pub enum Pixels<'a, R: Read + Seek + 'a> {
    OneBPP(Vec<PalletePixel>, BitReader<'a, R>),
    TwoBPP(Vec<PalletePixel>, BitReader<'a, R>),
    FourBPP(Vec<PalletePixel>, BitReader<'a, R>),
    EightBPP(Vec<PalletePixel>, &'a mut R),
    SixteenBPP(u16, u16, u16, u16, &'a mut R),
    TwentyFourBPP(&'a mut R),
    ThirtyTwoBPP(u32, u32, u32, u32, &'a mut R),
}

impl<'a, R: Read + Seek + 'a> Pixels<'a, R> {
    fn from_header(header: &BMPHeader,
                   pallete: Vec<PalletePixel>,
                   source: &'a mut R) -> Result<Pixels<'a, R>, BMPError> {
        match header.bpp {
            1 => Ok(Pixels::OneBPP(pallete, BitReader::new(source, 1))),
            2 => Ok(Pixels::TwoBPP(pallete, BitReader::new(source, 2))),
            4 => Ok(Pixels::FourBPP(pallete, BitReader::new(source, 4))),
            8 => Ok(Pixels::EightBPP(pallete, source)),
            16 => Ok(Pixels::SixteenBPP(
                                        header.red_mask as u16,
                                        header.green_mask as u16,
                                        header.blue_mask as u16,
                                        header.alpha_mask as u16,
                                        source)),
            24 => Ok(Pixels::TwentyFourBPP(source)),
            32 => Ok(Pixels::ThirtyTwoBPP(
                                        header.red_mask,
                                        header.green_mask,
                                        header.blue_mask,
                                        header.alpha_mask,
                                        source)),
            _ => Err(BMPError::UnsupportedBitsPerPixel(header.bpp)),
        }
    }

    pub fn new(mut source: &'a mut R) -> Result<(Pixels<'a, R>, u32, i32), BMPError> {
        let header = BMPHeader::from_buffer(&mut source)?;
        let mut pallete = Vec::with_capacity(header.n_colors as usize);

        match header.version {
            BMPVersion::Two => {
                for _ in 0..header.n_colors {
                    let mut px = [0; 3];
                    source.read(&mut px)?;
                    pallete.push(PalletePixel{red: px[2], green: px[1], blue: px[0]});
                }
            },
            _ => {
                for _ in 0..header.n_colors {
                    let mut px = [0; 4];
                    source.read(&mut px)?;
                    pallete.push(PalletePixel{red: px[2], green: px[1], blue: px[0]});
                }
            },
        }

        let current_offset = source.seek(SeekFrom::Current(0))?;
        if current_offset > header.pixel_offset {
            return Err(BMPError::HeaderTooLarge(current_offset, header.pixel_offset));
        }
        source.seek(SeekFrom::Start(header.pixel_offset))?;

        Ok((Pixels::from_header(&header, pallete, source)?, header.width, header.height))
    }

    pub fn seek_to_byte_boundary(&mut self, align: u64) -> Result<(), io::Error> {
        match self {
            &mut Pixels::OneBPP(_, ref mut reader) |
            &mut Pixels::TwoBPP(_, ref mut reader) |
            &mut Pixels::FourBPP(_, ref mut reader) => {
                return reader.seek_to_byte_boundary(align);
            },
            &mut Pixels::EightBPP(_, ref mut reader) |
            &mut Pixels::SixteenBPP(_, _, _, _, ref mut reader) |
            &mut Pixels::TwentyFourBPP(ref mut reader) |
            &mut Pixels::ThirtyTwoBPP(_, _, _, _, ref mut reader) => {
                let position = reader.seek(SeekFrom::Current(0))?;

                if position % align != 0 {
                    reader.seek(SeekFrom::Current((align - (position % align)) as i64))?;
                }

                return Ok(());
            }
        }
    }

    pub fn next_pixel(&mut self) -> Result<Pixel, io::Error> {
        match self {
            &mut Pixels::OneBPP(ref pallete, ref mut reader) |
            &mut Pixels::TwoBPP(ref pallete, ref mut reader) |
            &mut Pixels::FourBPP(ref pallete, ref mut reader) => {
                Ok(Pixel::from_pallete_pixel(&pallete[reader.read_bits()? as usize]))
            },
            &mut Pixels::EightBPP(ref pallete, ref mut reader) => {
                Ok(Pixel::from_pallete_pixel(&pallete[reader.read_u8()? as usize]))
            },
            &mut Pixels::SixteenBPP(red_mask, green_mask, blue_mask, alpha_mask, ref mut reader) => {
                return Ok(Pixel::from_bitfields(reader.read_u16::<LittleEndian>()? as u32,
                                                red_mask as u32,
                                                green_mask as u32,
                                                blue_mask as u32,
                                                alpha_mask as u32))
            },
            &mut Pixels::TwentyFourBPP(ref mut reader) => {
                let mut px = [0; 3];
                reader.read(&mut px)?;

                return Ok(Pixel::from_pallete_pixel(&PalletePixel{red: px[0],
                                                                 green: px[1],
                                                                 blue: px[2]}))
            },
            &mut Pixels::ThirtyTwoBPP(red_mask, green_mask, blue_mask, alpha_mask, ref mut reader) => {
                return Ok(Pixel::from_bitfields(reader.read_u32::<LittleEndian>()?,
                                                red_mask,
                                                green_mask,
                                                blue_mask,
                                                alpha_mask))
            },
        }
    }
}

