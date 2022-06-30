use crate::{Result, VadFile};
use chrono::{DateTime, NaiveDateTime, Utc};
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use std::io::{Read, Write};

const BASE_URL: &str = "http://tgftp.nws.noaa.gov/SL.us008001/DF.of/DC.radar/DS.48vwp";

fn get_color(altitude: f32) -> String {
    match altitude {
        x if x < 1. => "220 0 220".to_string(),
        x if x < 3. => "255 0 0".to_string(),
        x if x < 6. => "0 255 0".to_string(),
        x if x < 9. => "255 255 0".to_string(),
        _ => "0 255 255".to_string(),
    }
}

pub struct VadClient;

impl VadClient {
    pub fn update(&self) -> Result<()> {
        if self.download_new()? {
            self.create_placefile()?;
        }

        Ok(())
    }

    fn write_radar(&self, placefile: &mut impl Write, radar: &str) -> Result<()> {
        let is_tdwr = radar.starts_with('t');

        let url = format!("{BASE_URL}/SI.{radar}/sn.last");
        let reader = ureq::get(&url).call()?.into_reader();
        let vad_file = VadFile::from_reader(reader)?;

        if vad_file.data.prof.len() < 2 {
            return Ok(());
        }

        let max_size = vad_file
            .data
            .prof
            .iter()
            .cloned()
            .map(|m| m.wind_spd)
            .reduce(f32::max)
            .unwrap()
            .max(40.);

        // Calculate size
        let size = {
            if is_tdwr {
                (40. / max_size) / 300.
            } else {
                (40. / max_size) / 100.
            }
        };

        // Draw cirlces
        for i in (20..=max_size as u32).step_by(20) {
            writeln!(placefile, "Color: 100 100 100")?;
            writeln!(placefile, "Line: 1, 0")?;

            for a in 0..=60 {
                let angle = 2.0 * std::f32::consts::PI * a as f32 / 60.;
                let x = vad_file.location.0
                    + i as f32 * angle.cos() * vad_file.location.0.to_radians().cos() * size;
                let y = vad_file.location.1 + i as f32 * angle.sin() * size;

                writeln!(placefile, "{x}, {y}")?;
            }

            writeln!(placefile, "End:\n")?;
        }

        // let bunkers = vad_file
        //     .data
        //     .bunkers()
        //     .map_or(("NA".into(), "NA".into()), |b| {
        //         (b.0.to_string(), b.1.to_string())
        //     });

        let text = format!(
            "VWP valid {} UTC",
            vad_file.time.format("%m/%d/%Y %H%M"),
            // vad_file
            //     .data
            //     .mean_wind(6.)
            //     .map_or("NA".into(), |b| b.to_string()),
            // bunkers.0,
            // bunkers.1
        );

        let mut draw_color = String::new();

        for (i, m) in vad_file.data.prof.iter().enumerate() {
            let components = m.comp().flip();

            let x = vad_file.location.0
                + components.u() * vad_file.location.0.to_radians().cos() * size;
            let y = vad_file.location.1 + components.v() * size;

            let color = get_color(m.altitude);

            if i == 0 {
                write!(placefile, "Color: {color}\nLine: 3, 0, \"{text}\"\n")?;
                draw_color = color;
            } else if color != draw_color && i != vad_file.data.prof.len() - 1 {
                // Connect and finish last line
                writeln!(placefile, "{x}, {y}\nEnd:\n")?;

                // Start new line
                write!(placefile, "Color: {color}\nLine: 3, 0, \"{text}\"\n")?;
                draw_color = color;
            }

            writeln!(placefile, "{x}, {y}")?;
        }

        writeln!(placefile, "End:\n")?;

        Ok(())
    }

    fn fetch_times(&self) -> Result<HashMap<String, i64>> {
        lazy_static! {
            static ref DATES: Regex = Regex::new(r"\w{2}-\w{3}-\d{4} \d{2}:\d{2}").unwrap();
            static ref RADARS: Regex = Regex::new(r">SI.(\w{4})").unwrap();
        };

        let mut req = ureq::get(BASE_URL).call()?.into_string();
        let mut i = 0;

        while let Err(e) = req {
            req = ureq::get(BASE_URL).call()?.into_string();
            i += 1;
            if i == 10 {
                return Err(e.into());
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        let req = req.unwrap();

        let radars: Vec<String> = RADARS
            .captures_iter(&req)
            .map(|c| c.get(1).unwrap().as_str().to_string())
            .collect();

        let times: Vec<i64> = DATES
            .find_iter(&req)
            .map(|c| {
                NaiveDateTime::parse_from_str(c.as_str(), "%d-%b-%Y %H:%M")
                    .unwrap()
                    .timestamp()
            })
            .collect();

        assert!(
            radars.len() == times.len(),
            "Length of radars does not match length of times."
        );

        let map = HashMap::from_iter(radars.into_iter().zip(times.into_iter()));

        Ok(map)
    }

    fn cache_radar(&self, radar: &str, time: i64) -> Result<()> {
        let path = format!("./cache/{radar}.{time}");
        let mut file = Vec::new();
        match self.write_radar(&mut file, radar) {
            Ok(_) => std::fs::File::create(path)?.write_all(&file)?,
            Err(_) => {
                // eprintln!("Error: {e}");
                return Ok(());
            }
        }

        Ok(())
    }

    fn download_new(&self) -> Result<bool> {
        let files: Vec<String> = std::fs::read_dir("./cache/")?
            .map(|d| d.unwrap().file_name().to_str().unwrap().to_string())
            .collect();

        let mut new_files = false;

        let times = self.fetch_times().unwrap();
        for (radar, time) in times {
            let old_file = files.iter().find(|f| f.starts_with(&radar));

            if let Some(f) = old_file {
                // If new file
                if f.split('.').last().unwrap().parse::<i64>()? != time {
                    let old = format!("./cache/{f}");
                    let new = format!("./cache/{radar}.{time}");

                    std::fs::rename(old, new)?;
                    self.cache_radar(&radar, time).unwrap();
                    new_files = true;
                }
            } else {
                // No file exists
                self.cache_radar(&radar, time).unwrap();
            }
        }

        Ok(new_files)
    }

    fn create_placefile(&self) -> Result<()> {
        let mut bytes = Vec::new();
        for file_result in std::fs::read_dir("./cache/")? {
            let file = file_result?;

            // Timestamp
            let time = file
                .file_name()
                .into_string()
                .unwrap()
                .split('.')
                .last()
                .unwrap()
                .parse::<i64>()?;

            // If find is from last 20 minuties
            let datetime = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(time, 0), Utc);
            if (datetime - Utc::now()).num_minutes() < 20 {
                std::fs::File::open(file.path())?.read_to_end(&mut bytes)?;
            }
        }

        let mut placefile = std::fs::File::create("vwp_hodographs")?;
        writeln!(&mut placefile, "Title: VWP Hodographs")?;
        writeln!(&mut placefile, "Refresh: 1\n")?;
        placefile.write_all(&bytes)?;

        Ok(())
    }
}
