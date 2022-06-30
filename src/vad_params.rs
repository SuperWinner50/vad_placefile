use std::ops::{Add, Sub};

pub struct Vector(f32, f32);
pub struct Comp(f32, f32);

impl Comp {
    pub fn u(&self) -> f32 {
        self.0
    }

    pub fn v(&self) -> f32 {
        self.1
    }

    pub fn flip(self) -> Comp {
        Comp(self.1, self.0)
    }
}

impl From<Comp> for Vector {
    fn from(comp: Comp) -> Vector {
        Vector(
            90. - (-comp.1).atan2(-comp.0).to_degrees(),
            comp.0.hypot(comp.1),
        )
    }
}

impl Add for Vector {
    type Output = Self;
    fn add(self, other: Vector) -> Vector {
        (Comp::from(self) + Comp::from(other)).into()
    }
}

impl Sub for Vector {
    type Output = Self;
    fn sub(self, other: Vector) -> Vector {
        (Comp::from(self) - Comp::from(other)).into()
    }
}

impl std::fmt::Display for Vector {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:.0}/{:.0}", self.0.rem_euclid(360.), self.1)
    }
}

impl From<Vector> for Comp {
    fn from(vec: Vector) -> Comp {
        Comp(
            -vec.1 * vec.0.to_radians().sin(),
            -vec.1 * vec.0.to_radians().cos(),
        )
    }
}

impl Add for Comp {
    type Output = Self;
    fn add(self, other: Comp) -> Comp {
        Comp(self.u() + other.u(), self.v() + other.v())
    }
}

impl Sub for Comp {
    type Output = Self;
    fn sub(self, other: Comp) -> Comp {
        Comp(self.u() - other.u(), self.v() - other.v())
    }
}

impl std::fmt::Display for Comp {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:.0}, {:.0}", self.0, self.1)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VadMessage {
    pub wind_dir: f32,
    pub wind_spd: f32,
    pub altitude: f32,
}

impl VadMessage {
    pub fn comp(&self) -> Comp {
        Vector(self.wind_dir, self.wind_spd).into()
    }
}

pub struct VadProfile {
    pub prof: Vec<VadMessage>,
}

impl Default for VadProfile {
    fn default() -> Self {
        Self::new()
    }
}

impl VadProfile {
    pub fn new() -> Self {
        Self { prof: Vec::new() }
    }

    fn altitude(&self) -> Vec<f32> {
        self.prof.iter().map(|m| m.altitude).collect()
    }

    fn u(&self) -> Vec<f32> {
        self.prof
            .iter()
            .map(|m| -m.wind_spd * m.wind_dir.to_radians().sin())
            .collect()
    }

    fn v(&self) -> Vec<f32> {
        self.prof
            .iter()
            .map(|m| -m.wind_spd * m.wind_dir.to_radians().cos())
            .collect()
    }

    fn interp_height(&self, height: f32) -> Option<Comp> {
        Some(Comp(
            interp(height, &self.altitude(), &self.u())?,
            interp(height, &self.altitude(), &self.v())?,
        ))
    }

    pub fn mean_wind(&self, top: f32) -> Option<Vector> {
        if self.prof.is_empty() || top >= *self.altitude().last()? {
            return None;
        }

        let xs: Vec<f32> = (self.prof[0].altitude.ceil() as u32..top as u32)
            .map(|v| v as f32 / 1000.)
            .collect();

        let (u, v): (Vec<f32>, Vec<f32>) = xs
            .iter()
            .map(|&v| self.interp_height(v).map(|v| (v.u(), v.v())).unwrap())
            .unzip();

        Some(
            Comp(
                v.into_iter().sum::<f32>() / xs.len() as f32,
                u.into_iter().sum::<f32>() / xs.len() as f32,
            )
            .into(),
        )
    }

    pub fn wind_shear(&self, bot: f32, top: f32) -> Option<Vector> {
        if self.prof.is_empty() {
            return None;
        }

        Some((self.interp_height(top)? - self.interp_height(bot)?).into())
    }

    pub fn bunkers(&self) -> Option<(Vector, Vector)> {
        if self.prof.is_empty() {
            return None;
        }

        let Comp(mnu6, mnv6) = self.mean_wind(6.)?.into();
        let Comp(shru, shrv) = self.wind_shear(0., 6.)?.into();

        let d = 7.5f32.ms_to_kts();
        let tmp = d / shru.hypot(shrv);

        let rs = Comp(mnu6 + (tmp * shrv), mnv6 - (tmp * shru));
        let ls = Comp(mnu6 - (tmp * shrv), mnv6 + (tmp * shru));

        Some((rs.into(), ls.into()))
    }

    // fn helicity(&self, profile: Vec<VadMessage>, bottom: f32, top: f32) -> f32 {}
}

trait ConvertUnits {
    fn ms_to_kts(self) -> Self;
}

impl ConvertUnits for f32 {
    fn ms_to_kts(self) -> f32 {
        self * 1.94384
    }
}

fn interp(mut x: f32, xp: &[f32], yp: &[f32]) -> Option<f32> {
    assert!(xp.len() == yp.len(), "x and y len do not match");

    if x < xp[0] {
        x = xp[0];
    }

    let i: usize = match xp.iter().enumerate().find(|(_, &b)| x >= b) {
        Some(m) => m.0,
        None => return None,
    };

    Some(yp[i] + (x - xp[i]) * (yp[i + 1] - yp[i]) / (xp[i + 1] - xp[i]))
}
