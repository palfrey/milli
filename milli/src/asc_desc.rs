//! This module provides the `AscDesc` type and defines all the errors related to this type.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::is_reserved_keyword;
use crate::{CriterionError, Error, UserError};

/// This error type is never supposed to be shown to the end user.
/// You must always cast it to a sort error or a criterion error.
#[derive(Debug)]
pub enum AscDescError {
    InvalidLatitude,
    InvalidLongitude,
    InvalidSyntax { name: String },
    ReservedKeyword { name: String },
}

impl fmt::Display for AscDescError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::InvalidLatitude => {
                write!(f, "Latitude must be contained between -90 and 90 degrees.",)
            }
            Self::InvalidLongitude => {
                write!(f, "Longitude must be contained between -180 and 180 degrees.",)
            }
            Self::InvalidSyntax { name } => {
                write!(f, "invalid asc/desc syntax for {}.", name)
            }
            Self::ReservedKeyword { name } => {
                write!(
                    f,
                    "{} is a reserved keyword and thus can't be used as a asc/desc rule.",
                    name
                )
            }
        }
    }
}

impl From<AscDescError> for CriterionError {
    fn from(error: AscDescError) -> Self {
        match error {
            AscDescError::InvalidLatitude | AscDescError::InvalidLongitude => {
                CriterionError::ReservedNameForSort { name: "_geoPoint".to_string() }
            }
            AscDescError::InvalidSyntax { name } => CriterionError::InvalidName { name },
            AscDescError::ReservedKeyword { name } if name.starts_with("_geoPoint") => {
                CriterionError::ReservedNameForSort { name: "_geoPoint".to_string() }
            }
            AscDescError::ReservedKeyword { name } if name.starts_with("_geoRadius") => {
                CriterionError::ReservedNameForFilter { name: "_geoRadius".to_string() }
            }
            AscDescError::ReservedKeyword { name } => CriterionError::ReservedName { name },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum Member {
    Field(String),
    Geo([f64; 2]),
}

impl FromStr for Member {
    type Err = AscDescError;

    fn from_str(text: &str) -> Result<Member, Self::Err> {
        match text.strip_prefix("_geoPoint(").and_then(|text| text.strip_suffix(")")) {
            Some(point) => {
                let (lat, lng) = point
                    .split_once(',')
                    .ok_or_else(|| AscDescError::ReservedKeyword { name: text.to_string() })
                    .and_then(|(lat, lng)| {
                        lat.trim()
                            .parse()
                            .and_then(|lat| lng.trim().parse().map(|lng| (lat, lng)))
                            .map_err(|_| AscDescError::ReservedKeyword { name: text.to_string() })
                    })?;
                if !(-90.0..=90.0).contains(&lat) {
                    return Err(AscDescError::InvalidLatitude)?;
                } else if !(-180.0..=180.0).contains(&lng) {
                    return Err(AscDescError::InvalidLongitude)?;
                }
                Ok(Member::Geo([lat, lng]))
            }
            None => {
                if is_reserved_keyword(text) || text.starts_with("_geoRadius(") {
                    return Err(AscDescError::ReservedKeyword { name: text.to_string() })?;
                }
                Ok(Member::Field(text.to_string()))
            }
        }
    }
}

impl fmt::Display for Member {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Member::Field(name) => f.write_str(name),
            Member::Geo([lat, lng]) => write!(f, "_geoPoint({}, {})", lat, lng),
        }
    }
}

impl Member {
    pub fn field(&self) -> Option<&str> {
        match self {
            Member::Field(field) => Some(field),
            Member::Geo(_) => None,
        }
    }

    pub fn geo_point(&self) -> Option<&[f64; 2]> {
        match self {
            Member::Geo(point) => Some(point),
            Member::Field(_) => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum AscDesc {
    Asc(Member),
    Desc(Member),
}

impl AscDesc {
    pub fn member(&self) -> &Member {
        match self {
            AscDesc::Asc(member) => member,
            AscDesc::Desc(member) => member,
        }
    }

    pub fn field(&self) -> Option<&str> {
        self.member().field()
    }
}

impl FromStr for AscDesc {
    type Err = AscDescError;

    fn from_str(text: &str) -> Result<AscDesc, Self::Err> {
        match text.rsplit_once(':') {
            Some((left, "asc")) => Ok(AscDesc::Asc(left.parse()?)),
            Some((left, "desc")) => Ok(AscDesc::Desc(left.parse()?)),
            _ => Err(AscDescError::InvalidSyntax { name: text.to_string() }),
        }
    }
}

#[derive(Debug)]
pub enum SortError {
    InvalidLatitude,
    InvalidLongitude,
    BadGeoPointUsage { name: String },
    InvalidName { name: String },
    ReservedName { name: String },
    ReservedNameForSettings { name: String },
    ReservedNameForFilter { name: String },
}

impl From<AscDescError> for SortError {
    fn from(error: AscDescError) -> Self {
        match error {
            AscDescError::InvalidLatitude => SortError::InvalidLatitude,
            AscDescError::InvalidLongitude => SortError::InvalidLongitude,
            AscDescError::InvalidSyntax { name } => SortError::InvalidName { name },
            AscDescError::ReservedKeyword { name } if name.starts_with("_geoPoint") => {
                SortError::BadGeoPointUsage { name }
            }
            AscDescError::ReservedKeyword { name } if &name == "_geo" => {
                SortError::ReservedNameForSettings { name }
            }
            AscDescError::ReservedKeyword { name } if name.starts_with("_geoRadius") => {
                SortError::ReservedNameForFilter { name: String::from("_geoRadius") }
            }
            AscDescError::ReservedKeyword { name } => SortError::ReservedName { name },
        }
    }
}

impl fmt::Display for SortError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::InvalidLatitude => write!(f, "{}", AscDescError::InvalidLatitude),
            Self::InvalidLongitude => write!(f, "{}", AscDescError::InvalidLongitude),
            Self::BadGeoPointUsage { name } => {
                write!(
                    f,
                    "invalid syntax for the `_geoPoint` parameter: `{}`. \
                    Usage: `_geoPoint(latitude, longitude):asc`.",
                    name
                )
            }
            Self::InvalidName { name } => {
                write!(f, "invalid syntax for the sort parameter `{}`.", name)
            }
            Self::ReservedName { name } => {
                write!(
                    f,
                    "{} is a reserved keyword and thus can't be used as a sort expression.",
                    name
                )
            }
            Self::ReservedNameForSettings { name } | Self::ReservedNameForFilter { name } => {
                write!(
                    f,
                    "`{}` is a reserved keyword and thus can't be used as a sort expression. \
                    Use the `_geoPoint(latitude, longitude)` built-in rule to sort on `_geo` field coordinates.",
                    name,
                )
            }
        }
    }
}

impl From<SortError> for Error {
    fn from(error: SortError) -> Self {
        Self::UserError(UserError::SortError(error))
    }
}

#[cfg(test)]
mod tests {
    use big_s::S;
    use AscDesc::*;
    use AscDescError::*;
    use Member::*;

    use super::*;

    #[test]
    fn parse_asc_desc() {
        let valid_req = [
            ("truc:asc", Asc(Field(S("truc")))),
            ("bidule:desc", Desc(Field(S("bidule")))),
            ("a-b:desc", Desc(Field(S("a-b")))),
            ("a:b:desc", Desc(Field(S("a:b")))),
            ("a12:asc", Asc(Field(S("a12")))),
            ("42:asc", Asc(Field(S("42")))),
            ("_geoPoint(42, 59):asc", Asc(Geo([42., 59.]))),
            ("_geoPoint(42.459, 59):desc", Desc(Geo([42.459, 59.]))),
            ("_geoPoint(42, 59.895):desc", Desc(Geo([42., 59.895]))),
            ("_geoPoint(42, 59.895):desc", Desc(Geo([42., 59.895]))),
            ("_geoPoint(90.000000000, 180):desc", Desc(Geo([90., 180.]))),
            ("_geoPoint(-90, -180.0000000000):asc", Asc(Geo([-90., -180.]))),
            ("_geoPoint(42.0002, 59.895):desc", Desc(Geo([42.0002, 59.895]))),
            ("_geoPoint(42., 59.):desc", Desc(Geo([42., 59.]))),
            ("truc(12, 13):desc", Desc(Field(S("truc(12, 13)")))),
        ];

        for (req, expected) in valid_req {
            let res = req.parse::<AscDesc>();
            assert!(
                res.is_ok(),
                "Failed to parse `{}`, was expecting `{:?}` but instead got `{:?}`",
                req,
                expected,
                res
            );
            assert_eq!(res.unwrap(), expected);
        }

        let invalid_req = [
            ("truc:machin", InvalidSyntax { name: S("truc:machin") }),
            ("truc:deesc", InvalidSyntax { name: S("truc:deesc") }),
            ("truc:asc:deesc", InvalidSyntax { name: S("truc:asc:deesc") }),
            ("42desc", InvalidSyntax { name: S("42desc") }),
            ("_geoPoint:asc", ReservedKeyword { name: S("_geoPoint") }),
            ("_geoDistance:asc", ReservedKeyword { name: S("_geoDistance") }),
            ("_geoPoint(42.12 , 59.598)", InvalidSyntax { name: S("_geoPoint(42.12 , 59.598)") }),
            (
                "_geoPoint(42.12 , 59.598):deesc",
                InvalidSyntax { name: S("_geoPoint(42.12 , 59.598):deesc") },
            ),
            (
                "_geoPoint(42.12 , 59.598):machin",
                InvalidSyntax { name: S("_geoPoint(42.12 , 59.598):machin") },
            ),
            (
                "_geoPoint(42.12 , 59.598):asc:aasc",
                InvalidSyntax { name: S("_geoPoint(42.12 , 59.598):asc:aasc") },
            ),
            (
                "_geoPoint(42,12 , 59,598):desc",
                ReservedKeyword { name: S("_geoPoint(42,12 , 59,598)") },
            ),
            ("_geoPoint(35, 85, 75):asc", ReservedKeyword { name: S("_geoPoint(35, 85, 75)") }),
            ("_geoPoint(18):asc", ReservedKeyword { name: S("_geoPoint(18)") }),
            ("_geoPoint(200, 200):asc", InvalidLatitude),
            ("_geoPoint(90.000001, 0):asc", InvalidLatitude),
            ("_geoPoint(0, -180.000001):desc", InvalidLongitude),
            ("_geoPoint(159.256, 130):asc", InvalidLatitude),
            ("_geoPoint(12, -2021):desc", InvalidLongitude),
        ];

        for (req, expected_error) in invalid_req {
            let res = req.parse::<AscDesc>();
            assert!(
                res.is_err(),
                "Should no be able to parse `{}`, was expecting an error but instead got: `{:?}`",
                req,
                res,
            );
            let res = res.unwrap_err();
            assert_eq!(
                res.to_string(),
                expected_error.to_string(),
                "Bad error for input {}: got `{:?}` instead of `{:?}`",
                req,
                res,
                expected_error
            );
        }
    }

    #[test]
    fn sort_error_message() {
        let errors = [
            (
                AscDescError::InvalidSyntax { name: S("truc:machin") },
                S("invalid syntax for the sort parameter `truc:machin`."),
            ),
            (
                AscDescError::InvalidSyntax { name: S("hello:world") },
                S("invalid syntax for the sort parameter `hello:world`."),
            ),
            (
                AscDescError::ReservedKeyword { name: S("_geo") },
                S("`_geo` is a reserved keyword and thus can't be used as a sort expression. Use the `_geoPoint(latitude, longitude)` built-in rule to sort on `_geo` field coordinates."),
            ),
            (
                AscDescError::ReservedKeyword { name: S("_geoDistance") },
                S("_geoDistance is a reserved keyword and thus can't be used as a sort expression.")
            ),
            (
                AscDescError::ReservedKeyword { name: S("_geoRadius(12, 13)") },
                S("`_geoRadius` is a reserved keyword and thus can't be used as a sort expression. Use the `_geoPoint(latitude, longitude)` built-in rule to sort on `_geo` field coordinates."),
            ),
            (
                AscDescError::InvalidLatitude,
                S("Latitude must be contained between -90 and 90 degrees."),
            ),
            (
                AscDescError::InvalidLongitude,
                S("Longitude must be contained between -180 and 180 degrees."),
            ),
        ];

        for (asc_desc_error, expected_message) in errors {
            let sort_error = SortError::from(asc_desc_error);
            assert_eq!(
                sort_error.to_string(),
                expected_message,
                "was expecting {} for the error {:?} but instead got {}",
                expected_message,
                sort_error,
                sort_error.to_string()
            );
        }
    }
}
