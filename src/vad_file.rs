use crate::Result;
use crate::{VadError, VadMessage, VadProfile};
use std::io::Read;

use chrono::{DateTime, Duration, TimeZone, Utc};

// Possible use a read trait instead of a macro in the future?
macro_rules! read {
    ($reader:expr, $ty:ty, $len:expr) => {{
        let mut buf = [0u8; $len * std::mem::size_of::<$ty>()];
        let res: [$ty; $len] = $reader.read_exact(&mut buf).map(|_| {
            buf.chunks_exact(std::mem::size_of::<$ty>())
                .map(|v| <$ty>::from_be_bytes(v.try_into().unwrap()))
                .collect::<Vec<$ty>>()
                .try_into()
                .unwrap()
        })?;

        res
    }};

    ($reader:expr, $ty:ty) => {{
        let mut buf = [0u8; std::mem::size_of::<$ty>()];
        $reader
            .read_exact(&mut buf)
            .map(|_| <$ty>::from_be_bytes(buf))?
    }};
}

macro_rules! read_vec {
    ($reader:expr, $ty:ty, $len:expr) => {{
        let mut buf = vec![0u8; $len * std::mem::size_of::<$ty>()];
        $reader.read(buf.as_mut_slice()).map(|_| {
            buf.chunks_exact(std::mem::size_of::<$ty>())
                .map(|v| <$ty>::from_be_bytes(v.try_into().unwrap()))
                .collect::<Vec<_>>()
        })?
    }};
}

macro_rules! read_string {
    ($reader:expr, $len:expr) => {{
        let mut buf = vec![0u8; $len];

        $reader
            .read_exact(buf.as_mut_slice())
            .map(|_| buf.into_iter().map(|v| v as char).collect::<String>())?
    }};
}

pub struct VadFile {
    pub data: VadProfile,
    pub location: (f32, f32),
    pub time: DateTime<Utc>,
}

impl VadFile {
    pub fn from_reader(mut reader: impl Read) -> Result<Self> {
        read_headers(&mut reader)?;
        let (time, location, has_tabular) = read_desc_block(&mut reader)?;
        let mut messages = Vec::new();

        if has_tabular {
            messages = read_tabular(&mut reader)?;
        }

        let data = get_data(messages)?;

        Ok(Self {
            data,
            location,
            time,
        })
    }
}

fn get_data(messages: Vec<Vec<String>>) -> Result<VadProfile> {
    let mut vad_list = Vec::new();
    for page in &messages {
        if page[0].trim().starts_with("VAD Algorithm Output") {
            vad_list.extend(page[3..].to_owned());
        }
    }

    let mut data = VadProfile::new();

    for line in vad_list {
        let values = line.split_whitespace().collect::<Vec<_>>();
        let r_e: f32 = 4. / 3. * 6371.;

        let wind_dir = values[4].parse::<f32>()?;
        let wind_spd = values[5].parse::<f32>()?;
        let slant_range = values[8].parse::<f32>()? * 6076.1 / 3281.;
        let elev_angle = values[9].parse::<f32>()?;

        let altitude = (r_e.powi(2)
            + slant_range.powi(2)
            + 2.0 * r_e * slant_range * elev_angle.to_radians().sin())
        .sqrt()
            - r_e;

        data.prof.push(VadMessage {
            wind_dir,
            wind_spd,
            altitude,
        });
    }

    data.prof
        .sort_by(|m1, m2| m1.altitude.partial_cmp(&m2.altitude).unwrap());

    Ok(data)
}

fn read_headers(reader: &mut impl Read) -> Result<()> {
    let _wmo_header = read!(reader, u8, 30);
    let _message_date = read!(reader, i16);
    let _message_code = read!(reader, i16);
    let _message_time = read!(reader, i32);
    let _message_length = read!(reader, i32);
    let _source_id = read!(reader, i16);
    let _dest_id = read!(reader, i16);
    let _num_blocks = read!(reader, i16);

    Ok(())
}

fn read_desc_block(reader: &mut impl Read) -> Result<(DateTime<Utc>, (f32, f32), bool)> {
    // Block separator
    read!(reader, i16);

    let lat = read!(reader, i32) as f32 / 1000.0;
    let lon = read!(reader, i32) as f32 / 1000.0;

    let _radar_elev = read!(reader, i16);

    let product_code = read!(reader, i16);
    assert!(
        product_code == 48,
        "This is not a VWP file, found code {product_code} instead."
    );

    let _operation_mode = read!(reader, i16);
    let _vcp = read!(reader, i16);
    let _req_sequence_number = read!(reader, i16);
    let _vol_sequence_number = read!(reader, i16);

    let scan_date = read!(reader, i16);
    let scan_time = read!(reader, i32);

    let _product_date = read!(reader, i16);
    let _product_time = read!(reader, i32);

    // Unused variables
    read!(reader, i16, 27);

    let _version = read!(reader, i8);
    let _spot_blank = read!(reader, i8);

    let offset_symbology = read!(reader, i32);
    let _offset_graphic = read!(reader, i32);
    let offset_tabular = read!(reader, i32);

    let time = Utc.ymd(1969, 12, 31).and_hms(0, 0, 0)
        + Duration::days(scan_date as i64)
        + Duration::seconds(scan_time as i64);

    if offset_symbology > 0 {
        read_symbology(reader)?;
    }

    Ok((time, (lat, lon), offset_tabular > 0))
}

fn read_symbology(reader: &mut impl Read) -> Result<()> {
    // Block separator
    read!(reader, i16);

    let block_id = read!(reader, i16);
    if block_id != 1 {
        return Err(VadError::SymbologyBlockError.into());
    }

    let _block_length = read!(reader, i32);
    let _num_layers = read!(reader, i16);
    let _layer_sep = read!(reader, i16);
    let layer_num_bytes = read!(reader, i32);
    let _block_data = read_vec!(reader, i16, layer_num_bytes as usize / 2);

    Ok(())
}

fn read_tabular(reader: &mut impl Read) -> Result<Vec<Vec<String>>> {
    // Block separator
    read!(reader, i16);

    let block_id = read!(reader, i16);
    if block_id != 3 {
        return Err(VadError::TabularBlockError.into());
    }

    let _block_size = read!(reader, i32);

    // Unknown bytes
    read!(reader, u8, 30);

    let _product_code = read!(reader, i16);
    let _operation_mode = read!(reader, i16);
    let _vcp = read!(reader, i16);
    let _req_seq_number = read!(reader, i16);
    let _vol_seq_numbe = read!(reader, i16);

    let _scan_date = read!(reader, i16);
    let _scan_time = read!(reader, i32);
    let _product_date = read!(reader, i16);
    let _product_time = read!(reader, i32);

    // Unused variables
    read!(reader, i16, 27);

    let _version = read!(reader, i8);
    let _spot_blank = read!(reader, i8);

    let _offset_symbology = read!(reader, i32);
    let _offset_graphic = read!(reader, i32);
    let _offset_tabular = read!(reader, i32);
    // Block separator
    read!(reader, i16);
    let num_pages = read!(reader, i16);

    let mut messages = Vec::new();

    for _ in 0..num_pages {
        let mut message = Vec::new();
        let mut num_chars = read!(reader, i16);

        while num_chars != -1 {
            message.push(read_string!(reader, num_chars as usize));
            num_chars = read!(reader, i16);
        }

        messages.push(message);
    }

    Ok(messages)
}
