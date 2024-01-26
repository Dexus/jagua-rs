use std::cmp::Ordering;

use itertools::Itertools;

use crate::collision_detection::haz_prox_grid::proximity::Proximity;
use crate::collision_detection::hazards::hazard::Hazard;
use crate::collision_detection::hazards::hazard_entity::HazardEntity;
use crate::geometry::primitives::aa_rectangle::AARectangle;
use crate::geometry::primitives::circle::Circle;
use crate::geometry::geo_traits::{DistanceFrom, Shape};
use crate::geometry::primitives::point::Point;
use crate::geometry::geo_enums::GeoPosition;
use crate::N_QUALITIES;

#[derive(Clone, Debug)]
pub struct HPCell {
    bbox: AARectangle,
    centroid: Point,
    radius: f64,
    uni_haz_prox: (Proximity, HazardEntity),
    //proximity of closest hazard which is universally applicable (bin or item)
    static_uni_haz_prox: (Proximity, HazardEntity),
    //proximity of universal static hazards
    qz_haz_prox: [Proximity; N_QUALITIES], //proximity of closest quality zone for each quality
}

impl HPCell {
    pub fn new(bbox: AARectangle, static_hazards: &[Hazard]) -> Self {
        //Calculate the exact distance to the edge bin (add new method in shape trait to do this)
        //For each of the distinct quality zones in a bin, calculate the distance to the closest zone
        let centroid = bbox.centroid();
        let radius = f64::sqrt(f64::powi(bbox.width() / 2.0, 2) + f64::powi(bbox.height() / 2.0, 2));

        let mut static_uni_haz_prox = (Proximity::default(), HazardEntity::BinOuter);
        let mut qz_haz_prox = [Proximity::default(); N_QUALITIES];

        for hazard in static_hazards {
            let (pos, distance) = hazard.shape().distance_from_border(&centroid);
            let prox = match pos == hazard.entity().presence() {
                true => Proximity::new(GeoPosition::Interior, distance), //cell in hazard, negative distance
                false => Proximity::new(GeoPosition::Exterior, distance)
            };
            match hazard.entity() {
                HazardEntity::BinOuter | HazardEntity::BinHole { .. } => {
                    if prox < static_uni_haz_prox.0 {
                        static_uni_haz_prox = (prox, hazard.entity().clone());
                    }
                }
                HazardEntity::QualityZoneInferior { quality, .. } => {
                    qz_haz_prox[*quality] = qz_haz_prox[*quality].min(prox);
                }
                _ => panic!("Unexpected hazard entity type")
            }
        }

        Self {
            bbox,
            centroid,
            radius,
            uni_haz_prox: static_uni_haz_prox.clone(),
            static_uni_haz_prox,
            qz_haz_prox,
        }
    }

    pub fn register_hazards<'a, I>(&mut self, to_register: I)
        where I: Iterator<Item=&'a Hazard>
    {
        //For each item to register, calculate the distance from the cell to its bounding circle of the poles.
        //negative distance if inside of circle.
        //This serves as an lowerbound for the distance to the item itself.
        let mut bounding_pole_distances: Vec<(&Hazard, Option<Proximity>)> = to_register
            .filter(|haz| haz.is_active())
            .map(|haz| {
                match haz.entity().presence() {
                    GeoPosition::Exterior => (haz, None), //bounding poles only applicable for hazard inside the shape
                    GeoPosition::Interior => {
                        let pole_bounding_circle = haz.shape().surrogate().poles_bounding_circle();
                        let proximity = pole_bounding_circle.distance_from_border(&self.centroid);
                        let proximity = Proximity::new(proximity.0, proximity.1.abs());
                        (haz, Some(proximity))
                    }
                }
            }).collect();

        //Go over the items in order of the closest bounding circle
        while !bounding_pole_distances.is_empty() {
            let (index, (to_register, bounding_proximity)) = bounding_pole_distances.iter().enumerate()
                .min_by_key(|(_, (_, d))| d).unwrap();

            let current_proximity = self.universal_hazard_proximity().0;

            match bounding_proximity {
                None => {
                    self.register_hazard(to_register);
                    bounding_pole_distances.swap_remove(index);
                }
                Some(bounding_prox) => {
                    if bounding_prox <= &current_proximity {
                        //bounding circle is closer than current closest hazard, potentially affecting this cell
                        self.register_hazard(to_register);
                        bounding_pole_distances.swap_remove(index);
                    } else {
                        //bounding circle is further away than current closest.
                        //This, and all following items (which are further away) do not modify this cell
                        break;
                    }
                }
            }
        }
    }

    pub fn register_hazard(&mut self, to_register: &Hazard) -> HPCellUpdate {
        let current_prox = self.universal_hazard_proximity().0;

        //For dynamic hazards, the surrogate poles are used to calculate the distance to the hazard (overestimation, but fast)
        let haz_prox = match to_register.entity().presence() {
            GeoPosition::Interior => distance_to_surrogate_poles_border(self, to_register.shape().surrogate().poles()),
            GeoPosition::Exterior => unreachable!("No implementation yet for dynamic exterior hazards")
        };

        match haz_prox.cmp(&current_prox) {
            Ordering::Less => {
                //new hazard is closer
                self.uni_haz_prox = (haz_prox, to_register.entity().clone());
                HPCellUpdate::Affected
            }
            _ => {
                if haz_prox.distance_from_border > current_prox.distance_from_border + 2.0 * self.radius {
                    HPCellUpdate::Boundary
                } else {
                    HPCellUpdate::Unaffected
                }
            }
        }
    }

    pub fn deregister_hazards<'a, 'b, I, J>(&mut self, mut to_deregister: J, remaining: I) -> HPCellUpdate
        where I: Iterator<Item=&'a Hazard>, J: Iterator<Item=&'b HazardEntity>
    {
        if to_deregister.contains(&self.uni_haz_prox.1) {
            //closest current hazard has to be deregistered
            self.uni_haz_prox = self.static_uni_haz_prox.clone();

            self.register_hazards(remaining);
            HPCellUpdate::Affected
        } else {
            HPCellUpdate::Unaffected
        }
    }

    pub fn bbox(&self) -> &AARectangle {
        &self.bbox
    }

    pub fn radius(&self) -> f64 {
        self.radius
    }

    pub fn centroid(&self) -> Point {
        self.centroid
    }

    pub fn value(&self) -> f64 {
        //TODO: needs to move to heuristic
        Self::calculate_value(
            self.uni_haz_prox.0,
            self.qz_haz_prox,
        )
    }

    pub fn calculate_value(uni_haz_prox: Proximity, qz_prox: [Proximity; N_QUALITIES]) -> f64 {
        let value = uni_haz_prox.distance_from_border.powi(2); // value proportional with area instead of radius

        let centroid_in_qz = qz_prox.iter()
            .position(|qz_prox| qz_prox.position == GeoPosition::Interior);

        let quality_correction_factor = match centroid_in_qz {
            Some(quality) => (quality as f64 / N_QUALITIES as f64),
            None => 1.0,
        };

        value * quality_correction_factor
    }

    pub fn value_loss(&self, new_hazard_proximity: Proximity) -> (Option<f64>, HPCellUpdate) {
        match new_hazard_proximity.partial_cmp(&self.uni_haz_prox.0).unwrap() {
            Ordering::Less => {
                //new hazard is closer than current (avoiding floating point precision errors)
                let new_value = Self::calculate_value(
                    new_hazard_proximity,
                    self.qz_haz_prox,
                );

                (Some(self.value() - new_value), HPCellUpdate::Affected)
            }
            _ => {
                //no value loss and the cell might be a boundary cell
                match new_hazard_proximity.distance_from_border > self.uni_haz_prox.0.distance_from_border + 2.0 * self.radius {
                    true => (None, HPCellUpdate::Boundary),
                    false => (None, HPCellUpdate::Unaffected)
                }
            }
        }
    }

    pub fn hazard_proximity(&self, quality_level: Option<usize>) -> Proximity {
        //calculate the minimum distance to either bin, item or qz
        let mut haz_prox = self.uni_haz_prox.0;
        let relevant_qualities = match quality_level {
            Some(quality_level) => 0..quality_level,
            None => 0..N_QUALITIES
        };

        for quality in relevant_qualities {
            haz_prox = haz_prox.min(self.qz_haz_prox[quality]);
        }
        haz_prox
    }

    pub fn universal_hazard_proximity(&self) -> &(Proximity, HazardEntity) {
        &self.uni_haz_prox
    }
    pub fn bin_haz_prox(&self) -> Proximity {
        self.static_uni_haz_prox.0
    }
    pub fn qz_haz_prox(&self) -> [Proximity; 10] {
        self.qz_haz_prox
    }


    pub fn static_uni_haz_prox(&self) -> &(Proximity, HazardEntity) {
        &self.static_uni_haz_prox
    }
}


pub fn distance_to_surrogate_poles_border(hp_cell: &HPCell, poles: &[Circle]) -> Proximity {
    poles.iter()
        .map(|p| p.distance_from_border(&hp_cell.centroid))
        .map(|(pos, dist)| Proximity::new(pos, dist.abs()))
        .min().unwrap()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HPCellUpdate {
    Affected,
    Unaffected,
    Boundary, //Unaffected and its neighbors are also guaranteed to be unaffected
}