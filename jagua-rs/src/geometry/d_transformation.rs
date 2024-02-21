use std::borrow::Borrow;
use std::fmt::Display;

use ordered_float::NotNan;

use crate::geometry::transformation::Transformation;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// A proper rigid transformation, decomposed into a rotation followed by a translation.
pub struct DTransformation {
    /// The rotation in radians
    pub rotation: NotNan<f64>,
    /// The translation in the x and y-axis
    pub translation: (NotNan<f64>, NotNan<f64>),
}

impl DTransformation {
    pub fn new(rotation: f64, translation: (f64, f64)) -> Self {
        Self {
            rotation: NotNan::new(rotation).expect("rotation is NaN"),
            translation: (
                NotNan::new(translation.0).expect("translation.0 is NaN"),
                NotNan::new(translation.1).expect("translation.1 is NaN"),
            ),
        }
    }

    pub const fn empty() -> Self {
        const _0: NotNan<f64> = unsafe { NotNan::new_unchecked(0.0) };
        Self {
            rotation: _0,
            translation: (_0, _0),
        }
    }

    pub fn rotation(&self) -> f64 {
        self.rotation.into()
    }

    pub fn translation(&self) -> (f64, f64) {
        (self.translation.0.into(), self.translation.1.into())
    }

    pub fn compose(&self) -> Transformation {
        Transformation::from_rotation(self.rotation()).translate(self.translation())
    }
}

impl<T> From<T> for DTransformation where T: Borrow<Transformation> {
    fn from(t: T) -> Self {
        t.borrow().decompose()
    }
}

impl Display for DTransformation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "r: {:.3}°, t: ({:.3}, {:.3})", self.rotation.to_degrees(), self.translation.0.into_inner(), self.translation.1.into_inner())
    }
}