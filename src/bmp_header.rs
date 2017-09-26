use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{self,Read,Seek,SeekFrom};

const BMP_BITFIELD32_RED: u32   = 0x00ff0000;
const BMP_BITFIELD32_GREEN: u32 = 0x0000ff00;
const BMP_BITFIELD32_BLUE: u32  = 0x000000ff;
const BMP_BITFIELD16_RED: u16   = 0b0111110000000000;
const BMP_BITFIELD16_GREEN: u16 = 0b0000001111100000;
const BMP_BITFIELD16_BLUE: u16  = 0b0000000000011111;

pub enum BMPError {
    WrongMagicNumbers(u8, u8),
    UnsupportedHeaderSize(u32),
    UnsupportedNumberOfPlanes(u16),
    UnsupportedCompressionType(u32),
    UnsupportedBitsPerPixel(u16),
    BitfieldsNotSupportedForPixelDepth(u16),
    BitfieldsNotContiguous(u32, u32, u32, u32),
    BitfieldsOverlap(u32, u32, u32, u32),
    InvalidWidth(i32),
    InvalidHeight(i32),
    HeaderTooLarge(u64, u64),
    IOError(io::Error),
}

impl From<io::Error> for BMPError {
    fn from(err: io::Error) -> BMPError {
        BMPError::IOError(err)
    }
}

enum CompressionType {
    RGB,
    Bitfields,
    AlphaBitfields,
}

impl CompressionType {
    fn from_u32(val: u32) -> Result<CompressionType, BMPError> {
        match val {
            0 => Ok(CompressionType::RGB),
            3 => Ok(CompressionType::Bitfields),
            6 => Ok(CompressionType::AlphaBitfields),
            _ => Err(BMPError::UnsupportedCompressionType(val)),
        }
    }
}

#[derive(Copy,Clone)]
pub enum BMPVersion {
    Two,
    Three,
    Four,
    Five,
}

impl BMPVersion {
    fn from_dib_header_size(val: u32) -> Result<BMPVersion, BMPError> {
        match val {
            12 => Ok(BMPVersion::Two),
            40 => Ok(BMPVersion::Three),
            108 => Ok(BMPVersion::Four),
            124 => Ok(BMPVersion::Five),
            _ => Err(BMPError::UnsupportedHeaderSize(val)),
        }
    }
}

pub struct BMPHeader {
    pub version: BMPVersion,
    pub width: u32,
    pub height: i32,
    pub bpp: u16,
    pub n_colors: u32,
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
    pub alpha_mask: u32,
    pub pixel_offset: u64,
}

fn mask_is_contiguous(mask: u32) -> bool {
    if mask == 0 {
        return true;
    }

    let mask = mask >> mask.trailing_zeros();

    if mask == 0 {
        true;
    }

    let mask = mask >> mask.trailing_zeros();

    if mask == 0 {
        return true;
    }

    return false;
}

impl BMPHeader {
    fn new(version: BMPVersion, width: i32, height: i32, planes: u16, bpp: u16, n_colors: u32, pixel_offset: u64) -> Result<BMPHeader, BMPError> {
        if width <= 0 {
            return Err(BMPError::InvalidWidth(width));
        }

        if height == 0 {
            return Err(BMPError::InvalidHeight(height));
        }

        if planes != 1 {
            return Err(BMPError::UnsupportedNumberOfPlanes(planes));
        }

        Ok(BMPHeader {
            version,
            width: width.abs() as u32,
            height,
            bpp,
            n_colors: if bpp < 16 && (n_colors == 0 || n_colors > 1 << bpp) {
                1 << bpp
            } else {
                0
            },
            red_mask: match bpp {
                16 => BMP_BITFIELD16_RED as u32,
                32 => BMP_BITFIELD32_RED,
                _ => 0,
            },
            green_mask: match bpp {
                16 => BMP_BITFIELD16_GREEN as u32,
                32 => BMP_BITFIELD32_GREEN,
                _ => 0,
            },
            blue_mask: match bpp {
                16 => BMP_BITFIELD16_BLUE as u32,
                32 => BMP_BITFIELD32_BLUE,
                _ => 0,
            },
            alpha_mask: 0,
            pixel_offset,
        })
    }

    fn set_masks(&mut self, red_mask: u32, green_mask: u32, blue_mask: u32, alpha_mask: u32) -> Result<(),BMPError> {
        match self.bpp {
            16 | 32 => (),
            bpp => return Err(BMPError::BitfieldsNotSupportedForPixelDepth(bpp)),
        }

        if !mask_is_contiguous(red_mask) || !mask_is_contiguous(green_mask) ||
           !mask_is_contiguous(blue_mask) || !mask_is_contiguous(alpha_mask) {
            return Err(BMPError::BitfieldsNotContiguous(red_mask, green_mask, blue_mask, alpha_mask));
        }

        if red_mask & green_mask != 0 || red_mask & blue_mask != 0 || red_mask & alpha_mask != 0 ||
           green_mask & blue_mask != 0 || green_mask & alpha_mask != 0 || blue_mask & alpha_mask != 0 {
            return Err(BMPError::BitfieldsOverlap(red_mask, green_mask, blue_mask, alpha_mask));
        }

        self.red_mask = red_mask;
        self.green_mask = green_mask;
        self.blue_mask = blue_mask;
        self.alpha_mask = alpha_mask;
        Ok(())
    }

    fn from_v2_buffer<R: Read + Seek>(source: &mut R, pixel_offset: u64) -> Result<BMPHeader, BMPError> {
        let width = source.read_u16::<LittleEndian>()? as i32;
        let height = source.read_u16::<LittleEndian>()? as i32;
        let planes =source.read_u16::<LittleEndian>()?;
        let bpp = source.read_u16::<LittleEndian>()?;

        BMPHeader::new(BMPVersion::Two, width, height, planes, bpp, 0, pixel_offset)
    }

    fn from_v3_buffer<R: Read + Seek>(source: &mut R, version: BMPVersion, pixel_offset: u64) -> Result<BMPHeader, BMPError> {
        let width = source.read_i32::<LittleEndian>()?;
        let height = source.read_i32::<LittleEndian>()?;
        let planes =source.read_u16::<LittleEndian>()?;
        let bpp = source.read_u16::<LittleEndian>()?;
        let compression = CompressionType::from_u32(source.read_u32::<LittleEndian>()?)?;
        source.seek(SeekFrom::Current(12))?; /* skip ImageSize, XRes and YRes */
        let n_colors = source.read_u32::<LittleEndian>()?;
        source.seek(SeekFrom::Current(4))?; /* skip ColorsImportant */
        let mut header = BMPHeader::new(version, width, height, planes, bpp, n_colors, pixel_offset)?;

        match version {
            BMPVersion::Two => panic!(),
            BMPVersion::Three => {
                match compression {
                    CompressionType::RGB => (),
                    CompressionType::Bitfields | CompressionType::AlphaBitfields => {
                        header.set_masks(source.read_u32::<LittleEndian>()?,
                                         source.read_u32::<LittleEndian>()?,
                                         source.read_u32::<LittleEndian>()?,
                                         0)?;
                    },
                }
            },
            BMPVersion::Four => {
                match compression {
                    CompressionType::RGB => {
                        /* We ignore the rest of the v4 header. */
                        source.seek(SeekFrom::Current(72))?;
                    },
                    CompressionType::Bitfields | CompressionType::AlphaBitfields => {
                        header.set_masks(source.read_u32::<LittleEndian>()?,
                                         source.read_u32::<LittleEndian>()?,
                                         source.read_u32::<LittleEndian>()?,
                                         source.read_u32::<LittleEndian>()?)?;
                        /* We ignore the rest of the v4 header. */
                        source.seek(SeekFrom::Current(56))?;
                    },
                }
            },
            BMPVersion::Five => {
                match compression {
                    CompressionType::RGB => {
                        /* We ignore the rest of the v5 header. */
                        source.seek(SeekFrom::Current(84))?;
                    },
                    CompressionType::Bitfields | CompressionType::AlphaBitfields => {
                        header.set_masks(source.read_u32::<LittleEndian>()?,
                                         source.read_u32::<LittleEndian>()?,
                                         source.read_u32::<LittleEndian>()?,
                                         source.read_u32::<LittleEndian>()?)?;
                        /* We ignore the rest of the v4 header. */
                        source.seek(SeekFrom::Current(68))?;
                    },
                }
            },
        }

        Ok(header)
    }

    pub fn from_buffer<R: Read + Seek>(mut source: &mut R) -> Result<BMPHeader, BMPError> {
        let mut bm = [0, 0];

        source.read(&mut bm)?;
        if bm != b"BM"[..] {
            return Err(BMPError::WrongMagicNumbers(bm[0], bm[1]));
        }

        /* Skip the 32bit file size, and 32 reserved bits. */
        source.seek(SeekFrom::Current(8))?;

        /* Read the offset to the pixel array. */
        let pixel_offset = source.read_u16::<LittleEndian>()? as u64;
        let version = BMPVersion::from_dib_header_size(source.read_u32::<LittleEndian>()?)?;
        let header = match version {
            BMPVersion::Two => BMPHeader::from_v2_buffer(&mut source, pixel_offset)?,
            BMPVersion::Three | BMPVersion::Four | BMPVersion::Five => BMPHeader::from_v3_buffer(&mut source, version, pixel_offset)?,
        };

        Ok(header)
    }
}

